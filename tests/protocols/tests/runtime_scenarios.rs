//! The test-only runtime scenarios of `clients/js/examples/scenarios`, against the real programs,
//! on route `solToUsdc`: 1 SOL for USDC through Meteora DLMM, a venue that calls the Token program
//! directly.
//!
//! - Scenario A, `splitSellPayout`: a count loop sells the route in equal slices, each checked
//!   against the Pyth floor and logged as an `SLCE` event; a row loop pays the proceeds out; the
//!   run returns the total received.
//! - Scenario B, `nestedSplitSellPayout`: a run that calls Ballista to run `splitSellInner`, the
//!   same sell without the payout, reads the total it returns, and pays the rows out of it.
//!   `returnClaim` and `ballistaRelay` probe its edges and the call-depth limit.
//!
//! Each run takes `route`'s place after Jupiter's own compute budget and setup, which wraps the
//! seller's SOL and creates the USDC account. Jupiter's cleanup, which unwraps what is left, is
//! left out: a transaction's return data is its last instruction's, and the run's is the point
//! (`a_later_instruction_clears_the_runs_return_data`).
//!
//! What each scenario proves, and what it costs: `findings/runtime-scenarios.md`.

use {
    ballista_protocol_tests::{
        decode_hex,
        oracle::{pyth_price, set_pyth_price, PythPrice, SOL_USD_FEED_ID},
        snapshot::{jupiter_ran, Leg, Routing, Snapshot, ROUTE_HEAD, SNAPSHOT_DIR},
        template::{scenarios, upload, Example, Examples, Run},
        tx::{
            self, assert_ballista_failure, assert_requirement_failed, ballista_error, Failure,
            Outcome, ReturnData,
        },
        wallet::{self, fund, keypair, token_account, token_balance, SOL, WSOL_MINT},
    },
    ballista_sdk::{
        ballista_common::template::{ACCOUNT_EXECUTABLE, OP_EQ, OP_READ_U64},
        create_template_instruction, ProgramBuilder, RunInputs, TOKEN_PROGRAM_ID,
    },
    base64::{engine::general_purpose::STANDARD as BASE64, Engine},
    litesvm::LiteSVM,
    solana_address::Address,
    solana_instruction::{AccountMeta, Instruction},
    solana_keypair::Keypair,
    solana_signer::Signer,
    solana_transaction::{InstructionError, TransactionError},
};

const ROUTE: &str = "solToUsdc";
const SPLIT_SELL_ID: u16 = 30;
/// Where `nestedSplitSellPayout` pins its inner template: `INNER_TEMPLATE_ID` in
/// `nested-split-sell.ts`.
const INNER_ID: u16 = 31;
const OUTER_ID: u16 = 32;
const CLAIM_ID: u16 = 33;
const RELAY_ID: u16 = 34;
const RETURN_DATA_ID: u16 = 35;
/// 1%: how far below the oracle's valuation a slice's fill may land.
const TOLERANCE_BPS: u64 = 100;
/// The slices a run sells unless a test says otherwise.
const SLICES: u64 = 4;
/// `step.repeat`'s `max` in `split-sell.ts`.
const MAX_SLICES: u64 = 8;
/// ASCII `SLCE` and `PAID`: the tags of the split sell's and the outer run's events.
const SLICE_TAG: &[u8; 4] = b"SLCE";
const PAYOUT_TAG: &[u8; 4] = b"PAID";
/// The most instructions one transaction may run, its own and every CPI's together: Agave's
/// `MAX_INSTRUCTION_TRACE_LENGTH`.
const MAX_TRACE: usize = 64;
/// Trace entries in Jupiter's own transaction before the first slice: its two compute-budget
/// instructions; the setup's four, with the eight CPIs its two account creations make; and the run.
const JUPITER_TRANSACTION_ENTRIES: usize = 15;
/// Trace entries each slice adds: Jupiter's `route` and the call it makes to itself to log its
/// event, DLMM's swap and its two calls to itself, and the Token program's two transfers.
const ENTRIES_PER_SLICE: usize = 7;
/// The test wallets paid out of the proceeds: each gets an empty USDC account.
const RECIPIENTS: usize = 2;

/// A fresh SVM from the snapshot, with every scenario template uploaded by the test creator, the
/// seller holding SOL for Jupiter's setup to wrap, and `RECIPIENTS` wallets holding empty USDC
/// accounts.
struct Scene {
    svm: LiteSVM,
    creator: Keypair,
    seller: Keypair,
    leg: Leg,
    jupiter: Address,
    usdc: Address,
    sol_usd: Address,
    /// Each recipient's USDC account.
    recipients: Vec<Address>,
    split_sell: Address,
    inner: Address,
    outer: Address,
    claim: Address,
    relay: Address,
}

impl Scene {
    /// # Panics
    ///
    /// If the route is not the one these tests were written for: `route`, selling from the
    /// seller's wrapped SOL account through Meteora DLMM.
    fn new(snapshot: &Snapshot, scenarios: &Examples) -> Scene {
        let mut svm = snapshot.svm();
        let seller = wallet::wallet();
        let creator = keypair(b"ballista-protocol-tests-creator1");
        for key in [seller.pubkey(), creator.pubkey()] {
            fund(&mut svm, &key, 10 * SOL);
        }
        let leg = snapshot.route(ROUTE).legs[0].clone();
        let head: Vec<Address> = leg.instructions.swap.accounts[..ROUTE_HEAD]
            .iter()
            .map(|meta| meta.pubkey)
            .collect();
        assert_eq!(
            head,
            [
                TOKEN_PROGRAM_ID,
                seller.pubkey(),
                leg.source_token_account,
                leg.destination_token_account
            ],
            "route {ROUTE} does not start with the four accounts the template passes itself"
        );
        let dlmm = snapshot.named("meteoraDlmm");
        assert!(
            leg.instructions
                .swap
                .accounts
                .iter()
                .any(|meta| meta.pubkey == dlmm),
            "route {ROUTE} no longer goes through Meteora DLMM: a snapshot refresh moved it, so \
             the trace and depth figures in findings/runtime-scenarios.md need revisiting"
        );
        let usdc = snapshot.named("usdcMint");
        let recipients = (0..RECIPIENTS)
            .map(|index| {
                let mut seed = *b"ballista-scenario-recipient-0000";
                seed[31] += u8::try_from(index).unwrap();
                token_account(&mut svm, &keypair(&seed).pubkey(), &usdc, 0)
            })
            .collect();
        let mut upload_as =
            |id, name: &str| upload(&mut svm, &creator, id, &scenarios[name].payload);
        let split_sell = upload_as(SPLIT_SELL_ID, "splitSellPayout");
        let inner = upload_as(INNER_ID, "splitSellInner");
        let outer = upload_as(OUTER_ID, "nestedSplitSellPayout");
        let claim = upload_as(CLAIM_ID, "returnClaim");
        let relay = upload_as(RELAY_ID, "ballistaRelay");
        Scene {
            svm,
            creator,
            seller,
            leg,
            jupiter: snapshot.named("jupiter"),
            usdc,
            sol_usd: snapshot.named("pythSolUsd"),
            recipients,
            split_sell,
            inner,
            outer,
            claim,
            relay,
        }
    }

    /// Another scene from this one's state, without loading the snapshot or uploading again.
    fn copy(&self) -> Scene {
        Scene {
            svm: self.svm.clone(),
            creator: self.creator.insecure_clone(),
            seller: self.seller.insecure_clone(),
            leg: self.leg.clone(),
            recipients: self.recipients.clone(),
            ..*self
        }
    }

    /// Binds the fixed accounts and the route group every split sell and the outer run share.
    fn sale_accounts<'a>(&self, run: Run<'a>) -> Run<'a> {
        let routing = Routing::of(&self.leg);
        run.account("jupiter", self.jupiter, false, false)
            .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
            .account("priceUpdate", self.sol_usd, false, false)
            .account("seller", self.seller.pubkey(), false, true)
            .account("sourceAta", routing.source, true, false)
            .account("destinationAta", routing.destination, true, false)
            .group("routeAccounts", routing.steps)
    }

    /// A split sell of `slices`, `splitSellPayout` or `splitSellInner` at `template`, with no rows.
    fn split_sell<'a>(&self, template: Address, example: &'a Example, slices: u64) -> Run<'a> {
        let route = &self.leg.route;
        self.sale_accounts(Run::new(template, example))
            .input_pubkey("feedId", SOL_USD_FEED_ID)
            .input_bytes("routePlan", &route.route_plan)
            .input_u64("quotedInAmount", route.in_amount)
            .input_u64("quotedOutAmount", route.quoted_out_amount)
            .input_u64("slippageBps", u64::from(route.slippage_bps))
            .input_u64("platformFeeBps", u64::from(route.platform_fee_bps))
            .input_u64("toleranceBps", TOLERANCE_BPS)
            .input_u64("slices", slices)
    }

    /// Scenario A's run: `slices`, then recipient `i` paid `payouts[i]`.
    fn split_sell_payout(&self, example: &Example, slices: u64, payouts: &[u64]) -> Instruction {
        let run = self.split_sell(self.split_sell, example, slices);
        self.payout_rows(run, payouts).build()
    }

    /// Scenario B's run: the outer run, running the inner one over `slices`, then paying recipient
    /// `i` `payouts[i]`. `innerRun` is the inner run instruction's data after its `run` tag.
    fn nested(
        &self,
        outer: &Example,
        inner: &Example,
        slices: u64,
        payouts: &[u64],
    ) -> Instruction {
        let inner_run = self.split_sell(self.inner, inner, slices).build();
        let run = self
            .sale_accounts(Run::new(self.outer, outer))
            .account("ballista", ballista_sdk::ID, false, false)
            .account("innerTemplate", self.inner, false, false)
            .input_bytes("innerRun", &inner_run.data[1..]);
        self.payout_rows(run, payouts).build()
    }

    /// Adds a payout row per amount: recipient `i` paid `payouts[i]`.
    fn payout_rows<'a>(&self, mut run: Run<'a>, payouts: &[u64]) -> Run<'a> {
        assert!(payouts.len() <= RECIPIENTS, "{payouts:?}");
        for (recipient, &amount) in self.recipients.iter().zip(payouts) {
            run = run.row(|row| {
                row.account("recipientAta", *recipient, true, false)
                    .input_u64("amount", amount)
            });
        }
        run
    }

    /// `ballistaRelay` at the top of `relays` stacked relays, the innermost running `next` with
    /// `next_run` as its data and `next_accounts` as its accounts after the template. Each relay is
    /// one call frame more, so `next` runs at stack height `relays + 1`.
    fn relays(
        &self,
        relay: &Example,
        relays: usize,
        mut next: Address,
        mut next_run: Vec<u8>,
        mut next_accounts: Vec<AccountMeta>,
    ) -> Instruction {
        for _ in 1..relays {
            // A relay's own run: one group, then `nextRun`; its accounts are the Ballista program,
            // `next`, then the group.
            let group = u8::try_from(next_accounts.len()).unwrap();
            next_run = RunInputs::new().groups(&[group]).bytes(&next_run).finish();
            let mut accounts = vec![
                AccountMeta::new_readonly(ballista_sdk::ID, false),
                AccountMeta::new_readonly(next, false),
            ];
            accounts.extend(next_accounts);
            next_accounts = accounts;
            next = self.relay;
        }
        Run::new(self.relay, relay)
            .account("ballista", ballista_sdk::ID, false, false)
            .account("next", next, false, false)
            .input_bytes("nextRun", &next_run)
            .group("nextAccounts", next_accounts)
            .build()
    }

    /// Sends `run` after Jupiter's compute budget and setup, as the last instruction.
    fn send(&mut self, run: Instruction) -> Result<Outcome, Failure> {
        let mut instructions = self.leg.instructions.compute_budget.clone();
        instructions.extend(self.leg.instructions.setup.iter().cloned());
        instructions.push(run);
        self.send_all(&instructions)
    }

    fn send_all(&mut self, instructions: &[Instruction]) -> Result<Outcome, Failure> {
        tx::send(
            &mut self.svm,
            &self.seller,
            &[],
            instructions,
            &self.leg.lookup_tables,
        )
    }

    /// The seller's USDC.
    fn usdc(&self) -> u64 {
        token_balance(&self.svm, &self.leg.destination_token_account)
    }

    /// Each recipient's USDC.
    fn paid(&self) -> Vec<u64> {
        self.recipients
            .iter()
            .map(|recipient| token_balance(&self.svm, recipient))
            .collect()
    }

    /// Sets the SOL/USD price to `price`, in units of 10^−8 dollars (write rule 2).
    fn move_oracle(&mut self, price: i64) {
        let market = pyth_price(&self.svm, &self.sol_usd);
        set_pyth_price(&mut self.svm, &self.sol_usd, PythPrice { price, ..market });
    }
}

/// One `SLCE` event: the pass, what it sold and what it received.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Slice {
    pass: u8,
    sold: u64,
    received: u64,
}

/// The `SLCE` events Ballista logged at stack height `height`, in order.
fn slices_logged(logs: &[String], height: usize) -> Vec<Slice> {
    events(logs, SLICE_TAG, height, 1 + 8 + 8)
        .into_iter()
        .map(|body| Slice {
            pass: body[0],
            sold: u64_at(&body, 1),
            received: u64_at(&body, 9),
        })
        .collect()
}

/// The outer run's `PAID` event, if it logged one: the inner run's total, and what the rows were
/// paid.
fn payout_logged(logs: &[String]) -> Option<(u64, u64)> {
    match &events(logs, PAYOUT_TAG, 1, 8 + 8)[..] {
        [] => None,
        [body] => Some((u64_at(body, 0), u64_at(body, 8))),
        more => panic!("the outer run logs one PAID event, not {}", more.len()),
    }
}

/// The body after the tag of each event Ballista logged with `tag`.
///
/// # Panics
///
/// If such a line is not one field of the tag and `len` bytes, or another program or height
/// logged it.
fn events(logs: &[String], tag: &[u8; 4], height: usize, len: usize) -> Vec<Vec<u8>> {
    tx::program_data(logs)
        .into_iter()
        .filter(|logged| {
            logged
                .fields
                .first()
                .is_some_and(|field| field.starts_with(tag))
        })
        .map(|logged| {
            assert_eq!(
                (logged.program, logged.height),
                (ballista_sdk::ID, height),
                "{logged:?}"
            );
            let [field] = &logged.fields[..] else {
                panic!("an EMIT logs one field: {logged:?}");
            };
            assert_eq!(field.len(), tag.len() + len, "{logged:?}");
            field[tag.len()..].to_vec()
        })
        .collect()
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

/// A `u64` return value: eight little-endian bytes, set by `program`.
fn returned(return_data: &ReturnData, program: &Address) -> u64 {
    assert_eq!(&return_data.program, program, "{return_data:?}");
    u64::from_le_bytes(return_data.data[..].try_into().unwrap())
}

/// The log line the runtime writes when `program` sets `value` as its return data.
fn return_line(program: &Address, value: u64) -> String {
    format!(
        "Program return: {program} {}",
        BASE64.encode(value.to_le_bytes())
    )
}

/// How many instructions ran, the transaction's own and every CPI: each logs one `invoke` line.
fn trace_entries(logs: &[String]) -> usize {
    logs.iter()
        .filter(|line| line.contains(" invoke ["))
        .count()
}

/// The fills of `count` slices of `slice` sold one after another through the route by Jupiter
/// alone, one transaction each, after Jupiter's setup, with `wrapped` lamports of wrapped SOL
/// written to the seller first (write rule 1): what the run's slices should each receive.
fn jupiter_slices(
    snapshot: &Snapshot,
    leg: &Leg,
    wrapped: u64,
    slice: u64,
    count: u64,
) -> Vec<u64> {
    let mut svm = snapshot.svm();
    let seller = wallet::wallet();
    fund(&mut svm, &seller.pubkey(), 10 * SOL);
    if wrapped > 0 {
        token_account(&mut svm, &seller.pubkey(), &WSOL_MINT, wrapped);
    }
    let mut setup = leg.instructions.compute_budget.clone();
    setup.extend(leg.instructions.setup.iter().cloned());
    tx::send(&mut svm, &seller, &[], &setup, &leg.lookup_tables)
        .unwrap_or_else(|failure| panic!("Jupiter's setup: {failure:?}"));
    (0..count)
        .map(|_| {
            let before = token_balance(&svm, &leg.destination_token_account);
            let mut data = leg.instructions.swap.data[..8].to_vec();
            data.extend_from_slice(&leg.route.route_plan);
            data.extend_from_slice(&slice.to_le_bytes());
            data.extend_from_slice(&slice_quote(leg, slice).to_le_bytes());
            data.extend_from_slice(&leg.route.slippage_bps.to_le_bytes());
            data.push(leg.route.platform_fee_bps);
            let mut instructions = leg.instructions.compute_budget.clone();
            instructions.push(Instruction {
                data,
                ..leg.instructions.swap.clone()
            });
            tx::send(&mut svm, &seller, &[], &instructions, &leg.lookup_tables)
                .unwrap_or_else(|failure| panic!("Jupiter selling {slice}: {failure:?}"));
            token_balance(&svm, &leg.destination_token_account) - before
        })
        .collect()
}

/// The quote rescaled to `slice`, as the template rescales it.
fn slice_quote(leg: &Leg, slice: u64) -> u64 {
    let quote = u128::from(leg.route.quoted_out_amount) * u128::from(slice)
        / u128::from(leg.route.in_amount);
    u64::try_from(quote).unwrap()
}

/// The template's floor for a slice: `slice × price ÷ 10^11`, less `TOLERANCE_BPS`, rounded down
/// at each step.
fn slice_floor(slice: u64, price: i64) -> u64 {
    let worth = u128::from(slice) * u128::try_from(price).unwrap() / 10u128.pow(11);
    u64::try_from(worth * u128::from(10_000 - TOLERANCE_BPS) / 10_000).unwrap()
}

/// The highest price whose floor a fill of `received` for `slice` still clears. The floor never
/// falls as the price rises, so this searches for the edge.
fn highest_price_cleared(slice: u64, received: u64) -> i64 {
    let clears = |price| slice_floor(slice, price) <= received;
    let (mut cleared, mut refused) = (0, i64::MAX / 1_000);
    assert!(clears(cleared) && !clears(refused));
    while refused - cleared > 1 {
        let middle = cleared + (refused - cleared) / 2;
        if clears(middle) {
            cleared = middle;
        } else {
            refused = middle;
        }
    }
    cleared
}

/// Prints a landed run's costs for `findings/runtime-scenarios.md`.
fn report(what: &str, outcome: &Outcome, jupiter: &Address) {
    println!(
        "{what}: {} CU in the transaction, {} in Ballista's outermost run, {} in Jupiter's \
         costliest route; {} bytes; {} trace entries; deepest frame {}",
        outcome.compute_units,
        outcome.compute_units_of(&ballista_sdk::ID).unwrap(),
        outcome.compute_units_of(jupiter).unwrap_or(0),
        outcome.size,
        trace_entries(&outcome.logs),
        tx::deepest_invocation(&outcome.logs),
    );
}

// ------------------------------------------------------------------ scenario A: split sell, payout

/// The split sell lands: every slice sells exactly its share and fetches what Jupiter alone fetches
/// for it, each pass logs its own `SLCE` event with those amounts, the rows are paid, and the run
/// returns the total that the balances show arrived.
#[test]
fn a_split_sell_lands_logs_each_slice_and_returns_the_total() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let scenarios = scenarios();
    let example = &scenarios["splitSellPayout"];
    let mut scene = Scene::new(&snapshot, &scenarios);
    let slice = scene.leg.in_amount / SLICES;
    let payouts = [1_000_000, 2_500_000];

    let run = scene.split_sell_payout(example, SLICES, &payouts);
    let outcome = scene
        .send(run)
        .unwrap_or_else(|failure| panic!("{failure:?}"));

    // Nothing is left to sell: 1 SOL is four slices exactly.
    assert_eq!(slice * SLICES, scene.leg.in_amount);
    assert_eq!(
        token_balance(&scene.svm, &scene.leg.source_token_account),
        0
    );
    // What arrived, measured on the balances: what the seller kept and what the rows were paid.
    let paid = scene.paid();
    assert_eq!(paid, payouts);
    let total = scene.usdc() + paid.iter().sum::<u64>();

    let fills = jupiter_slices(&snapshot, &scene.leg, 0, slice, SLICES);
    let expected: Vec<Slice> = fills
        .iter()
        .enumerate()
        .map(|(pass, &received)| Slice {
            pass: u8::try_from(pass).unwrap(),
            sold: slice,
            received,
        })
        .collect();
    assert_eq!(slices_logged(&outcome.logs, 1), expected);
    assert_eq!(fills.iter().sum::<u64>(), total);
    assert_eq!(returned(&outcome.return_data, &ballista_sdk::ID), total);

    // One route instruction per slice, each a CPI straight from the run.
    let routes = format!("Program {} invoke [2]", scene.jupiter);
    assert_eq!(
        outcome.logs.iter().filter(|line| **line == routes).count(),
        usize::try_from(SLICES).unwrap()
    );
    let price = pyth_price(&scene.svm, &scene.sol_usd).price;
    println!(
        "sold {SLICES} slices of {slice} lamports for {fills:?} USDC units, {total} in all; each \
         slice's floor {}",
        slice_floor(slice, price)
    );
    report("scenario A, 4 slices, 2 rows", &outcome, &scene.jupiter);
}

/// Each slice is held to the floor on its own pass. Selling 40 SOL crosses DLMM bins, so each
/// slice fills lower than the one before; with the oracle between the third fill's highest
/// cleared price and the fourth's (write rule 2), three slices sell and log their events, and the
/// fourth fails at `sliceBeatTheOracle`, reverting the whole transaction.
#[test]
fn each_slice_must_clear_the_oracle_floor_on_its_own_pass() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let scenarios = scenarios();
    let example = &scenarios["splitSellPayout"];
    let mut scene = Scene::new(&snapshot, &scenarios);
    // 39 SOL written to the seller's wrapped SOL account (write rule 1); the setup wraps the 40th.
    let wrapped = 39 * SOL;
    let seller = scene.seller.pubkey();
    token_account(&mut scene.svm, &seller, &WSOL_MINT, wrapped);
    let slice = (wrapped + scene.leg.in_amount) / SLICES;

    let fills = jupiter_slices(&snapshot, &scene.leg, wrapped, slice, SLICES);
    assert!(
        fills.windows(2).all(|pair| pair[1] < pair[0]),
        "each slice should fill lower than the last: {fills:?}"
    );
    let last = highest_price_cleared(slice, fills[3]);
    assert!(last < highest_price_cleared(slice, fills[2]));
    scene.move_oracle(last + 1);

    let run = scene.split_sell_payout(example, SLICES, &[1]);
    let failure = scene.send(run).unwrap_err();
    assert_requirement_failed(&failure, example, "sliceBeatTheOracle");
    // The failed transaction still logged the three passes that cleared.
    let logged: Vec<u64> = slices_logged(&failure.logs, 1)
        .iter()
        .map(|slice| slice.received)
        .collect();
    assert_eq!(logged, fills[..3]);
    // Everything reverted but the fee: the 39 SOL written stay, and the setup's USDC account is
    // gone with the rest.
    assert_eq!(
        token_balance(&scene.svm, &scene.leg.source_token_account),
        wrapped
    );
    assert_eq!(
        scene.svm.get_account(&scene.leg.destination_token_account),
        None
    );
    println!(
        "40 SOL in four slices filled {fills:?}; oracle at {}",
        last + 1
    );
}

/// A count above the loop's `max` fails as the loop starts, before any slice is sold.
#[test]
fn more_slices_than_the_loops_max_fail_before_anything_is_sold() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let scenarios = scenarios();
    let example = &scenarios["splitSellPayout"];
    let mut scene = Scene::new(&snapshot, &scenarios);

    let run = scene.split_sell_payout(example, MAX_SLICES + 1, &[1]);
    let failure = scene.send(run).unwrap_err();
    assert_ballista_failure(&failure, example, "LoopCountExceeded", "sellSlices");
    assert!(!jupiter_ran(&scene.jupiter, &failure), "{failure:?}");
    assert!(slices_logged(&failure.logs, 1).is_empty());
}

/// Each slice is a Jupiter `route` whose platform fee rate and account are the run's builder's
/// choice. The split sell caps `platformFeeBps` at `MAX_PLATFORM_FEE_BPS`, 0, so a 1% fee paid to
/// an attacker's USDC account, at 2% slippage, fails at `platformFeeWithinCap` before the first
/// slice is sold. `findings/platform-fee.md`.
#[test]
fn a_hostile_platform_fee_fails_at_platform_fee_within_cap() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let scenarios = scenarios();
    let example = &scenarios["splitSellPayout"];
    let mut scene = Scene::new(&snapshot, &scenarios);
    let attacker = keypair(b"ballista-protocol-tests-attacker").pubkey();
    let attacker_usdc = token_account(&mut scene.svm, &attacker, &scene.usdc, 0);
    scene.leg = scene.leg.with_platform_fee(100, 200, attacker_usdc);

    let run = scene.split_sell_payout(example, 2, &[1]);
    let failure = scene
        .send(run)
        .expect_err("a platform fee above the cap should fail");
    assert_ballista_failure(
        &failure,
        example,
        "RequirementFailed",
        "platformFeeWithinCap",
    );
    assert!(!jupiter_ran(&scene.jupiter, &failure), "{failure:?}");
    assert!(slices_logged(&failure.logs, 1).is_empty());
    assert_eq!(token_balance(&scene.svm, &attacker_usdc), 0);
}

/// Rows that ask one unit more than the sale received fail at `payoutsWithinProceeds`: the first
/// row takes the whole total, and the second's unit is refused before its transfer. Rows that ask
/// exactly the total land and leave the seller none of it.
#[test]
fn payouts_above_the_total_received_fail_at_their_label() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let scenarios = scenarios();
    let example = &scenarios["splitSellPayout"];
    let mut scene = Scene::new(&snapshot, &scenarios);
    // The same sale in a copy measures the total the rows may share.
    let mut probe = scene.copy();
    let run = probe.split_sell_payout(example, SLICES, &[1]);
    let outcome = probe
        .send(run)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    let total = returned(&outcome.return_data, &ballista_sdk::ID);

    let run = scene.split_sell_payout(example, SLICES, &[total, 1]);
    let failure = scene.send(run).unwrap_err();
    assert_requirement_failed(&failure, example, "payoutsWithinProceeds");
    assert_eq!(
        slices_logged(&failure.logs, 1).len(),
        usize::try_from(SLICES).unwrap()
    );

    let run = scene.split_sell_payout(example, SLICES, &[total - 1, 1]);
    scene
        .send(run)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(scene.paid(), [total - 1, 1]);
    assert_eq!(scene.usdc(), 0);
}

/// The loop's `max` is 8, but in Jupiter's own transaction a sale stops at 6 slices: each adds
/// seven instructions to the trace, and a transaction runs at most 64. The seventh fails as the
/// trace overflows, far short of the compute budget. With the token accounts already in place, and
/// so no setup in the transaction, all 8 fit.
#[test]
fn the_instruction_trace_caps_the_slices_below_the_loops_max() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let scenarios = scenarios();
    let example = &scenarios["splitSellPayout"];
    let scene = Scene::new(&snapshot, &scenarios);
    let payouts = [1, 2];

    for slices in [1, 2, 4, 6] {
        let mut attempt = scene.copy();
        let run = attempt.split_sell_payout(example, slices, &payouts);
        let outcome = attempt
            .send(run)
            .unwrap_or_else(|failure| panic!("{slices} slices: {failure:?}"));
        assert_eq!(
            trace_entries(&outcome.logs),
            JUPITER_TRANSACTION_ENTRIES
                + ENTRIES_PER_SLICE * usize::try_from(slices).unwrap()
                + payouts.len()
        );
        report(
            &format!("scenario A, {slices} slices in Jupiter's transaction"),
            &outcome,
            &scene.jupiter,
        );
    }

    let mut attempt = scene.copy();
    let run = attempt.split_sell_payout(example, 7, &[1]);
    let failure = attempt.send(run).unwrap_err();
    // The run is instruction 6, after two compute-budget and four setup instructions. The runtime
    // refuses the CPI that would be entry 65, so Ballista itself fails, with no code of its own.
    assert_eq!(
        (failure.program, &failure.err),
        (
            ballista_sdk::ID,
            &TransactionError::InstructionError(
                6,
                InstructionError::MaxInstructionTraceLengthExceeded
            )
        ),
        "{failure:?}"
    );
    assert_eq!(trace_entries(&failure.logs), MAX_TRACE);
    assert_eq!(slices_logged(&failure.logs, 1).len(), 7);

    // The seller's token accounts written beforehand (write rule 1): the transaction is the
    // compute budget and the run, three entries before the first slice.
    let mut attempt = scene.copy();
    let seller = attempt.seller.pubkey();
    let (wrapped, usdc) = (attempt.leg.in_amount, attempt.usdc);
    token_account(&mut attempt.svm, &seller, &WSOL_MINT, wrapped);
    token_account(&mut attempt.svm, &seller, &usdc, 0);
    let mut instructions = attempt.leg.instructions.compute_budget.clone();
    instructions.push(attempt.split_sell_payout(example, MAX_SLICES, &payouts));
    let outcome = attempt
        .send_all(&instructions)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(
        trace_entries(&outcome.logs),
        3 + ENTRIES_PER_SLICE * usize::try_from(MAX_SLICES).unwrap() + payouts.len()
    );
    assert_eq!(
        slices_logged(&outcome.logs, 1).len(),
        usize::try_from(MAX_SLICES).unwrap()
    );
    report(
        "scenario A, 8 slices without Jupiter's setup",
        &outcome,
        &scene.jupiter,
    );
}

/// A transaction's return data is its last instruction's: the runtime clears it as each
/// instruction starts. With Jupiter's cleanup after the run, the transaction ends with the Token
/// program's empty return data, though the run did return its total.
#[test]
fn a_later_instruction_clears_the_runs_return_data() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let scenarios = scenarios();
    let example = &scenarios["splitSellPayout"];
    let mut scene = Scene::new(&snapshot, &scenarios);

    let run = scene.split_sell_payout(example, SLICES, &[1]);
    let instructions = scene.leg.instructions.with_swap(run);
    let outcome = scene
        .send_all(&instructions)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(
        outcome.return_data,
        ReturnData {
            program: TOKEN_PROGRAM_ID,
            data: vec![],
        }
    );
    let total = slices_logged(&outcome.logs, 1)
        .iter()
        .map(|slice| slice.received)
        .sum();
    assert!(
        outcome
            .logs
            .contains(&return_line(&ballista_sdk::ID, total)),
        "{outcome:?}"
    );
}

// --------------------------------------------------------------- scenario B: nested composition

/// The outer run calls Ballista to run the inner split sell, reads the total it returns, and pays
/// the rows exactly that. The inner run's events are logged a frame down, the outer's own at the
/// top, and the deepest call is the Token program under DLMM under Jupiter under the inner run
/// under the outer: five frames, the runtime's limit.
#[test]
fn a_nested_run_pays_out_exactly_what_the_inner_run_returned() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let scenarios = scenarios();
    let (outer, inner) = (
        &scenarios["nestedSplitSellPayout"],
        &scenarios["splitSellInner"],
    );
    let mut scene = Scene::new(&snapshot, &scenarios);
    let slice = scene.leg.in_amount / SLICES;
    let fills = jupiter_slices(&snapshot, &scene.leg, 0, slice, SLICES);
    let total: u64 = fills.iter().sum();
    let payouts = [total - total / 3, total / 3];

    let run = scene.nested(outer, inner, SLICES, &payouts);
    let outcome = scene
        .send(run)
        .unwrap_or_else(|failure| panic!("{failure:?}"));

    assert_eq!(scene.paid(), payouts);
    assert_eq!(scene.usdc(), 0);
    let received: Vec<u64> = slices_logged(&outcome.logs, 2)
        .iter()
        .map(|slice| slice.received)
        .collect();
    assert_eq!(received, fills);
    assert_eq!(payout_logged(&outcome.logs), Some((total, total)));
    // The inner run's return, as the runtime logged it for the outer run to read.
    assert!(
        outcome
            .logs
            .contains(&return_line(&ballista_sdk::ID, total)),
        "{outcome:?}"
    );
    assert_eq!(tx::deepest_invocation(&outcome.logs), 5);
    report("scenario B, 4 slices, 2 rows", &outcome, &scene.jupiter);
}

/// Rows that ask one unit more than the inner run returned fail at the outer run's
/// `payoutsWithinInnerTotal`, after the inner run sold everything.
#[test]
fn payouts_above_the_inner_total_fail_at_their_label() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let scenarios = scenarios();
    let (outer, inner) = (
        &scenarios["nestedSplitSellPayout"],
        &scenarios["splitSellInner"],
    );
    let mut scene = Scene::new(&snapshot, &scenarios);
    let slice = scene.leg.in_amount / SLICES;
    let total: u64 = jupiter_slices(&snapshot, &scene.leg, 0, slice, SLICES)
        .iter()
        .sum();

    let run = scene.nested(outer, inner, SLICES, &[total, 1]);
    let failure = scene.send(run).unwrap_err();
    assert_requirement_failed(&failure, outer, "payoutsWithinInnerTotal");
    assert_eq!(
        slices_logged(&failure.logs, 2).len(),
        usize::try_from(SLICES).unwrap()
    );
}

/// Return data names the program that set it, not the template: any Ballista run passes the
/// runtime's program check. An unpinned relay passes up whatever `returnClaim` claims. The outer
/// run pins its inner template, so the same claim in the inner run's place fails that account's
/// constraint before any step runs.
#[test]
fn the_outer_run_refuses_any_inner_template_but_the_pinned_one() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let scenarios = scenarios();
    let (outer, inner) = (
        &scenarios["nestedSplitSellPayout"],
        &scenarios["splitSellInner"],
    );
    let mut scene = Scene::new(&snapshot, &scenarios);
    // A million USDC, which nothing sold.
    let claimed = 1_000_000 * 1_000_000;

    let relay = scene.relays(
        &scenarios["ballistaRelay"],
        1,
        scene.claim,
        RunInputs::new().u64(claimed).finish(),
        vec![],
    );
    let outcome = scene
        .send_all(&[relay])
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(returned(&outcome.return_data, &ballista_sdk::ID), claimed);

    let mut run = scene.nested(outer, inner, SLICES, &[claimed]);
    let position = outer
        .fixed_accounts
        .iter()
        .position(|name| name == "innerTemplate")
        .unwrap();
    // The run's accounts start with the outer template; the fixed accounts follow.
    assert_eq!(run.accounts[1 + position].pubkey, scene.inner);
    run.accounts[1 + position].pubkey = scene.claim;
    let failure = scene.send(run).unwrap_err();
    assert_eq!(
        ballista_error(&failure),
        Some(("AccountConstraintFailed", u16::try_from(position).unwrap())),
        "{failure:?}"
    );
    assert!(!jupiter_ran(&scene.jupiter, &failure));
}

/// The runtime's return-data read takes data only from the program the invoke before it called.
/// A nested run that sets none leaves the last program's instead: here `fixtures/return-data.hex`,
/// which asks the Token program for an account's size and reads it itself, leaves the Token
/// program's 165. The relay's read refuses it with `ReturnDataMismatch`.
#[test]
fn a_nested_run_cannot_pass_off_another_programs_return_data() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let scenarios = scenarios();
    let relay = &scenarios["ballistaRelay"];
    let mut scene = Scene::new(&snapshot, &scenarios);
    let size_reader = upload(
        &mut scene.svm,
        &scene.creator,
        RETURN_DATA_ID,
        &decode_hex(include_str!("../../../fixtures/return-data.hex")),
    );

    // `return-data`'s accounts: the Token program, then a mint. It takes no inputs.
    let run = scene.relays(
        relay,
        1,
        size_reader,
        vec![],
        vec![
            AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
            AccountMeta::new_readonly(scene.usdc, false),
        ],
    );
    let failure = scene.send_all(&[run]).unwrap_err();
    assert_ballista_failure(&failure, relay, "ReturnDataMismatch", "readNextResult");
    assert!(
        failure.logs.contains(&return_line(&TOKEN_PROGRAM_ID, 165)),
        "{failure:?}"
    );
}

/// A return-data read before the call that sets it cannot be stored: the verifier refuses a read
/// anywhere but straight after an unconditional invoke, at upload. The same template with the read
/// moved after the call uploads.
#[test]
fn a_return_data_read_before_the_inner_call_is_refused_at_upload() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let scenarios = scenarios();
    let mut scene = Scene::new(&snapshot, &scenarios);
    let creator = scene.creator.pubkey();

    // A template that calls Ballista and requires its return to equal itself.
    let template = |read_first: bool| {
        let mut builder = ProgramBuilder::new();
        let ballista = builder.account(
            ACCOUNT_EXECUTABLE,
            Some(ballista_sdk::ID.to_bytes()),
            None,
            0,
        );
        let cpi = builder.cpi(ballista, &[], &[]);
        let total = if read_first {
            let total = builder.return_data(OP_READ_U64, 0);
            builder.invoke(cpi, None);
            total
        } else {
            builder.invoke(cpi, None);
            builder.return_data(OP_READ_U64, 0)
        };
        let same = builder.binary(OP_EQ, total, total);
        builder.require(same);
        builder.build().unwrap()
    };

    let create = create_template_instruction(creator, 40, &template(true));
    let failure = tx::send(&mut scene.svm, &scene.creator, &[], &[create], &[]).unwrap_err();
    // The read is the template's first instruction.
    assert_eq!(
        ballista_error(&failure),
        Some(("InvalidReturnData", 0)),
        "{failure:?}"
    );

    let create = create_template_instruction(creator, 41, &template(false));
    tx::send(&mut scene.svm, &scene.creator, &[], &[create], &[])
        .unwrap_or_else(|failure| panic!("{failure:?}"));
}

/// A failure inside the inner run fails the whole nested run, and is named by the inner template's
/// own label: with the oracle 5% above the market (write rule 2), the first slice fails at
/// `splitSellInner`'s `sliceBeatTheOracle`, and the outer run never reads a total.
#[test]
fn the_inner_runs_floor_fails_the_nested_run_at_the_inner_label() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let scenarios = scenarios();
    let (outer, inner) = (
        &scenarios["nestedSplitSellPayout"],
        &scenarios["splitSellInner"],
    );
    let mut scene = Scene::new(&snapshot, &scenarios);
    let market = pyth_price(&scene.svm, &scene.sol_usd).price;
    scene.move_oracle(market * 105 / 100);

    let run = scene.nested(outer, inner, SLICES, &[1]);
    let failure = scene.send(run).unwrap_err();
    assert_requirement_failed(&failure, inner, "sliceBeatTheOracle");
    assert!(slices_logged(&failure.logs, 2).is_empty());
    assert_eq!(payout_logged(&failure.logs), None);
}

/// The runtime's call-depth limit is five frames: the transaction's own instruction and four
/// nested calls. Relays add one frame each over `returnClaim`: four relays put it at frame five
/// and land, passing its claim all the way up; five would put it at six and fail as the fifth relay
/// calls it. Scenario B's deepest call, the Token program under DLMM, Jupiter and two Ballista
/// runs, sits exactly at the limit.
#[test]
fn the_call_depth_limit_is_five_frames() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let scenarios = scenarios();
    let relay = &scenarios["ballistaRelay"];
    let mut scene = Scene::new(&snapshot, &scenarios);
    let claimed = 42;
    let chain = |scene: &Scene, relays| {
        scene.relays(
            relay,
            relays,
            scene.claim,
            RunInputs::new().u64(claimed).finish(),
            vec![],
        )
    };

    let outcome = scene
        .send_all(&[chain(&scene, 4)])
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(tx::deepest_invocation(&outcome.logs), 5);
    assert_eq!(returned(&outcome.return_data, &ballista_sdk::ID), claimed);
    println!(
        "four relays over returnClaim: {} CU, {} bytes",
        outcome.compute_units, outcome.size
    );

    let failure = scene.send_all(&[chain(&scene, 5)]).unwrap_err();
    assert_eq!(
        (failure.program, &failure.err),
        (
            ballista_sdk::ID,
            &TransactionError::InstructionError(0, InstructionError::CallDepth)
        ),
        "{failure:?}"
    );
    assert_eq!(tx::deepest_invocation(&failure.logs), 5);
}
