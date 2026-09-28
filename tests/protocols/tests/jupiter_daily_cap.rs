//! `jupiterDailyCapSwap` against the real programs: route `solToUsdc`, 1 SOL for USDC through
//! Meteora DLMM, capped per caller at 1.728 SOL that refills at 20,000 lamports a second.
//!
//! The cap passes `route` its token program and signer itself and forwards the rest of the
//! route's accounts as its group. The run takes `route`'s place in Jupiter's own transaction:
//! Jupiter's setup wraps the SOL before it, and its cleanup closes the wrapped SOL account after.
//!
//! Each caller's entry is created inside its first run, at the caller's expense. The snapshot's
//! routes were built for one wallet; a second caller sends the same transaction rebuilt for its
//! own address and token accounts, which Jupiter's setup then creates.

use {
    ballista_protocol_tests::{
        snapshot::{warp, Leg, LegInstructions, Snapshot, SNAPSHOT_DIR},
        template::{examples, upload, Example, Run},
        tx::{self, assert_requirement_failed, ballista_error, Failure, Outcome},
        wallet::{self, associated_token_address, fund, keypair, token_balance, SOL},
    },
    ballista_sdk::{
        ballista_common::template::{REGISTRY_ENTRY_HEADER_LEN, REGISTRY_ENTRY_MAGIC},
        find_registry_entry_address, SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID,
    },
    litesvm::LiteSVM,
    solana_address::Address,
    solana_clock::Clock,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const TEMPLATE: &str = "jupiterDailyCapSwap";
const ROUTE: &str = "solToUsdc";
const TEMPLATE_ID: u16 = 13;
/// The accounts at the head of `route`'s list that the cap passes itself: the token program and
/// the signer. The rest arrive as `actionAccounts`.
const ROUTE_HEAD: usize = 2;
/// The template's cap and rate: 1.728 SOL, refilling over a day.
const DAILY_CAP: u64 = 1_728_000_000;
const REFILL_PER_SECOND: u64 = 20_000;
/// `dailySpend`, the template's one registry, and the bytes of its fields: `spent` (u64), then
/// `lastSpend` (i64).
const DAILY_SPEND: u8 = 0;
const FIELDS_LEN: usize = 8 + 8;
/// Ballista's `InvalidRegistryEntry`, without its context.
const INVALID_REGISTRY_ENTRY: u32 = 6025;
/// The key of a caller other than the wallet the snapshot's routes were built for.
const SECOND_CALLER: &[u8; 32] = b"ballista-protocol-tests-caller-2";

/// A caller: its keypair, the leg built for it, and its entry's address.
struct Caller {
    keypair: Keypair,
    leg: Leg,
    entry: Address,
}

impl Caller {
    fn address(&self) -> Address {
        self.keypair.pubkey()
    }
}

/// A fresh SVM with the template uploaded.
struct Cap {
    svm: LiteSVM,
    template: Address,
    jupiter: Address,
    /// The route's leg as the snapshot holds it, built for [`wallet::wallet`].
    leg: Leg,
}

impl Cap {
    fn new(snapshot: &Snapshot, example: &Example) -> Cap {
        let mut svm = snapshot.svm();
        let creator = keypair(b"ballista-protocol-tests-creator1");
        fund(&mut svm, &creator.pubkey(), 10 * SOL);
        let template = upload(&mut svm, &creator, TEMPLATE_ID, &example.payload);
        Cap {
            svm,
            template,
            jupiter: snapshot.named("jupiter"),
            leg: snapshot.route(ROUTE).legs[0].clone(),
        }
    }

    /// A caller holding 10 SOL, with the leg rebuilt for it (see [`rekeyed`]) and its entry at
    /// its registry address for its own key.
    fn caller(&mut self, keypair: Keypair) -> Caller {
        let address = keypair.pubkey();
        fund(&mut self.svm, &address, 10 * SOL);
        let leg = rekeyed(&self.leg, &address);
        let head: Vec<Address> = leg.instructions.swap.accounts[..ROUTE_HEAD]
            .iter()
            .map(|meta| meta.pubkey)
            .collect();
        assert_eq!(head, [TOKEN_PROGRAM_ID, address]);
        let (entry, _) =
            find_registry_entry_address(&self.template, DAILY_SPEND, &address.to_bytes());
        Caller { keypair, leg, entry }
    }

    /// A run of `caller`'s leg, on the route's own terms, with `entry` at `spend`.
    fn run(&self, example: &Example, caller: &Caller, entry: Address) -> Instruction {
        let route = &caller.leg.route;
        let swap = &caller.leg.instructions.swap;
        Run::new(self.template, example)
            .account("actionProgram", self.jupiter, false, false)
            .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
            .account("actor", caller.address(), true, true)
            .account("spend", entry, true, false)
            .account("systemProgram", SYSTEM_PROGRAM_ID, false, false)
            .input_bytes("routePlan", &route.route_plan)
            .input_u64("inAmount", route.in_amount)
            .input_u64("quotedOutAmount", route.quoted_out_amount)
            .input_u64("slippageBps", route.slippage_bps.into())
            .input_u64("platformFeeBps", route.platform_fee_bps.into())
            .group("actionAccounts", swap.accounts[ROUTE_HEAD..].to_vec())
            .build()
    }

    /// Sends `caller`'s run with `entry` at `spend`, in `route`'s place between Jupiter's own
    /// setup and cleanup.
    fn act_on(
        &mut self,
        example: &Example,
        caller: &Caller,
        entry: Address,
    ) -> Result<Outcome, Failure> {
        let instructions = caller.leg.instructions.with_swap(self.run(example, caller, entry));
        tx::send(
            &mut self.svm,
            &caller.keypair,
            &[],
            &instructions,
            &caller.leg.lookup_tables,
        )
    }

    /// Sends `caller`'s run on its own entry.
    fn act(&mut self, example: &Example, caller: &Caller) -> Result<Outcome, Failure> {
        self.act_on(example, caller, caller.entry)
    }

    /// Moves the clock on by `seconds`, and the slot at mainnet's 400 ms a slot (write rule 3).
    fn wait(&mut self, seconds: u64) {
        warp(&mut self.svm, seconds * 5 / 2, seconds);
    }

    fn now(&self) -> i64 {
        self.svm.get_sysvar::<Clock>().unix_timestamp
    }

    /// `caller`'s entry's `spent` and `lastSpend`.
    fn entry(&self, caller: &Caller) -> (u64, i64) {
        let account = self.svm.get_account(&caller.entry).expect("the entry exists");
        let fields = &account.data[REGISTRY_ENTRY_HEADER_LEN..];
        (
            u64::from_le_bytes(fields[..8].try_into().unwrap()),
            i64::from_le_bytes(fields[8..16].try_into().unwrap()),
        )
    }

    fn jupiter_ran(&self, failure: &Failure) -> bool {
        let invoked = format!("Program {} invoke", self.jupiter);
        failure.logs.iter().any(|line| line.starts_with(&invoked))
    }
}

/// `leg` rebuilt for `owner`: the wallet the snapshot's routes were built for, and its token
/// accounts for the leg's two mints, replaced by `owner` and `owner`'s wherever they appear. The
/// setup then creates `owner`'s token accounts and wraps its SOL, and the route sells from and
/// pays into them, all through the same instructions. For the wallet itself, this is `leg`.
fn rekeyed(leg: &Leg, owner: &Address) -> Leg {
    let wallet = wallet::wallet().pubkey();
    for (account, mint) in [
        (leg.source_token_account, leg.input_mint),
        (leg.destination_token_account, leg.output_mint),
    ] {
        assert_eq!(account, associated_token_address(&wallet, &mint));
    }
    let replacements = [
        (wallet, *owner),
        (
            leg.source_token_account,
            associated_token_address(owner, &leg.input_mint),
        ),
        (
            leg.destination_token_account,
            associated_token_address(owner, &leg.output_mint),
        ),
    ];
    let rekey = |instruction: &Instruction| Instruction {
        accounts: instruction
            .accounts
            .iter()
            .map(|meta| AccountMeta {
                pubkey: replacements
                    .iter()
                    .find(|(from, _)| *from == meta.pubkey)
                    .map_or(meta.pubkey, |(_, to)| *to),
                ..meta.clone()
            })
            .collect(),
        ..instruction.clone()
    };
    let instructions = &leg.instructions;
    Leg {
        instructions: LegInstructions {
            compute_budget: instructions.compute_budget.iter().map(rekey).collect(),
            setup: instructions.setup.iter().map(rekey).collect(),
            token_ledger: instructions.token_ledger.as_ref().map(rekey),
            swap: rekey(&instructions.swap),
            cleanup: instructions.cleanup.as_ref().map(rekey),
            other: instructions.other.iter().map(rekey).collect(),
        },
        source_token_account: replacements[1].1,
        destination_token_account: replacements[2].1,
        ..leg.clone()
    }
}

/// Jupiter's own transaction for the leg, in a fresh SVM: the run's baseline. Returns its
/// outcome, the USDC it bought, and the lamports it cost the wallet, fee aside.
fn jupiter_alone(snapshot: &Snapshot, leg: &Leg) -> (Outcome, u64, u64) {
    let mut svm = snapshot.svm();
    let trader = wallet::wallet();
    fund(&mut svm, &trader.pubkey(), 10 * SOL);
    let outcome = tx::send(
        &mut svm,
        &trader,
        &[],
        &leg.instructions.all(),
        &leg.lookup_tables,
    )
    .unwrap_or_else(|failure| panic!("Jupiter's own transaction: {failure:?}"));
    let bought = token_balance(&svm, &leg.destination_token_account);
    let cost = 10 * SOL - svm.get_balance(&trader.pubkey()).unwrap() - outcome.fee;
    (outcome, bought, cost)
}

/// The compute units `program` consumed in a transaction, from its logs, as
/// [`Outcome::compute_units_of`] reads them.
fn units_of(logs: &[String], program: &Address) -> Option<u64> {
    let prefix = format!("Program {program} consumed ");
    logs.iter()
        .filter_map(|line| line.strip_prefix(&prefix)?.split_once(" of ")?.0.parse().ok())
        .max()
}

/// Ballista refused the run with `InvalidRegistryEntry` when it opened the entry: at that open's
/// pc, before the first step, where no label reaches. Jupiter never ran.
#[track_caller]
fn assert_invalid_registry_entry(cap: &Cap, example: &Example, failure: &Failure) {
    assert_eq!(failure.program, ballista_sdk::ID, "{failure:?}");
    assert_eq!(
        failure.code.map(|code| code & 0xffff),
        Some(INVALID_REGISTRY_ENTRY),
        "{failure:?}"
    );
    let (name, pc) = ballista_error(failure).expect("Ballista's own error");
    assert_eq!(name, "InvalidRegistryEntry");
    assert_eq!(example.label_at(pc), None, "{failure:?}");
    assert!(!cap.jupiter_ran(failure), "{failure:?}");
    println!(
        "refused: InvalidRegistryEntry at pc {pc} after {} CU in Ballista's run",
        units_of(&failure.logs, &ballista_sdk::ID).unwrap()
    );
}

/// The first swap creates the caller's entry, at the caller's expense, and charges it the route's
/// `inAmount`. The route fills as in Jupiter's own transaction.
#[test]
fn the_first_swap_creates_the_callers_entry() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut cap = Cap::new(&snapshot, example);
    let caller = cap.caller(wallet::wallet());
    assert_eq!(cap.svm.get_account(&caller.entry), None);

    let outcome = cap
        .act(example, &caller)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    let entry = cap
        .svm
        .get_account(&caller.entry)
        .expect("the run created the entry");
    let size = REGISTRY_ENTRY_HEADER_LEN + FIELDS_LEN;
    assert_eq!(entry.owner, ballista_sdk::ID);
    assert_eq!(entry.data.len(), size);
    assert_eq!(entry.lamports, cap.svm.minimum_balance_for_rent_exemption(size));
    assert_eq!(entry.data[..4], REGISTRY_ENTRY_MAGIC);
    assert_eq!(entry.data[5], DAILY_SPEND);
    assert_eq!(entry.data[8..40], cap.template.to_bytes());
    assert_eq!(entry.data[40..72], caller.address().to_bytes());
    assert_eq!(cap.entry(&caller), (caller.leg.route.in_amount, cap.now()));

    // The route bought what Jupiter's own transaction does, and the run cost the caller that
    // transaction's lamports plus the entry's rent.
    let (alone, bought_alone, cost_alone) = jupiter_alone(&snapshot, &caller.leg);
    let bought = token_balance(&cap.svm, &caller.leg.destination_token_account);
    assert_eq!(bought, bought_alone);
    assert_eq!(cap.svm.get_account(&caller.leg.source_token_account), None);
    let cost = 10 * SOL - cap.svm.get_balance(&caller.address()).unwrap() - outcome.fee;
    assert_eq!(cost, cost_alone + entry.lamports);

    let run_units = outcome.compute_units_of(&ballista_sdk::ID).unwrap();
    let route_units = outcome.compute_units_of(&cap.jupiter).unwrap();
    println!(
        "template: {} bytes; entry: {size} bytes, {} lamports of rent",
        example.payload.len(),
        entry.lamports
    );
    println!(
        "run, creating the entry: {} CU in the transaction, {run_units} in Ballista's run, \
         {route_units} of them Jupiter's; {} bytes",
        outcome.compute_units, outcome.size,
    );
    println!(
        "Jupiter alone: {} CU in the transaction, {} in route; {} bytes; bought {bought} USDC units",
        alone.compute_units,
        alone.compute_units_of(&cap.jupiter).unwrap(),
        alone.size,
    );
}

/// With one sale on the books, a second fails at `withinRateLimit` until the refill covers it:
/// one second short it still fails, and at the exact second it lands, on the entry the first run
/// created.
#[test]
fn a_swap_past_the_cap_waits_for_the_refill() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut cap = Cap::new(&snapshot, example);
    let caller = cap.caller(wallet::wallet());
    let sold = caller.leg.route.in_amount;
    assert!(
        sold <= DAILY_CAP && 2 * sold > DAILY_CAP,
        "the route sells {sold}: one fits, two do not"
    );

    cap.act(example, &caller)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    let first_sale = cap.now();
    let failure = cap.act(example, &caller).unwrap_err();
    assert_requirement_failed(&failure, example, "withinRateLimit");
    assert!(!cap.jupiter_ran(&failure), "{failure:?}");
    println!(
        "refused at withinRateLimit after {} CU in Ballista's run",
        units_of(&failure.logs, &ballista_sdk::ID).unwrap()
    );

    // The second sale fits once `2 × sold − refill ≤ DAILY_CAP`.
    let needed = (2 * sold - DAILY_CAP).div_ceil(REFILL_PER_SECOND);
    cap.wait(needed - 1);
    let failure = cap.act(example, &caller).unwrap_err();
    assert_requirement_failed(&failure, example, "withinRateLimit");
    assert_eq!(cap.entry(&caller), (sold, first_sale));

    cap.wait(1);
    let outcome = cap
        .act(example, &caller)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(cap.now() - first_sale, i64::try_from(needed).unwrap());
    assert_eq!(
        cap.entry(&caller),
        (2 * sold - needed * REFILL_PER_SECOND, cap.now())
    );

    let run_units = outcome.compute_units_of(&ballista_sdk::ID).unwrap();
    let route_units = outcome.compute_units_of(&cap.jupiter).unwrap();
    println!(
        "waited {needed} s; spent {} of {DAILY_CAP}",
        cap.entry(&caller).0
    );
    println!(
        "run on an existing entry: {} CU in the transaction, {run_units} in Ballista's run, \
         {route_units} of them Jupiter's; {} bytes",
        outcome.compute_units, outcome.size,
    );
}

/// Each caller's limit is its own. With the wallet's spent for now, a second caller's swap still
/// lands, on an entry of its own that its run creates, and leaves the wallet's alone.
#[test]
fn a_second_caller_has_its_own_limit() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut cap = Cap::new(&snapshot, example);
    let first = cap.caller(wallet::wallet());
    let second = cap.caller(keypair(SECOND_CALLER));
    assert_ne!(first.entry, second.entry);
    let sold = first.leg.route.in_amount;

    cap.act(example, &first)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    let first_sale = cap.now();
    let failure = cap.act(example, &first).unwrap_err();
    assert_requirement_failed(&failure, example, "withinRateLimit");

    assert_eq!(cap.svm.get_account(&second.entry), None);
    let outcome = cap
        .act(example, &second)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(cap.entry(&second), (sold, cap.now()));
    assert_eq!(cap.entry(&first), (sold, first_sale));
    let bought = token_balance(&cap.svm, &second.leg.destination_token_account);
    assert!(
        bought >= second.leg.other_amount_threshold,
        "bought {bought} USDC units, under Jupiter's floor of {}",
        second.leg.other_amount_threshold
    );
    assert_eq!(cap.svm.get_account(&second.leg.source_token_account), None);
    println!(
        "second caller, creating its entry: {} CU in the transaction, {} in Ballista's run; {} bytes",
        outcome.compute_units,
        outcome.compute_units_of(&ballista_sdk::ID).unwrap(),
        outcome.size,
    );
}

/// One caller cannot charge or spend from another's entry. The run opens the entry for its own
/// signer's key before the first step, and the runtime refuses any other account there with
/// `InvalidRegistryEntry`: before the other caller's entry exists, it is not the address the run
/// would create; after, its header names the other key.
#[test]
fn a_caller_cannot_use_another_callers_entry() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut cap = Cap::new(&snapshot, example);
    let owner = cap.caller(wallet::wallet());
    let intruder = cap.caller(keypair(SECOND_CALLER));

    let failure = cap.act_on(example, &intruder, owner.entry).unwrap_err();
    assert_invalid_registry_entry(&cap, example, &failure);
    assert_eq!(cap.svm.get_account(&owner.entry), None);

    cap.act(example, &owner)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    let before = cap.entry(&owner);
    let failure = cap.act_on(example, &intruder, owner.entry).unwrap_err();
    assert_invalid_registry_entry(&cap, example, &failure);
    assert_eq!(cap.entry(&owner), before);
    assert_eq!(cap.svm.get_account(&intruder.entry), None);
}
