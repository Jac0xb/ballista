//! `pythFreshPriceGate` against the real programs: route `solToUsdc`, 1 SOL for USDC through
//! Meteora DLMM, run only while the snapshot's Pyth SOL/USD price is fresh, confident and in band.
//!
//! The gate passes `route` its token program and signer itself and forwards the rest of the
//! route's accounts as its group. The run takes `route`'s place in Jupiter's own transaction:
//! Jupiter's setup wraps the SOL and creates the USDC account before it, and its cleanup closes the
//! wrapped SOL account after.

use {
    ballista_protocol_tests::{
        oracle::{copy_pyth_feed, pyth_price, SOL_USD_FEED_ID, USDC_USD_FEED_ID},
        snapshot::{warp, Leg, Snapshot, SNAPSHOT_DIR},
        template::{examples, upload, Example, Run},
        tx::{self, assert_requirement_failed, Failure, Outcome},
        wallet::{self, fund, keypair, token_balance, SOL},
    },
    ballista_sdk::TOKEN_PROGRAM_ID,
    litesvm::LiteSVM,
    solana_address::Address,
    solana_clock::Clock,
    solana_instruction::Instruction,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const TEMPLATE: &str = "pythFreshPriceGate";
const ROUTE: &str = "solToUsdc";
const TEMPLATE_ID: u16 = 7;
/// The accounts at the head of `route`'s list that the gate passes itself: the token program and
/// the signer. The rest arrive as `actionAccounts`.
const ROUTE_HEAD: usize = 2;
/// A minute, in seconds.
const MAXIMUM_AGE: i64 = 60;
/// $0.10 at the feed's exponent of −8.
const MAXIMUM_CONFIDENCE: u64 = 10_000_000;
/// $100 and $150 at the feed's exponent of −8.
const FLOOR_PRICE: i64 = 10_000_000_000;
const CEILING_PRICE: i64 = 15_000_000_000;

/// What a run asks of the price.
#[derive(Clone, Copy, Debug)]
struct Terms {
    price_update: Address,
    feed_id: Address,
    maximum_age: i64,
    maximum_confidence: u64,
    floor_price: i64,
    ceiling_price: i64,
}

/// A fresh SVM with the template uploaded and the actor holding SOL, which Jupiter's setup wraps.
struct Gate {
    svm: LiteSVM,
    actor: Keypair,
    template: Address,
    leg: Leg,
    jupiter: Address,
    /// The snapshot's SOL/USD price update.
    sol_usd: Address,
}

impl Gate {
    fn new(snapshot: &Snapshot, example: &Example) -> Gate {
        let mut svm = snapshot.svm();
        let actor = wallet::wallet();
        let creator = keypair(b"ballista-protocol-tests-creator1");
        for key in [actor.pubkey(), creator.pubkey()] {
            fund(&mut svm, &key, 10 * SOL);
        }
        let template = upload(&mut svm, &creator, TEMPLATE_ID, &example.payload);
        let leg = snapshot.route(ROUTE).legs[0].clone();
        let head: Vec<Address> = leg.instructions.swap.accounts[..ROUTE_HEAD]
            .iter()
            .map(|meta| meta.pubkey)
            .collect();
        assert_eq!(head, [TOKEN_PROGRAM_ID, actor.pubkey()]);
        Gate {
            svm,
            actor,
            template,
            leg,
            jupiter: snapshot.named("jupiter"),
            sol_usd: snapshot.named("pythSolUsd"),
        }
    }

    /// The terms a trader selling SOL would set: the SOL/USD price, at most a minute old, confident
    /// to $0.10, between $100 and $150.
    fn terms(&self) -> Terms {
        Terms {
            price_update: self.sol_usd,
            feed_id: SOL_USD_FEED_ID,
            maximum_age: MAXIMUM_AGE,
            maximum_confidence: MAXIMUM_CONFIDENCE,
            floor_price: FLOOR_PRICE,
            ceiling_price: CEILING_PRICE,
        }
    }

    fn run(&self, example: &Example, terms: Terms) -> Instruction {
        let swap = &self.leg.instructions.swap;
        Run::new(self.template, example)
            .account("priceUpdate", terms.price_update, false, false)
            .account("actionProgram", self.jupiter, false, false)
            .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
            .account("actor", self.actor.pubkey(), true, true)
            .input_pubkey("feedId", terms.feed_id)
            .input_i64("maximumAge", terms.maximum_age)
            .input_u64("maximumConfidence", terms.maximum_confidence)
            .input_i64("floorPrice", terms.floor_price)
            .input_i64("ceilingPrice", terms.ceiling_price)
            .input_bytes("actionData", &self.leg.route.args)
            .group("actionAccounts", swap.accounts[ROUTE_HEAD..].to_vec())
            .build()
    }

    /// Sends a run on `terms` in `route`'s place, between Jupiter's own setup and cleanup.
    fn act(&mut self, example: &Example, terms: Terms) -> Result<Outcome, Failure> {
        let instructions = self.leg.instructions.with_swap(self.run(example, terms));
        tx::send(
            &mut self.svm,
            &self.actor,
            &[],
            &instructions,
            &self.leg.lookup_tables,
        )
    }

    /// Moves the clock on by `seconds`, and the slot at mainnet's 400 ms a slot (write rule 3).
    fn wait(&mut self, seconds: u64) {
        warp(&mut self.svm, seconds * 5 / 2, seconds);
    }

    /// The route ran: Jupiter paid at least its own slippage floor into the USDC account, and the
    /// cleanup closed the wrapped SOL account the route emptied. Returns what it paid.
    fn assert_the_route_ran(&self) -> u64 {
        let leg = &self.leg;
        let received = token_balance(&self.svm, &leg.destination_token_account);
        assert!(
            received >= leg.other_amount_threshold,
            "bought {received} USDC units, under Jupiter's floor of {}",
            leg.other_amount_threshold
        );
        assert_eq!(self.svm.get_account(&leg.source_token_account), None);
        received
    }
}

/// Jupiter's own transaction for the leg, in a fresh SVM: the run's baseline. Returns its outcome
/// and the USDC it bought.
fn jupiter_alone(snapshot: &Snapshot, leg: &Leg) -> (Outcome, u64) {
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
    (outcome, bought)
}

#[test]
fn a_fresh_price_in_band_lets_the_route_run() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut gate = Gate::new(&snapshot, example);
    let price = pyth_price(&gate.svm, &gate.sol_usd);
    let age = gate.svm.get_sysvar::<Clock>().unix_timestamp - price.publish_time;
    // The snapshot's price meets the terms with room to spare.
    assert!(
        (0..MAXIMUM_AGE).contains(&age)
            && price.conf < MAXIMUM_CONFIDENCE
            && price.exponent == -8
            && (FLOOR_PRICE + 1..CEILING_PRICE).contains(&price.price),
        "{age} s old: {price:?}"
    );

    let outcome = gate
        .act(example, gate.terms())
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    let received = gate.assert_the_route_ran();

    // The gate adds checks, not trades: the route fills as in Jupiter's own transaction.
    let (alone, bought_alone) = jupiter_alone(&snapshot, &gate.leg);
    assert_eq!(received, bought_alone);
    let run_units = outcome.compute_units_of(&ballista_sdk::ID).unwrap();
    let route_units = outcome.compute_units_of(&gate.jupiter).unwrap();
    println!(
        "price {} (exponent {}), {age} s old, confidence {}: bought {received} USDC units",
        price.price, price.exponent, price.conf,
    );
    println!(
        "run: {} CU in the transaction, {run_units} in Ballista's run, {route_units} of them \
         Jupiter's; {} bytes",
        outcome.compute_units, outcome.size,
    );
    println!(
        "Jupiter alone: {} CU in the transaction, {} in route; {} bytes",
        alone.compute_units,
        alone.compute_units_of(&gate.jupiter).unwrap(),
        alone.size,
    );
}

/// Freshness is Pyth's own rule, `publish_time + maximum_age >= now`. With the clock moved on
/// until the price is exactly `maximumAge` old, the route still runs; one second later the run
/// fails at `priceIsFresh`.
#[test]
fn a_price_older_than_maximum_age_fails_at_price_is_fresh() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut gate = Gate::new(&snapshot, example);
    let published = pyth_price(&gate.svm, &gate.sol_usd).publish_time;
    let now = gate.svm.get_sysvar::<Clock>().unix_timestamp;
    let until_the_edge = u64::try_from(published + MAXIMUM_AGE - now)
        .expect("the snapshot's price is at most a minute old");

    gate.wait(until_the_edge);
    assert_eq!(
        gate.svm.get_sysvar::<Clock>().unix_timestamp - published,
        MAXIMUM_AGE
    );
    gate.act(example, gate.terms())
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    gate.assert_the_route_ran();

    gate.wait(1);
    let failure = gate.act(example, gate.terms()).unwrap_err();
    assert_requirement_failed(&failure, example, "priceIsFresh");
}

/// The band is inclusive. A price one raw unit, a hundred-millionth of a dollar, outside either
/// end fails at that end's requirement; a band that is the price alone lets the route run.
#[test]
fn a_band_that_excludes_the_price_fails_at_its_edge() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut gate = Gate::new(&snapshot, example);
    let price = pyth_price(&gate.svm, &gate.sol_usd).price;
    let terms = gate.terms();

    let failure = gate
        .act(
            example,
            Terms {
                floor_price: price + 1,
                ..terms
            },
        )
        .unwrap_err();
    assert_requirement_failed(&failure, example, "priceAboveFloor");
    let failure = gate
        .act(
            example,
            Terms {
                ceiling_price: price - 1,
                ..terms
            },
        )
        .unwrap_err();
    assert_requirement_failed(&failure, example, "priceBelowCeiling");

    // A failed run changes nothing but the fee payer's balance.
    let exact = Terms {
        floor_price: price,
        ceiling_price: price,
        ..terms
    };
    gate.act(example, exact)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    gate.assert_the_route_ran();
}

/// A price from another feed is refused, even one inside the band.
///
/// The account is SOL/USD's copied to mainnet's USDC/USD feed, at that feed's address and with its
/// id (write rule 2). Its price is SOL's, so only the feed id tells the two apart.
#[test]
fn a_price_from_another_feed_fails_at_the_feed_pin() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut gate = Gate::new(&snapshot, example);
    let usdc_usd = copy_pyth_feed(&mut gate.svm, &gate.sol_usd, &USDC_USD_FEED_ID);
    let terms = Terms {
        price_update: usdc_usd,
        ..gate.terms()
    };

    let failure = gate.act(example, terms).unwrap_err();
    assert_requirement_failed(&failure, example, "priceIsTheExpectedFeed");

    // Named as what it is, the same account passes every other check: before the pin, nothing told
    // the feeds apart.
    let named = Terms {
        feed_id: USDC_USD_FEED_ID,
        ..terms
    };
    gate.act(example, named)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    gate.assert_the_route_ran();
}
