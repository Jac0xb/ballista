//! `pumpFunSellAll` against pump.fun's real bonding-curve program: the seller's whole balance of a
//! coin, read when the transaction runs, sold on the curve with a floor on the SOL received.

use {
    ballista_protocol_tests::{
        pump::{
            self, Coin, Trader, AVJ1, AVYG, BFX4, BUYBACK_FEE_RECIPIENT, EVENT_AUTHORITY,
            FEE_CONFIG, GLOBAL, HJXC, PUMP, PUMP_FEES, TOKEN, TOKEN_2022_PROGRAM,
        },
        template::{examples, upload, Example, Run},
        tx::{self, Failure, Outcome},
        wallet::{fund, keypair, SOL},
    },
    ballista_sdk::SYSTEM_PROGRAM_ID,
    litesvm::LiteSVM,
    solana_address::Address,
    solana_instruction::{AccountMeta, Instruction},
    solana_signer::Signer,
};

const EXAMPLE: &str = "pumpFunSellAll";

/// Anchor's own `ConstraintSeeds`, from its framework-wide range: pump.fun raises it when the curve
/// is not the PDA of the mint.
const ANCHOR_CONSTRAINT_SEEDS: u32 = 2006;

/// The snapshot, a seller holding some of each live coin, bought with pump.fun's own `buy`, and the
/// template.
struct Setup {
    svm: LiteSVM,
    seller: Trader,
    template: Address,
}

/// What the seller buys of each live coin before a test sells.
const HOLDING: u64 = 2_000_000 * TOKEN;

fn setup(example: &Example) -> Setup {
    let mut svm = pump::svm();
    let seller = pump::trader(&mut svm, "seller", 10 * SOL, &[HJXC, AVYG, AVJ1, BFX4]);
    for mint in [HJXC, AVYG, AVJ1] {
        let coin = pump::coin(&svm, mint);
        pump::buy_directly(&mut svm, &seller, &coin, HOLDING);
    }
    let creator = keypair(b"ballista-protocol-tests-creator1");
    fund(&mut svm, &creator.pubkey(), 10 * SOL);
    let template = upload(&mut svm, &creator, 1, &example.payload);
    Setup {
        svm,
        seller,
        template,
    }
}

/// The run selling everything in `token_account` on `coin`'s curve, signed by `seller`.
fn sell_all(
    template: Address,
    example: &Example,
    seller: Address,
    coin: &Coin,
    token_account: Address,
    min_sol_out: u64,
) -> Instruction {
    Run::new(template, example)
        .account("pumpProgram", PUMP, false, false)
        .account("global", GLOBAL, false, false)
        .account("feeRecipient", coin.fee_recipient, true, false)
        .account("mint", coin.mint, false, false)
        .account("bondingCurve", coin.bonding_curve, true, false)
        .account("curveTokenAccount", coin.curve_token_account, true, false)
        .account("sellerTokenAccount", token_account, true, false)
        .account("seller", seller, true, true)
        .account("systemProgram", SYSTEM_PROGRAM_ID, false, false)
        .account("creatorVault", coin.creator_vault, true, false)
        .account("tokenProgram", TOKEN_2022_PROGRAM, false, false)
        .account("eventAuthority", EVENT_AUTHORITY, false, false)
        .account("feeConfig", FEE_CONFIG, false, false)
        .account("feeProgram", PUMP_FEES, false, false)
        .account("bondingCurveV2", coin.bonding_curve_v2, false, false)
        .account("buybackFeeRecipient", BUYBACK_FEE_RECIPIENT, true, false)
        .input_u64("minSolOut", min_sol_out)
        .build()
}

impl Setup {
    fn coin(&self, mint: Address) -> Coin {
        pump::coin(&self.svm, mint)
    }

    fn lamports(&self) -> u64 {
        pump::lamports(&self.svm, &self.seller.address())
    }

    fn balance(&self, mint: &Address) -> u64 {
        pump::token_balance(&self.svm, &self.seller.token_account(mint))
    }

    /// Sells the seller's own balance of `mint`.
    fn sell(
        &mut self,
        example: &Example,
        mint: Address,
        min_sol_out: u64,
    ) -> Result<Outcome, Failure> {
        let coin = self.coin(mint);
        let account = self.seller.token_account(&mint);
        self.sell_from(example, &coin, account, min_sol_out)
    }

    fn sell_from(
        &mut self,
        example: &Example,
        coin: &Coin,
        token_account: Address,
        min_sol_out: u64,
    ) -> Result<Outcome, Failure> {
        let run = sell_all(
            self.template,
            example,
            self.seller.address(),
            coin,
            token_account,
            min_sol_out,
        );
        let seller = self.seller.keypair.insecure_clone();
        tx::send(&mut self.svm, &seller, &[], &[run], &[])
    }
}

/// What selling `amount` of `mint` pays, sold with pump.fun's own `sell` from a fresh copy of the
/// setup: the seller's lamports after, less before, less the transaction fee.
fn sold_directly(example: &Example, mint: Address, amount: u64) -> u64 {
    let mut twin = setup(example);
    let coin = twin.coin(mint);
    let before = twin.lamports();
    let sell = pump::sell(
        &twin.seller.address(),
        &twin.seller.token_account(&mint),
        &coin,
        amount,
        0,
    );
    let seller = twin.seller.keypair.insecure_clone();
    let outcome = tx::send(&mut twin.svm, &seller, &[], &[sell], &[])
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    twin.lamports() + outcome.fee - before
}

/// Sells each live coin whole: an ordinary coin and a mayhem-mode one.
#[test]
fn the_whole_balance_is_sold_for_what_pump_pays() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    for mint in [HJXC, AVJ1] {
        let mut setup = setup(example);
        let proceeds = sold_directly(example, mint, HOLDING);
        let before = setup.lamports();

        // A floor of exactly the proceeds lands.
        let outcome = setup
            .sell(example, mint, proceeds)
            .unwrap_or_else(|failure| panic!("{failure:?}"));

        assert_eq!(setup.balance(&mint), 0, "nothing is left");
        assert_eq!(setup.lamports() + outcome.fee - before, proceeds);
        println!(
            "sell {mint}: {proceeds} lamports for {HOLDING} base units; {} CU, {} of them \
             Ballista's own, {} bytes",
            outcome.compute_units,
            outcome.own_compute_units_of(&ballista_sdk::ID).unwrap(),
            outcome.size
        );
    }
}

/// The run is built for one balance, then more of the coin arrives before it lands. The template
/// reads the balance when it runs, so it sells all of it.
#[test]
fn a_balance_that_grew_after_signing_is_sold_whole() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let coin = setup.coin(AVYG);
    let run = sell_all(
        setup.template,
        example,
        setup.seller.address(),
        &coin,
        setup.seller.token_account(&AVYG),
        0,
    );
    pump::buy_directly(&mut setup.svm, &setup.seller, &coin, HOLDING);
    assert_eq!(setup.balance(&AVYG), 2 * HOLDING);

    let seller = setup.seller.keypair.insecure_clone();
    tx::send(&mut setup.svm, &seller, &[], &[run], &[])
        .unwrap_or_else(|failure| panic!("{failure:?}"));

    assert_eq!(setup.balance(&AVYG), 0);
}

#[test]
fn a_sale_below_the_floor_fails_at_received_at_least_min_sol_out() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let proceeds = sold_directly(example, HJXC, HOLDING);
    let before = setup.lamports();

    let failure = setup.sell(example, HJXC, proceeds + 1).unwrap_err();

    tx::assert_requirement_failed(&failure, example, "receivedAtLeastMinSolOut");
    assert_eq!(setup.balance(&HJXC), HOLDING, "the sale reverted");
    assert_eq!(setup.lamports(), before - failure.fee);
}

#[test]
fn a_graduated_coin_fails_at_curve_not_graduated() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    assert!(pump::curve(&setup.svm, &setup.coin(BFX4)).complete);

    let failure = setup.sell(example, BFX4, 0).unwrap_err();

    tx::assert_requirement_failed(&failure, example, "curveNotGraduated");
}

#[test]
fn an_empty_account_fails_at_has_tokens_to_sell() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let coin = setup.coin(HJXC);
    let empty = pump::trader(&mut setup.svm, "seller, empty", SOL, &[HJXC]);
    let run = sell_all(
        setup.template,
        example,
        empty.address(),
        &coin,
        empty.token_account(&HJXC),
        0,
    );

    let failure = tx::send(&mut setup.svm, &empty.keypair, &[], &[run], &[]).unwrap_err();

    tx::assert_requirement_failed(&failure, example, "hasTokensToSell");
}

/// The seller's own account of another coin, passed with this coin's curve.
#[test]
fn a_token_account_of_another_coin_fails_at_holds_the_curves_coin() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let coin = setup.coin(HJXC);
    let other = setup.seller.token_account(&AVYG);

    let failure = setup.sell_from(example, &coin, other, 0).unwrap_err();

    tx::assert_requirement_failed(&failure, example, "holdsTheCurvesCoin");
    assert_eq!(
        (setup.balance(&HJXC), setup.balance(&AVYG)),
        (HOLDING, HOLDING)
    );
}

/// Another coin's curve, with this coin's mint and account: the template's checks pass, and
/// pump.fun refuses a curve that is not the mint's.
#[test]
fn a_curve_of_another_coin_fails_in_pump() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let other = setup.coin(AVYG);
    let coin = Coin {
        bonding_curve: other.bonding_curve,
        curve_token_account: other.curve_token_account,
        ..setup.coin(HJXC)
    };
    let account = setup.seller.token_account(&HJXC);

    let failure = setup.sell_from(example, &coin, account, 0).unwrap_err();

    pump::assert_pump_error(&failure, ANCHOR_CONSTRAINT_SEEDS);
    assert_eq!(setup.balance(&HJXC), HOLDING);
}

/// Token-2022's `Approve`: `owner` lets `delegate` move up to `amount` out of `account`.
fn approve(account: &Address, delegate: &Address, owner: &Address, amount: u64) -> Instruction {
    let mut data = vec![4];
    data.extend_from_slice(&amount.to_le_bytes());
    Instruction {
        program_id: TOKEN_2022_PROGRAM,
        accounts: vec![
            AccountMeta::new(*account, false),
            AccountMeta::new_readonly(*delegate, false),
            AccountMeta::new_readonly(*owner, true),
        ],
        data,
    }
}

/// Security: pump.fun sells from whatever account the seller may move tokens out of, including one
/// it is only a delegate on. Alone, it sells a stranger's balance and pays the seller; in the
/// template, the run fails before the sale.
#[test]
fn a_delegated_strangers_balance_fails_at_sells_the_sellers_own_tokens() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let coin = setup.coin(HJXC);
    let stranger = pump::trader(&mut setup.svm, "seller, stranger", 10 * SOL, &[HJXC]);
    pump::buy_directly(&mut setup.svm, &stranger, &coin, HOLDING);
    let strangers = stranger.token_account(&HJXC);
    let approval = approve(
        &strangers,
        &setup.seller.address(),
        &stranger.address(),
        HOLDING,
    );
    tx::send(&mut setup.svm, &stranger.keypair, &[], &[approval], &[])
        .unwrap_or_else(|failure| panic!("{failure:?}"));

    let failure = setup.sell_from(example, &coin, strangers, 0).unwrap_err();

    tx::assert_requirement_failed(&failure, example, "sellsTheSellersOwnTokens");
    assert_eq!(pump::token_balance(&setup.svm, &strangers), HOLDING);

    // The same sale, sent to pump.fun alone, lands: the stranger's coins are sold for the seller.
    let before = setup.lamports();
    let alone = pump::sell(&setup.seller.address(), &strangers, &coin, HOLDING, 0);
    let seller = setup.seller.keypair.insecure_clone();
    tx::send(&mut setup.svm, &seller, &[], &[alone], &[])
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(pump::token_balance(&setup.svm, &strangers), 0);
    assert!(setup.lamports() > before);
}
