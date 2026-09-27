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
        snapshot::{Leg, Snapshot, SNAPSHOT_DIR},
        template::{examples, upload, Example, Run},
        tx::{self, Failure, Outcome},
        wallet::{self, fund, keypair, token_account, token_balance, SOL},
    },
    ballista_sdk::TOKEN_PROGRAM_ID,
    litesvm::LiteSVM,
    solana_address::Address,
    solana_keypair::Keypair,
    solana_signer::Signer,
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
    fn new(snapshot: &'a Snapshot, example: &'a Example) -> Self {
        let mut svm = snapshot.svm();
        let seller = wallet::wallet();
        let creator = keypair(b"ballista-protocol-tests-creator1");
        for key in [seller.pubkey(), creator.pubkey()] {
            fund(&mut svm, &key, 10 * SOL);
        }
        let template = upload(&mut svm, &creator, TEMPLATE_ID, &example.payload);
        Sweep {
            example,
            leg: &snapshot.route(ROUTE).legs[0],
            jupiter: snapshot.named("jupiter"),
            usdc: snapshot.named("usdcMint"),
            svm,
            seller,
            template,
        }
    }

    /// Gives the seller exactly `balance` USDC (write rule 1), then sells it in Jupiter's own
    /// transaction with the run in place of `route`: the compute budget, the setup that creates
    /// the wrapped SOL account, the run, and the cleanup that unwraps it.
    fn sell(&mut self, balance: u64) -> Result<Outcome, Failure> {
        self.sell_with_slippage(balance, self.leg.route.slippage_bps)
    }

    /// [`Sweep::sell`] with a slippage other than the quote's.
    fn sell_with_slippage(&mut self, balance: u64, slippage_bps: u16) -> Result<Outcome, Failure> {
        let leg = self.leg;
        let seller = self.seller.pubkey();
        let source = token_account(&mut self.svm, &seller, &self.usdc, balance);
        assert_eq!(source, leg.source_token_account);
        let swap = &leg.instructions.swap;
        // The template passes `route`'s first four accounts itself; the rest are the group.
        let head: Vec<Address> = swap.accounts[..4].iter().map(|meta| meta.pubkey).collect();
        assert_eq!(
            head,
            [
                TOKEN_PROGRAM_ID,
                seller,
                leg.source_token_account,
                leg.destination_token_account
            ]
        );
        let run = Run::new(self.template, self.example)
            .account("jupiter", self.jupiter, false, false)
            .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
            .account("seller", seller, true, true)
            .account("sourceAta", leg.source_token_account, true, false)
            .account("destinationAta", leg.destination_token_account, true, false)
            .input_bytes("routePlan", &leg.route.route_plan)
            .input_u64("quotedInAmount", leg.route.in_amount)
            .input_u64("quotedOutAmount", leg.route.quoted_out_amount)
            .input_u64("slippageBps", u64::from(slippage_bps))
            .input_u64("platformFeeBps", u64::from(leg.route.platform_fee_bps))
            .input_u64("dustFloor", DUST_FLOOR)
            .group("routeAccounts", swap.accounts[4..].iter().cloned())
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
    fn assert_nothing_sold(&self, balance: u64, lamports_before: u64, failure: &Failure) {
        assert_eq!(
            token_balance(&self.svm, &self.leg.source_token_account),
            balance
        );
        assert_eq!(
            self.svm.get_account(&self.leg.destination_token_account),
            None
        );
        assert_eq!(self.lamports(), lamports_before - failure.fee);
    }
}

/// The least a sale of `balance` may fetch: the quote rescaled to `balance`, less the quote's
/// slippage.
fn least_proceeds(leg: &Leg, balance: u64) -> u64 {
    let rescaled = u128::from(leg.route.quoted_out_amount) * u128::from(balance)
        / u128::from(leg.route.in_amount);
    let least = rescaled * u128::from(10_000 - leg.route.slippage_bps) / 10_000;
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
            0
        );
        // The cleanup closed the wrapped SOL account, paying the seller the proceeds and the rent
        // the setup had taken.
        assert_eq!(
            sweep.svm.get_account(&sweep.leg.destination_token_account),
            None
        );
        let proceeds = sweep.lamports() + outcome.fee - before;
        let least = least_proceeds(sweep.leg, balance);
        assert!(
            proceeds >= least,
            "selling {balance} fetched {proceeds} lamports, under the rescaled quote's {least}\n{outcome:?}"
        );
        println!(
            "sold {balance} for {proceeds} lamports (at least {least}): {} CU, {} bytes",
            outcome.compute_units, outcome.size
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

/// The route, not the template, bounds how far above the quote a balance may go. Ten thousand
/// times the quote would carry the price past the tick arrays the route's accounts include, so
/// Raydium refuses inside the run, and the seller keeps the balance. The measurement below finds
/// where the bounds lie.
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

/// Measures how far above the quote a balance may go, for `findings/token-sweep.md`: the largest
/// balance that lands with the quote's own slippage, then with the slippage relaxed so that only
/// the route's tick arrays bound it, each searched to 1 USDC. Run it again after refreshing the
/// snapshot:
///
/// ```text
/// cargo test --manifest-path tests/protocols/Cargo.toml --test token_sweep -- --ignored --nocapture
/// ```
#[test]
#[ignore = "a measurement for the findings, not a check"]
fn measure_how_far_above_the_quote_a_balance_may_go() {
    const USDC: u64 = 1_000_000;
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let leg = &snapshot.route(ROUTE).legs[0];
    let quoted = leg.route.in_amount;
    for slippage_bps in [leg.route.slippage_bps, 500] {
        let sell =
            |balance: u64| Sweep::new(&snapshot, example).sell_with_slippage(balance, slippage_bps);
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
        let error = failure.logs.iter().find_map(|line| {
            let (_, rest) = line.split_once("Error Code: ")?;
            rest.split('.').next()
        });
        println!(
            "{slippage_bps} bps: {lands} lands ({:.1} times the quote, {} CU); {fails} fails in {} \
             with {:?} {}",
            lands as f64 / quoted as f64,
            outcome.compute_units,
            failure.program,
            failure.code,
            error.unwrap_or("")
        );
    }
}
