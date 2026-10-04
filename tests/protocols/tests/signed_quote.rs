//! `signedQuoteSettlement` against the real programs: a maker's quote, signed with the maker's
//! wallet key, settled in wrapped SOL for USDC through mainnet's Token program.
//!
//! Each transaction is the one the docs describe: the Ed25519 precompile instruction, carrying the
//! maker's key, the signature and the 128-byte quote, directly before the run, which reads it
//! through the Instructions sysvar. The taker pays the fee and signs; the maker co-signs. LiteSVM
//! verifies the signature as mainnet does (its `precompiles` feature), so a bad one fails the
//! transaction before Ballista runs.
//!
//! [`Quote::message`] and [`ed25519_instruction`] are ported from the Rust runner,
//! `clients/rust/examples/protocol_templates_run.rs#signed-quote`.

use {
    ballista_protocol_tests::{
        snapshot::{warp, Snapshot, SNAPSHOT_DIR},
        template::{examples, upload, Example, Run},
        tx::{
            self, assert_ballista_failure, assert_requirement_failed, ballista_error, Failure,
            Outcome,
        },
        wallet::{create_token_account, fund, keypair, token_account, token_balance, SOL},
    },
    ballista_sdk::{ED25519_PROGRAM_ID, INSTRUCTIONS_SYSVAR_ID, TOKEN_PROGRAM_ID},
    litesvm::LiteSVM,
    solana_address::Address,
    solana_clock::Clock,
    solana_compute_budget_interface::ComputeBudgetInstruction,
    solana_ed25519_program::new_ed25519_instruction_with_signature,
    solana_instruction::Instruction,
    solana_keypair::Keypair,
    solana_precompile_error::PrecompileError,
    solana_signer::Signer,
    solana_transaction::{InstructionError, TransactionError},
};

const TEMPLATE: &str = "signedQuoteSettlement";
const TEMPLATE_ID: u16 = 21;
/// Prices carry six decimals: quote-token base units per 1,000,000 base-token base units.
const PRICE_SCALE: u64 = 1_000_000;
/// 150.25 USDC a SOL: 150,250,000 USDC units per 10^9 lamports, so 150,250 per 10^6.
const PRICE: u64 = 150_250;
/// The most the maker sells on one quote: 2 SOL.
const MAX_AMOUNT: u64 = 2 * SOL;
/// How long a quote stands, in seconds.
const WINDOW: i64 = 60;
/// What the taker holds to pay with: 1,000 USDC.
const TAKER_USDC: u64 = 1_000_000_000;
/// What the maker holds to deliver: 10 SOL, wrapped.
const MAKER_WSOL: u64 = 10 * SOL;
/// 1.5 SOL and a lamport, which at `PRICE` comes to 225,375,000.15025 USDC units.
const AMOUNT: u64 = 3 * SOL / 2 + 1;
/// Where the Ed25519 instruction's data holds the message: after the signature count and its
/// padding byte, one signature's seven offsets, the 32-byte key and the 64-byte signature.
const MESSAGE_START: usize = 2 + 14 + 32 + 64;
/// The price's offset in the quote.
const QUOTE_PRICE: usize = 8;
/// What LiteSVM, like mainnet, charges for each signature a transaction carries.
const LAMPORTS_PER_SIGNATURE: u64 = 5_000;

/// The quote a maker signs off chain, as `signed-quote-settlement.ts` reads it.
#[derive(Clone, Copy, Debug)]
struct Quote {
    /// Quote-token base units per 1,000,000 base-token base units.
    price: u64,
    /// The most base-token base units the maker delivers.
    max_amount: u64,
    /// The last Unix timestamp at which the quote can settle.
    expiry: i64,
    /// The one wallet that can take the quote.
    taker: Address,
    /// The mint the maker delivers, and the mint the taker pays in.
    base_mint: Address,
    quote_mint: Address,
}

impl Quote {
    /// The 128 bytes the maker signs: the tag `BLSTQT01`, then the fields, integers
    /// little-endian.
    fn message(&self) -> Vec<u8> {
        let mut message = Vec::with_capacity(128);
        message.extend_from_slice(b"BLSTQT01");
        message.extend_from_slice(&self.price.to_le_bytes());
        message.extend_from_slice(&self.max_amount.to_le_bytes());
        message.extend_from_slice(&self.expiry.to_le_bytes());
        for key in [self.taker, self.base_mint, self.quote_mint] {
            message.extend_from_slice(key.as_ref());
        }
        message
    }
}

/// The Ed25519 precompile instruction that verifies `signature`, `signer`'s over `message`. It
/// holds one signature, with the key, the signature and the message in its own data, laid out as
/// `new_ed25519_instruction_with_signature` lays them out.
fn ed25519_instruction(signer: &Address, signature: &[u8; 64], message: &[u8]) -> Instruction {
    // `u16::MAX` as an instruction index: the bytes are in this instruction's own data.
    const OWN_DATA: u16 = u16::MAX;
    let key_offset: u16 = 2 + 14;
    let signature_offset = key_offset + 32;
    let message_offset = signature_offset + 64;
    let mut data = vec![1, 0]; // one signature, then a padding byte
    for field in [
        signature_offset,
        OWN_DATA,
        key_offset,
        OWN_DATA,
        message_offset,
        message.len() as u16,
        OWN_DATA,
    ] {
        data.extend_from_slice(&field.to_le_bytes());
    }
    data.extend_from_slice(signer.as_ref());
    data.extend_from_slice(signature);
    data.extend_from_slice(message);
    Instruction {
        program_id: ED25519_PROGRAM_ID,
        accounts: vec![],
        data,
    }
}

/// `signer`'s Ed25519 instruction over `message`, signed with its wallet key: a Solana keypair is
/// an Ed25519 key, and `sign_message` signs the bytes as they are.
fn signed_by(signer: &Keypair, message: &[u8]) -> Instruction {
    let signature: [u8; 64] = signer
        .sign_message(message)
        .as_ref()
        .try_into()
        .expect("an Ed25519 signature is 64 bytes");
    ed25519_instruction(&signer.pubkey(), &signature, message)
}

/// What the template charges for `amount` at `price`: `amount × price ÷ 1,000,000`, rounded up in
/// the maker's favour.
fn payment(amount: u64, price: u64) -> u64 {
    let exact = u128::from(amount) * u128::from(price);
    u64::try_from(exact.div_ceil(u128::from(PRICE_SCALE))).expect("the payment fits in a u64")
}

/// The four token accounts' balances.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Balances {
    taker_usdc: u64,
    maker_usdc: u64,
    maker_wsol: u64,
    taker_wsol: u64,
}

impl Balances {
    /// These balances once the taker has paid `paid` and the maker delivered `delivered`.
    fn after(self, paid: u64, delivered: u64) -> Balances {
        Balances {
            taker_usdc: self.taker_usdc - paid,
            maker_usdc: self.maker_usdc + paid,
            maker_wsol: self.maker_wsol - delivered,
            taker_wsol: self.taker_wsol + delivered,
        }
    }
}

/// A fresh SVM with the template uploaded and each side's two token accounts written (write
/// rule 1): the taker holds USDC to pay with, the maker wrapped SOL to deliver.
struct Market {
    svm: LiteSVM,
    template: Address,
    maker: Keypair,
    taker: Keypair,
    usdc: Address,
    wsol: Address,
    taker_usdc: Address,
    maker_usdc: Address,
    maker_wsol: Address,
    taker_wsol: Address,
}

impl Market {
    fn new(snapshot: &Snapshot, example: &Example) -> Market {
        let mut svm = snapshot.svm();
        let usdc = snapshot.named("usdcMint");
        let wsol = snapshot.named("wsolMint");
        let creator = keypair(b"ballista-protocol-tests-creator1");
        let maker = keypair(b"ballista-protocol-tests-maker-01");
        let taker = keypair(b"ballista-protocol-tests-taker-01");
        for key in [creator.pubkey(), maker.pubkey(), taker.pubkey()] {
            fund(&mut svm, &key, 10 * SOL);
        }
        let template = upload(&mut svm, &creator, TEMPLATE_ID, &example.payload);
        let taker_usdc = token_account(&mut svm, &taker.pubkey(), &usdc, TAKER_USDC);
        let maker_usdc = token_account(&mut svm, &maker.pubkey(), &usdc, 0);
        let maker_wsol = token_account(&mut svm, &maker.pubkey(), &wsol, MAKER_WSOL);
        let taker_wsol = token_account(&mut svm, &taker.pubkey(), &wsol, 0);
        Market {
            svm,
            template,
            maker,
            taker,
            usdc,
            wsol,
            taker_usdc,
            maker_usdc,
            maker_wsol,
            taker_wsol,
        }
    }

    fn now(&self) -> i64 {
        self.svm.get_sysvar::<Clock>().unix_timestamp
    }

    /// Moves the clock on by `seconds`, and the slot at mainnet's 400 ms a slot (write rule 3).
    fn wait(&mut self, seconds: u64) {
        warp(&mut self.svm, seconds * 5 / 2, seconds);
    }

    /// The quote a maker would give this taker: wrapped SOL for USDC at 150.25, up to 2 SOL, for a
    /// minute from now.
    fn quote(&self) -> Quote {
        Quote {
            price: PRICE,
            max_amount: MAX_AMOUNT,
            expiry: self.now() + WINDOW,
            taker: self.taker.pubkey(),
            base_mint: self.wsol,
            quote_mint: self.usdc,
        }
    }

    /// The run, taking `amount` and paying into `payee`; the other accounts are the market's own.
    fn run(&self, example: &Example, payee: Address, amount: u64) -> Instruction {
        Run::new(self.template, example)
            .account("instructions", INSTRUCTIONS_SYSVAR_ID, false, false)
            .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
            .account("taker", self.taker.pubkey(), false, true)
            .account("maker", self.maker.pubkey(), false, true)
            .account("takerQuoteAccount", self.taker_usdc, true, false)
            .account("makerQuoteAccount", payee, true, false)
            .account("makerBaseAccount", self.maker_wsol, true, false)
            .account("takerBaseAccount", self.taker_wsol, true, false)
            .input_u64("amount", amount)
            .build()
    }

    /// Sends `instructions`: the taker pays the fee and signs, and the maker co-signs.
    fn send(&mut self, instructions: &[Instruction]) -> Result<Outcome, Failure> {
        tx::send(
            &mut self.svm,
            &self.taker,
            &[&self.maker],
            instructions,
            &[],
        )
    }

    /// Settles `quote` as the docs build it: the maker's Ed25519 instruction over the quote, then
    /// the run, taking `amount` and paying the maker's own USDC account.
    fn settle(
        &mut self,
        example: &Example,
        quote: &Quote,
        amount: u64,
    ) -> Result<Outcome, Failure> {
        let signed = signed_by(&self.maker, &quote.message());
        let run = self.run(example, self.maker_usdc, amount);
        self.send(&[signed, run])
    }

    /// Sends `instructions`, which must fail, and checks that no balance moved. Prints where it
    /// failed and Ballista's units, for the findings.
    fn refuse(&mut self, instructions: &[Instruction]) -> Failure {
        let before = self.balances();
        let failure = self
            .send(instructions)
            .expect_err("the transaction should have failed");
        assert_eq!(self.balances(), before, "a failed transaction moved tokens");
        println!("refused: {}", summary(&failure));
        failure
    }

    fn balances(&self) -> Balances {
        Balances {
            taker_usdc: token_balance(&self.svm, &self.taker_usdc),
            maker_usdc: token_balance(&self.svm, &self.maker_usdc),
            maker_wsol: token_balance(&self.svm, &self.maker_wsol),
            taker_wsol: token_balance(&self.svm, &self.taker_wsol),
        }
    }

    /// The two wrapped-SOL accounts' lamports, maker's then taker's.
    fn wsol_lamports(&self) -> (u64, u64) {
        let lamports = |account| self.svm.get_balance(account).expect("the account exists");
        (lamports(&self.maker_wsol), lamports(&self.taker_wsol))
    }
}

/// One line on a failure: the program, the error, Ballista's own name and program counter when it
/// raised one, and the units Ballista's run consumed, if it ran.
fn summary(failure: &Failure) -> String {
    let consumed = format!("Program {} consumed ", ballista_sdk::ID);
    let units = failure
        .logs
        .iter()
        .find_map(|line| line.strip_prefix(&consumed))
        .unwrap_or("nothing: it never ran");
    format!(
        "{} failed with {:?}, Ballista's {:?}; Ballista consumed {units}",
        failure.program,
        failure.err,
        ballista_error(failure)
    )
}

/// Asserts that the Ed25519 precompile, the transaction's instruction `index`, failed it with
/// `error`, and that Ballista never ran.
#[track_caller]
fn assert_precompile_failed(failure: &Failure, index: u8, error: PrecompileError) {
    let code = error.clone() as u32;
    assert!(
        failure.program == ED25519_PROGRAM_ID
            && failure.code == Some(code)
            && failure.err == TransactionError::InstructionError(index, InstructionError::Custom(code)),
        "expected the Ed25519 precompile, instruction {index}, to fail with {error:?}, but {failure:?}"
    );
    let ballista_ran = format!("Program {} invoke", ballista_sdk::ID);
    assert!(
        !failure
            .logs
            .iter()
            .any(|line| line.starts_with(&ballista_ran)),
        "Ballista ran after the precompile failed: {failure:?}"
    );
}

/// The quote as the docs settle it: 1.5 SOL and a lamport at 150.25 USDC a SOL. Each side's
/// balances move by exactly the quoted amounts, the price rounded up in the maker's favour, and the
/// wrapped SOL's lamports move with its balance.
#[test]
fn an_honest_quote_settles_at_the_signed_price_rounded_up() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut market = Market::new(&snapshot, example);
    let quote = market.quote();
    let paid = payment(AMOUNT, PRICE);
    // 225,375,000.15025 USDC units: the maker gets the fraction.
    assert_eq!(
        (AMOUNT * PRICE / PRICE_SCALE, paid),
        (225_375_000, 225_375_001)
    );

    let before = market.balances();
    let (maker_lamports, taker_lamports) = market.wsol_lamports();
    let outcome = market
        .settle(example, &quote, AMOUNT)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(market.balances(), before.after(paid, AMOUNT));
    assert_eq!(
        market.wsol_lamports(),
        (maker_lamports - AMOUNT, taker_lamports + AMOUNT)
    );
    println!(
        "settled {AMOUNT} lamports for {paid} USDC units: {} CU, {} in Ballista's run, {} of them \
         its own, {} bytes, fee {}",
        outcome.compute_units,
        outcome.compute_units_of(&ballista_sdk::ID).unwrap(),
        outcome.own_compute_units_of(&ballista_sdk::ID).unwrap(),
        outcome.size,
        outcome.fee,
    );
}

/// Where the Ed25519 instruction's cost shows. Not in compute: it logs nothing, the transaction's
/// units are all Ballista's run, its two token transfers included, and alone in a transaction it
/// lands having consumed none. In the fee: its signature is charged as the transaction's own two
/// are. And in size: 276 of the settlement's 783 bytes are its program's key and the instruction.
#[test]
fn the_precompile_costs_a_signature_s_fee_and_no_compute() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut market = Market::new(&snapshot, example);
    let quote = market.quote();

    let outcome = market
        .settle(example, &quote, AMOUNT)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    let run_units = outcome.compute_units_of(&ballista_sdk::ID).unwrap();
    let limit = outcome
        .logs
        .iter()
        .find_map(|line| line.strip_prefix(&format!("Program {} consumed ", ballista_sdk::ID)))
        .expect("Ballista logged its units");
    let transfers: Vec<&str> = outcome
        .logs
        .iter()
        .filter_map(|line| line.strip_prefix(&format!("Program {TOKEN_PROGRAM_ID} consumed ")))
        .collect();
    println!(
        "settlement: {} CU, Ballista's run {limit}, the transfers {transfers:?}; {} bytes; fee {}",
        outcome.compute_units, outcome.size, outcome.fee
    );
    let ed25519 = ED25519_PROGRAM_ID.to_string();
    assert!(
        !outcome.logs.iter().any(|line| line.contains(&ed25519)),
        "{outcome:?}"
    );
    assert_eq!(outcome.compute_units, run_units, "{outcome:?}");
    assert_eq!(outcome.fee, 3 * LAMPORTS_PER_SIGNATURE, "{outcome:?}");
    assert_eq!(outcome.size, 783);

    let signed = signed_by(&market.maker, &quote.message());
    // The key, the signature, the quote and the 16 bytes before them.
    assert_eq!(signed.data.len(), 240);
    let alone = tx::send(&mut market.svm, &market.taker, &[], &[signed], &[])
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    println!(
        "the Ed25519 instruction alone: {} CU, {} bytes, fee {}",
        alone.compute_units, alone.size, alone.fee
    );
    assert!(alone.logs.is_empty(), "{alone:?}");
    assert_eq!(alone.compute_units, 0);
    assert_eq!(alone.fee, 2 * LAMPORTS_PER_SIGNATURE);
    assert_eq!(alone.size, 410);
}

/// The ported instruction is byte for byte the one Solana's own builder,
/// `new_ed25519_instruction_with_signature`, makes: the layout the runner's comment names, and
/// the one the template's header check expects.
#[test]
fn the_ported_ed25519_instruction_is_solana_s_own_layout() {
    let maker = keypair(b"ballista-protocol-tests-maker-01");
    let quote = Quote {
        price: PRICE,
        max_amount: MAX_AMOUNT,
        expiry: 1_800_000_000,
        taker: keypair(b"ballista-protocol-tests-taker-01").pubkey(),
        base_mint: Address::new_from_array([1; 32]),
        quote_mint: Address::new_from_array([2; 32]),
    };
    let message = quote.message();
    assert_eq!(message.len(), 128);
    let signed = signed_by(&maker, &message);
    let signature: [u8; 64] = signed.data[2 + 14 + 32..MESSAGE_START].try_into().unwrap();
    assert_eq!(
        signed,
        new_ed25519_instruction_with_signature(&message, &signature, &maker.pubkey().to_bytes())
    );
}

/// A quote signed by a key other than the maker's. The precompile verifies it, since the signature
/// is good for the key it carries, and the run refuses it at `quoteIsBySigner`. The maker still
/// co-signs the transaction: only the quote's signer is wrong.
#[test]
fn a_quote_signed_by_another_key_fails_at_quote_is_by_signer() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut market = Market::new(&snapshot, example);
    let stranger = keypair(b"ballista-protocol-tests-stranger");
    let signed = signed_by(&stranger, &market.quote().message());
    let run = market.run(example, market.maker_usdc, AMOUNT);
    let failure = market.refuse(&[signed, run]);
    assert_requirement_failed(&failure, example, "quoteIsBySigner");
}

/// A signature that does not match the bytes and the key it comes with fails the Ed25519
/// precompile itself, at instruction 0, with `InvalidSignature`; Ballista never runs. Two ways:
/// the price lowered by one unit after the maker signed, and the maker's key carrying a stranger's
/// signature over the true quote.
#[test]
fn a_message_changed_after_signing_fails_in_the_precompile() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut market = Market::new(&snapshot, example);
    let message = market.quote().message();
    let run = market.run(example, market.maker_usdc, AMOUNT);

    let mut repriced = signed_by(&market.maker, &message);
    assert_eq!(
        repriced.data[MESSAGE_START..MESSAGE_START + 16],
        message[..16],
        "the message is where the offsets say"
    );
    repriced.data[MESSAGE_START + QUOTE_PRICE] -= 1;
    let failure = market.refuse(&[repriced, run.clone()]);
    assert_precompile_failed(&failure, 0, PrecompileError::InvalidSignature);

    let stranger = keypair(b"ballista-protocol-tests-stranger");
    let by_stranger = signed_by(&stranger, &message);
    let signature: [u8; 64] = by_stranger.data[2 + 14 + 32..MESSAGE_START]
        .try_into()
        .unwrap();
    let misattributed = ed25519_instruction(&market.maker.pubkey(), &signature, &message);
    let failure = market.refuse(&[misattributed, run]);
    assert_precompile_failed(&failure, 0, PrecompileError::InvalidSignature);
}

/// The maker's signature over bytes of the quote's shape under another tag, as if signed for
/// something else: the precompile verifies it, and the run refuses it at `quoteIsTagged`.
#[test]
fn a_message_under_another_tag_fails_at_quote_is_tagged() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut market = Market::new(&snapshot, example);
    let mut message = market.quote().message();
    message[..8].copy_from_slice(b"BLSTQT02");
    let signed = signed_by(&market.maker, &message);
    let run = market.run(example, market.maker_usdc, AMOUNT);
    let failure = market.refuse(&[signed, run]);
    assert_requirement_failed(&failure, example, "quoteIsTagged");
}

/// The expiry is the last second a quote settles. With the clock moved on to exactly the expiry
/// (write rule 3) it lands; one second later it fails at `quoteHasNotExpired`.
#[test]
fn a_quote_settles_until_its_expiry_and_not_a_second_after() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut market = Market::new(&snapshot, example);
    let quote = market.quote();

    market.wait(WINDOW.unsigned_abs());
    assert_eq!(market.now(), quote.expiry);
    let before = market.balances();
    market
        .settle(example, &quote, AMOUNT)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(
        market.balances(),
        before.after(payment(AMOUNT, PRICE), AMOUNT)
    );

    market.wait(1);
    let signed = signed_by(&market.maker, &quote.message());
    let run = market.run(example, market.maker_usdc, AMOUNT);
    let failure = market.refuse(&[signed, run]);
    assert_requirement_failed(&failure, example, "quoteHasNotExpired");
}

/// One base unit over `maxAmount` fails at `withinTheQuotedSize`; exactly `maxAmount` lands.
#[test]
fn an_amount_over_the_cap_fails_at_within_the_quoted_size() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut market = Market::new(&snapshot, example);
    let quote = market.quote();

    let signed = signed_by(&market.maker, &quote.message());
    let run = market.run(example, market.maker_usdc, MAX_AMOUNT + 1);
    let failure = market.refuse(&[signed, run]);
    assert_requirement_failed(&failure, example, "withinTheQuotedSize");

    let before = market.balances();
    market
        .settle(example, &quote, MAX_AMOUNT)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    // 2 SOL at 150.25 is 300.5 USDC exactly: nothing to round.
    assert_eq!(payment(MAX_AMOUNT, PRICE), 300_500_000);
    assert_eq!(market.balances(), before.after(300_500_000, MAX_AMOUNT));
}

/// The taker points the payment at a second USDC account of its own, made through the System and
/// Token programs' instructions as a wallet would make it. The run refuses it at
/// `paymentReachesTheMaker`.
#[test]
fn a_payee_the_maker_does_not_own_fails_at_payment_reaches_the_maker() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut market = Market::new(&snapshot, example);
    let diverted = keypair(b"ballista-protocol-tests-diverted");
    let taker = market.taker.pubkey();
    let create = create_token_account(
        &market.svm,
        &taker,
        &diverted.pubkey(),
        &market.usdc,
        &taker,
    );
    tx::send(&mut market.svm, &market.taker, &[&diverted], &create, &[])
        .unwrap_or_else(|failure| panic!("creating the second USDC account: {failure:?}"));

    let signed = signed_by(&market.maker, &market.quote().message());
    let run = market.run(example, diverted.pubkey(), AMOUNT);
    let failure = market.refuse(&[signed, run]);
    assert_requirement_failed(&failure, example, "paymentReachesTheMaker");
    assert_eq!(token_balance(&market.svm, &diverted.pubkey()), 0);
}

/// Each of the quote's other terms binds the run. A quote made out to another taker fails at
/// `quoteIsForThisTaker`; one naming wrapped SOL as the quote mint, at `paysInTheQuotedMint`; and
/// one naming USDC as the base mint, at `deliversTheQuotedMint`.
#[test]
fn a_quote_for_another_taker_or_other_mints_fails_at_its_term() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut market = Market::new(&snapshot, example);
    let quote = market.quote();
    let stranger = keypair(b"ballista-protocol-tests-stranger");
    for (terms, label) in [
        (
            Quote {
                taker: stranger.pubkey(),
                ..quote
            },
            "quoteIsForThisTaker",
        ),
        (
            Quote {
                quote_mint: market.wsol,
                ..quote
            },
            "paysInTheQuotedMint",
        ),
        (
            Quote {
                base_mint: market.usdc,
                ..quote
            },
            "deliversTheQuotedMint",
        ),
    ] {
        let signed = signed_by(&market.maker, &terms.message());
        let run = market.run(example, market.maker_usdc, AMOUNT);
        let failure = market.refuse(&[signed, run]);
        assert_requirement_failed(&failure, example, label);
    }
}

/// The run reads the instruction directly before its own. With nothing before it, the index of
/// the one before underflows at `quoteInstructionIndex`; so it does with the Ed25519 instruction
/// placed after the run. With a compute-budget instruction in between, the instruction before
/// the run is not Ed25519's, and it fails at `quoteIsEd25519`. The same compute-budget instruction
/// first, where wallets put it, is harmless.
#[test]
fn the_run_needs_the_ed25519_instruction_directly_before_it() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut market = Market::new(&snapshot, example);
    let signed = signed_by(&market.maker, &market.quote().message());
    let run = market.run(example, market.maker_usdc, AMOUNT);
    let budget = ComputeBudgetInstruction::set_compute_unit_limit(100_000);

    for instructions in [vec![run.clone()], vec![run.clone(), signed.clone()]] {
        let failure = market.refuse(&instructions);
        assert_ballista_failure(
            &failure,
            example,
            "ArithmeticOverflow",
            "quoteInstructionIndex",
        );
        assert_eq!(
            failure.err,
            TransactionError::InstructionError(0, InstructionError::Custom(failure.code.unwrap()))
        );
    }

    let failure = market.refuse(&[signed.clone(), budget.clone(), run.clone()]);
    assert_requirement_failed(&failure, example, "quoteIsEd25519");

    let before = market.balances();
    market
        .send(&[budget, signed, run])
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(
        market.balances(),
        before.after(payment(AMOUNT, PRICE), AMOUNT)
    );
}

/// Ballista keeps no state, so it cannot count settlements. The same signed quote, byte for byte,
/// settles twice within its window, and the two settlements together deliver more than
/// `maxAmount`: the cap bounds each settlement, not the quote. This is the limit the example
/// documents: `docs/examples/protocols/signed-quote.md`, "A quote can settle more than once", and
/// the template's own comment in `signed-quote-settlement.ts`. Only the maker's co-signer can
/// refuse the second (see `a_settlement_the_maker_does_not_co_sign_fails`).
#[test]
fn the_same_quote_settles_twice_within_its_window() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut market = Market::new(&snapshot, example);
    let quote = market.quote();
    let amount = 3 * SOL / 2;
    assert!(2 * amount > quote.max_amount);
    let signed = signed_by(&market.maker, &quote.message());
    let run = market.run(example, market.maker_usdc, amount);

    let before = market.balances();
    for _ in 0..2 {
        market
            .send(&[signed.clone(), run.clone()])
            .unwrap_or_else(|failure| panic!("{failure:?}"));
    }
    assert!(market.now() <= quote.expiry);
    let paid = payment(amount, PRICE);
    assert_eq!(market.balances(), before.after(2 * paid, 2 * amount));
}

/// The maker's co-signature is what the replay above needs, and what the template requires. The
/// same transaction, with the maker's account passed without its signature and only the taker
/// signing, fails in Ballista with the runtime's `MissingRequiredSignature`: the declared signer is
/// checked before any step, so no step label applies.
#[test]
fn a_settlement_the_maker_does_not_co_sign_fails() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut market = Market::new(&snapshot, example);
    let signed = signed_by(&market.maker, &market.quote().message());
    let mut run = market.run(example, market.maker_usdc, AMOUNT);
    let maker = market.maker.pubkey();
    for meta in run.accounts.iter_mut().filter(|meta| meta.pubkey == maker) {
        meta.is_signer = false;
    }
    let before = market.balances();
    let failure = tx::send(&mut market.svm, &market.taker, &[], &[signed, run], &[])
        .expect_err("the maker did not sign");
    assert_eq!(market.balances(), before);
    println!("refused: {}", summary(&failure));
    assert!(
        failure.program == ballista_sdk::ID
            && failure.code.is_none()
            && ballista_error(&failure).is_none()
            && failure.err
                == TransactionError::InstructionError(
                    1,
                    InstructionError::MissingRequiredSignature
                ),
        "{failure:?}"
    );
}
