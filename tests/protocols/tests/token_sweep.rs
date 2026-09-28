//! `tokenSweepIntoSwap` against the real programs, on route `usdcToSol`: 150 USDC for SOL through
//! Raydium CLMM's SOL/USDC pool.
//!
//! The seller holds a USDC balance other than the one Jupiter quoted, written directly (rule 1).
//! The run passes the quote's route plan and numbers; the template sells the whole balance it
//! reads as `route`'s `in_amount` and rescales the quoted output to match. The route's own setup
//! creates the wrapped SOL account the proceeds land in, and its cleanup unwraps them.
//!
//! What was measured, and how far the balance may drift from the quote: `findings/token-sweep.md`.

use {
    ballista_protocol_tests::{
        snapshot::{jupiter_ran, Leg, Routing, Snapshot, ROUTE_HEAD, SNAPSHOT_DIR},
        template::{examples, upload, Example, Run},
        tx::{self, Failure, Outcome},
        wallet::{
            self, associated_token_address, fund, holding, keypair, token_account, token_balance,
            SOL, WSOL_MINT,
        },
    },
    ballista_sdk::TOKEN_PROGRAM_ID,
    litesvm::LiteSVM,
    solana_address::Address,
    solana_keypair::Keypair,
    solana_signer::Signer,
    std::collections::BTreeMap,
};

const TEMPLATE: &str = "tokenSweepIntoSwap";
const ROUTE: &str = "usdcToSol";
const TEMPLATE_ID: u16 = 1;
/// 0.01 USDC. The template sells only a balance above this, and leaves at most this behind.
const DUST_FLOOR: u64 = 10_000;
/// Raydium CLMM's `NotEnoughTickArrayAccount`: the swap would cross into a tick array it was not
/// given.
const NOT_ENOUGH_TICK_ARRAY_ACCOUNT: u32 = 6023;

/// A fresh SVM from the snapshot, with the template uploaded and the seller funded with SOL for
/// the fee and the wrapped SOL account's rent.
struct Sweep<'a> {
    example: &'a Example,
    leg: &'a Leg,
    jupiter: Address,
    usdc: Address,
    svm: LiteSVM,
    seller: Keypair,
    template: Address,
}

impl<'a> Sweep<'a> {
    /// # Panics
    ///
    /// If the route is not the one these tests were written for: `route`, selling from the
    /// seller's USDC account, through Raydium CLMM.
    fn new(snapshot: &'a Snapshot, example: &'a Example) -> Self {
        let leg = &snapshot.route(ROUTE).legs[0];
        let seller = wallet::wallet();
        let usdc = snapshot.named("usdcMint");
        assert_eq!(
            leg.source_token_account,
            associated_token_address(&seller.pubkey(), &usdc),
            "route {ROUTE} does not sell from the seller's USDC account"
        );
        let swap = &leg.instructions.swap;
        let head: Vec<Address> = swap.accounts[..ROUTE_HEAD]
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
        let raydium = snapshot.named("raydiumClmm");
        assert!(
            swap.accounts.iter().any(|meta| meta.pubkey == raydium),
            "route {ROUTE} no longer goes through Raydium CLMM: a snapshot refresh moved it, so \
             a_balance_past_the_routes_tick_arrays_fails_in_raydium and the figures in \
             findings/token-sweep.md need revisiting"
        );

        let mut svm = snapshot.svm();
        let creator = keypair(b"ballista-protocol-tests-creator1");
        for key in [seller.pubkey(), creator.pubkey()] {
            fund(&mut svm, &key, 10 * SOL);
        }
        let template = upload(&mut svm, &creator, TEMPLATE_ID, &example.payload);
        Sweep {
            example,
            leg,
            jupiter: snapshot.named("jupiter"),
            usdc,
            svm,
            seller,
            template,
        }
    }

    /// Another sweep from this one's state, without loading the snapshot again.
    fn copy(&self) -> Self {
        Sweep {
            svm: self.svm.clone(),
            seller: wallet::wallet(),
            ..*self
        }
    }

    /// Gives the seller exactly `balance` USDC (write rule 1), then sells it in Jupiter's own
    /// transaction with the run in place of `route`: the compute budget, the setup that creates
    /// the wrapped SOL account, the run, and the cleanup that unwraps it.
    fn sell(&mut self, balance: u64) -> Result<Outcome, Failure> {
        self.sell_with(balance, self.leg.route.slippage_bps, DUST_FLOOR)
    }

    /// [`Sweep::sell`] with another slippage than the quote's, or another dust floor.
    fn sell_with(
        &mut self,
        balance: u64,
        slippage_bps: u16,
        dust_floor: u64,
    ) -> Result<Outcome, Failure> {
        self.sell_routed(balance, slippage_bps, dust_floor, Routing::of(self.leg))
    }

    /// [`Sweep::sell_with`] with `routing`'s accounts in place of the route's own.
    fn sell_routed(
        &mut self,
        balance: u64,
        slippage_bps: u16,
        dust_floor: u64,
        routing: Routing,
    ) -> Result<Outcome, Failure> {
        let leg = self.leg;
        let seller = self.seller.pubkey();
        token_account(&mut self.svm, &seller, &self.usdc, balance);
        let run = Run::new(self.template, self.example)
            .account("jupiter", self.jupiter, false, false)
            .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
            .account("seller", seller, true, true)
            .account("sourceAta", routing.source, true, false)
            .account("destinationAta", routing.destination, true, false)
            .input_bytes("routePlan", &leg.route.route_plan)
            .input_u64("quotedInAmount", leg.route.in_amount)
            .input_u64("quotedOutAmount", leg.route.quoted_out_amount)
            .input_u64("slippageBps", u64::from(slippage_bps))
            .input_u64("platformFeeBps", u64::from(leg.route.platform_fee_bps))
            .input_u64("dustFloor", dust_floor)
            .group("routeAccounts", routing.steps)
            .build();
        tx::send(
            &mut self.svm,
            &self.seller,
            &[],
            &leg.instructions.with_swap(run),
            &leg.lookup_tables,
        )
    }

    fn lamports(&self) -> u64 {
        self.svm.get_balance(&self.seller.pubkey()).unwrap()
    }

    /// After a failed sale: the transaction reverted whole. The seller still holds `balance`, the
    /// setup's wrapped SOL account is gone with it, and only the fee was paid.
    #[track_caller]
    fn assert_nothing_sold(&self, balance: u64, lamports_before: u64, failure: &Failure) {
        assert_eq!(
            token_balance(&self.svm, &self.leg.source_token_account),
            balance,
            "the failed sale of {balance} moved some of it"
        );
        assert_eq!(
            self.svm.get_account(&self.leg.destination_token_account),
            None,
            "the failed sale of {balance} left the setup's wrapped SOL account behind"
        );
        assert_eq!(
            self.lamports(),
            lamports_before - failure.fee,
            "the failed sale of {balance} cost the seller more or less than the fee"
        );
    }
}

/// The quote rescaled to `balance`, as the template rescales it.
fn rescaled_quote(leg: &Leg, balance: u64) -> u64 {
    let rescaled = u128::from(leg.route.quoted_out_amount) * u128::from(balance)
        / u128::from(leg.route.in_amount);
    u64::try_from(rescaled).unwrap()
}

/// The least a sale of `balance` may fetch: the rescaled quote, less the quote's slippage.
fn least_proceeds(leg: &Leg, balance: u64) -> u64 {
    let least = u128::from(rescaled_quote(leg, balance))
        * u128::from(10_000 - leg.route.slippage_bps)
        / 10_000;
    u64::try_from(least).unwrap()
}

/// The whole balance is sold, whether it is above or below the quoted one, and fetches at least
/// the quote rescaled to it, less the quote's slippage.
#[test]
fn sells_a_balance_other_than_the_quoted_one() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let quoted = snapshot.route(ROUTE).legs[0].route.in_amount;
    // 3% over and under the quoted 150 USDC, and ten times it.
    for balance in [quoted * 103 / 100, quoted * 97 / 100, quoted * 10] {
        let mut sweep = Sweep::new(&snapshot, example);
        let before = sweep.lamports();
        let outcome = sweep
            .sell(balance)
            .unwrap_or_else(|failure| panic!("selling {balance}: {failure:?}"));

        // All of it was `in_amount`, so nothing is left, let alone more than the dust floor.
        assert_eq!(
            token_balance(&sweep.svm, &sweep.leg.source_token_account),
            0,
            "selling {balance} left some of it behind"
        );
        // The cleanup closed the wrapped SOL account, paying the seller the proceeds and the rent
        // the setup had taken.
        assert_eq!(
            sweep.svm.get_account(&sweep.leg.destination_token_account),
            None,
            "selling {balance} left the wrapped SOL account open"
        );
        let proceeds = sweep.lamports() + outcome.fee - before;
        let least = least_proceeds(sweep.leg, balance);
        assert!(
            proceeds >= least,
            "selling {balance} fetched {proceeds} lamports, under the rescaled quote's {least}\n{outcome:?}"
        );
        println!(
            "sold {balance} for {proceeds} lamports (rescaled quote {}, at least {least}): {} CU, \
             {} bytes",
            rescaled_quote(sweep.leg, balance),
            outcome.compute_units,
            outcome.size
        );
    }
}

/// A balance at the dust floor is not worth selling: the run stops before calling Jupiter.
#[test]
fn a_balance_at_the_dust_floor_is_not_worth_selling() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut sweep = Sweep::new(&snapshot, example);
    let before = sweep.lamports();
    let failure = sweep.sell(DUST_FLOOR).unwrap_err();

    tx::assert_requirement_failed(&failure, example, "worthSelling");
    sweep.assert_nothing_sold(DUST_FLOOR, before, &failure);
}

/// Past the tick arrays the route's accounts include, Raydium refuses inside the run, and the
/// seller keeps the balance. Ten thousand times the quote is far past them. The measurements
/// below find where they end, and where the quote's slippage stops a sale before that.
#[test]
fn a_balance_past_the_routes_tick_arrays_fails_in_raydium() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let balance = snapshot.route(ROUTE).legs[0].route.in_amount * 10_000;
    let mut sweep = Sweep::new(&snapshot, example);
    let before = sweep.lamports();
    let failure = sweep.sell(balance).unwrap_err();

    assert_eq!(
        (failure.program, failure.code),
        (
            snapshot.named("raydiumClmm"),
            Some(NOT_ENOUGH_TICK_ARRAY_ACCOUNT)
        ),
        "{failure:?}"
    );
    sweep.assert_nothing_sold(balance, before, &failure);
}

/// A hostile route's proceeds: an attacker's wrapped SOL account (write rule 1) at
/// `destinationAta`, and in place of the seller's as the Raydium step's output. Jupiter checks
/// `route`'s destination by its mint alone, and the step pays the account it names, so the proceeds
/// reach the attacker, where the template measures them, and meet the quote. The template requires
/// the seller to own the destination, and fails at `proceedsGoToTheSeller`, before the route runs.
/// With the seller's own account at `destinationAta` instead, the step still pays the attacker, and
/// the template sees no proceeds.
#[test]
fn an_attackers_destination_fails_at_proceeds_go_to_the_seller() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut sweep = Sweep::new(&snapshot, example);
    let leg = sweep.leg;
    let balance = leg.route.in_amount;
    let attacker = keypair(b"ballista-protocol-tests-attacker").pubkey();
    let attacker_wsol = token_account(&mut sweep.svm, &attacker, &WSOL_MINT, 0);
    let before = sweep.lamports();

    let sale = sweep.sell_routed(
        balance,
        leg.route.slippage_bps,
        DUST_FLOOR,
        Routing {
            destination: attacker_wsol,
            ..Routing::paying(leg, attacker_wsol)
        },
    );
    let failure = match sale {
        Err(failure) => failure,
        // What the requirement stops: the seller's whole balance buys SOL for the attacker.
        Ok(outcome) => panic!(
            "the sale landed. The attacker's wrapped SOL account holds {}; the seller's USDC \
             account holds {} of the {balance} it held, its wrapped SOL account {}, and its \
             lamports changed by {} (the fee {}). {} CU, {} bytes",
            token_balance(&sweep.svm, &attacker_wsol),
            token_balance(&sweep.svm, &leg.source_token_account),
            holding(&sweep.svm, &leg.destination_token_account),
            i128::from(sweep.lamports()) - i128::from(before),
            outcome.fee,
            outcome.compute_units,
            outcome.size,
        ),
    };
    tx::assert_requirement_failed(&failure, example, "proceedsGoToTheSeller");
    assert!(!jupiter_ran(&sweep.jupiter, &failure), "{failure:?}");
    sweep.assert_nothing_sold(balance, before, &failure);
    assert_eq!(token_balance(&sweep.svm, &attacker_wsol), 0);

    // The seller's own wrapped SOL account where the template measures, the attacker's still in
    // the step.
    let sale = sweep.sell_routed(
        balance,
        leg.route.slippage_bps,
        DUST_FLOOR,
        Routing::paying(leg, attacker_wsol),
    );
    let failure = sale.unwrap_err();
    tx::assert_requirement_failed(&failure, example, "saleMetTheQuote");
    assert_eq!(token_balance(&sweep.svm, &attacker_wsol), 0);
}

/// Another wallet's USDC account (write rule 1) at `sourceAta`, while the route's step still sells
/// from the seller's own. The template holds both ends of the sale to the seller: it fails at
/// `sweepsTheSellersOwnBalance`, before the route runs.
#[test]
fn another_wallets_source_fails_at_sweeps_the_sellers_own_balance() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut sweep = Sweep::new(&snapshot, example);
    let leg = sweep.leg;
    let balance = leg.route.in_amount;
    let other = keypair(b"ballista-protocol-tests-other-01").pubkey();
    let theirs = token_account(&mut sweep.svm, &other, &sweep.usdc, balance);
    let before = sweep.lamports();

    let failure = sweep
        .sell_routed(
            balance,
            leg.route.slippage_bps,
            DUST_FLOOR,
            Routing {
                source: theirs,
                ..Routing::of(leg)
            },
        )
        .unwrap_err();
    tx::assert_requirement_failed(&failure, example, "sweepsTheSellersOwnBalance");
    assert!(!jupiter_ran(&sweep.jupiter, &failure), "{failure:?}");
    sweep.assert_nothing_sold(balance, before, &failure);
    assert_eq!(token_balance(&sweep.svm, &theirs), balance);
}

// ------------------------------------------------------------------------------ measurements
//
// Each ignored test below prints figures in `findings/token-sweep.md` that the checks above do
// not; `sells_a_balance_other_than_the_quoted_one` prints the rest. After refreshing the
// snapshot, rerun them all and update the findings:
//
//     cargo test --manifest-path tests/protocols/Cargo.toml --test token_sweep -- \
//         --include-ignored --nocapture

/// Who refused a sale: `Jupiter 6001`, `Raydium CLMM 6023 NotEnoughTickArrayAccount` (an Anchor
/// program logs its error's name), or `Ballista RequirementFailed in saleMetTheQuote`.
fn refusal(snapshot: &Snapshot, example: &Example, failure: &Failure) -> String {
    if let Some((kind, context)) = tx::ballista_error(failure) {
        // Only some failures' context is a program counter, which names a step; a requirement's
        // is. Any other failure prints its context as it is.
        return match kind {
            "RequirementFailed" => format!(
                "Ballista {kind} in {}",
                example.label_at(context).unwrap_or("an unlabelled step")
            ),
            _ => format!("Ballista {kind} ({context})"),
        };
    }
    let program = [("Jupiter", "jupiter"), ("Raydium CLMM", "raydiumClmm")]
        .into_iter()
        .find(|(_, name)| snapshot.named(name) == failure.program)
        .map_or_else(
            || failure.program.to_string(),
            |(label, _)| label.to_string(),
        );
    let name = failure.logs.iter().find_map(|line| {
        let (_, rest) = line.split_once("Error Code: ")?;
        rest.split('.').next()
    });
    let mut parts = vec![program];
    parts.extend(failure.code.map(|code| code.to_string()));
    parts.extend(name.map(str::to_string));
    parts.join(" ")
}

/// How far above the quote a balance may go: the largest balance that lands with the quote's own
/// slippage, then with the slippage relaxed so that only the route's tick arrays bound it, each
/// searched to 1 USDC. It also counts who refused the sales that failed along the way.
#[test]
#[ignore = "a measurement for the findings, not a check"]
fn measure_how_far_above_the_quote_a_balance_may_go() {
    const USDC: u64 = 1_000_000;
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let start = Sweep::new(&snapshot, example);
    let quoted = start.leg.route.in_amount;
    let mut refused: BTreeMap<String, u32> = BTreeMap::new();
    for slippage_bps in [start.leg.route.slippage_bps, 500] {
        let mut sell = |balance: u64| {
            let sale = start.copy().sell_with(balance, slippage_bps, DUST_FLOOR);
            if let Err(failure) = &sale {
                *refused
                    .entry(refusal(&snapshot, example, failure))
                    .or_default() += 1;
            }
            sale
        };
        // Double the balance until a sale fails, then halve the gap.
        let mut lands = quoted;
        let mut fails = quoted;
        let mut failure = loop {
            fails = fails.checked_mul(2).expect("every balance landed");
            match sell(fails) {
                Ok(_) => lands = fails,
                Err(failure) => break failure,
            }
        };
        while fails - lands > USDC {
            let middle = lands + (fails - lands) / 2;
            match sell(middle) {
                Ok(_) => lands = middle,
                Err(next) => (fails, failure) = (middle, next),
            }
        }
        let outcome = sell(lands).unwrap();
        println!(
            "{slippage_bps} bps: {lands} lands ({:.1} times the quote, {} CU); {fails} is refused \
             by {}",
            lands as f64 / quoted as f64,
            outcome.compute_units,
            refusal(&snapshot, example, &failure)
        );
    }
    for (refusal, count) in refused {
        println!("{count} sales refused by {refusal}");
    }
}

/// The smallest balances the route can sell. With a dust floor of 0 the template lets any balance
/// through, and rounding decides whether the route can fill it. Prints each run of consecutive
/// sizes, from 1 unit to 1,000, that ended the same way, then a few sizes from just above the
/// tests' dust floor to half the quote.
#[test]
#[ignore = "a measurement for the findings, not a check"]
fn measure_the_smallest_balance_the_route_can_sell() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let start = Sweep::new(&snapshot, example);
    let quoted = start.leg.route.in_amount;
    let sell = |balance: u64| match start
        .copy()
        .sell_with(balance, start.leg.route.slippage_bps, 0)
    {
        Ok(_) => "lands".to_string(),
        Err(failure) => format!("refused by {}", refusal(&snapshot, example, &failure)),
    };
    let mut runs: Vec<(u64, u64, String)> = Vec::new();
    for balance in 1..=1_000 {
        let ending = sell(balance);
        match runs.last_mut() {
            Some((_, last, previous)) if *previous == ending => *last = balance,
            _ => runs.push((balance, balance, ending)),
        }
    }
    for (first, last, ending) in runs {
        println!("{first} to {last} units: {ending}");
    }
    for balance in [DUST_FLOOR + 1, 100_000, 1_000_000, quoted / 10, quoted / 2] {
        println!("{balance} units: {}", sell(balance));
    }
}

/// Compute units: the route's own limit; Jupiter's own transaction at the quoted size; the run at
/// 1, 100 and 1,000 times the quote; and every `consumed` line of the sale at 3% over, from which
/// the findings split its cost by program. Builtins log no such line, so the compute budget's
/// share is what the lines leave of the total.
#[test]
#[ignore = "a measurement for the findings, not a check"]
fn measure_compute_units() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let start = Sweep::new(&snapshot, example);
    let leg = start.leg;
    let quoted = leg.route.in_amount;

    // `SetComputeUnitLimit`: tag 2, then a little-endian u32.
    let limit = leg
        .instructions
        .compute_budget
        .iter()
        .find_map(|instruction| {
            let (&2, limit) = instruction.data.split_first()? else {
                return None;
            };
            Some(u32::from_le_bytes(limit.try_into().ok()?))
        });
    println!("the route's compute-unit limit: {limit:?}");

    let mut plain = start.copy();
    token_account(&mut plain.svm, &plain.seller.pubkey(), &plain.usdc, quoted);
    let outcome = tx::send(
        &mut plain.svm,
        &plain.seller,
        &[],
        &leg.instructions.all(),
        &leg.lookup_tables,
    )
    .unwrap();
    println!(
        "Jupiter's own transaction at the quoted size: {} CU, {} bytes",
        outcome.compute_units, outcome.size
    );

    for times in [1, 100, 1_000] {
        let outcome = start.copy().sell(quoted * times).unwrap();
        println!(
            "the run at {times} times the quote: {} CU, {} bytes",
            outcome.compute_units, outcome.size
        );
    }

    let outcome = start.copy().sell(quoted * 103 / 100).unwrap();
    println!(
        "the run at 3% over the quote: {} CU, of which",
        outcome.compute_units
    );
    for line in outcome
        .logs
        .iter()
        .filter(|line| line.contains(" consumed "))
    {
        println!("  {line}");
    }
}
