//! `jupiterOracleCheckedSwap` against the real programs: route `solToUsdc`, 1 SOL for USDC through
//! Meteora DLMM, valued at the snapshot's Pyth SOL/USD price.
//!
//! The template reads the feed's exponent and both mints' decimals itself, so a run passes only the
//! feed id, the route and a tolerance. The run takes `route`'s place in Jupiter's own transaction:
//! Jupiter's setup wraps the SOL and creates the USDC account before it, and its cleanup closes the
//! wrapped SOL account after.

use {
    ballista_protocol_tests::{
        oracle::{
            copy_pyth_feed, pyth_price, set_pyth_price, PythPrice, SOL_USD_FEED_ID,
            USDC_USD_FEED_ID,
        },
        snapshot::{Leg, Snapshot, SNAPSHOT_DIR},
        template::{examples, upload, Example, Run},
        tx::{self, assert_requirement_failed, Failure, Outcome},
        wallet::{self, fund, keypair, token_balance, SOL},
    },
    ballista_sdk::TOKEN_PROGRAM_ID,
    litesvm::LiteSVM,
    solana_address::Address,
    solana_instruction::Instruction,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const TEMPLATE: &str = "jupiterOracleCheckedSwap";
const ROUTE: &str = "solToUsdc";
const TEMPLATE_ID: u16 = 5;
/// 1%: how far below the oracle's valuation the fill may land.
const TOLERANCE_BPS: u64 = 100;
/// The accounts at the head of `route`'s list that the template passes itself: the token program,
/// the trader, and the two token accounts it measures. The rest arrive as `routeAccounts`.
const ROUTE_HEAD: usize = 4;
/// SPL Token `Mint`: `decimals` is the byte at 44.
const MINT_DECIMALS: usize = 44;

/// A fresh SVM with the template uploaded and the trader holding SOL, which Jupiter's setup wraps.
struct Swap {
    svm: LiteSVM,
    trader: Keypair,
    template: Address,
    leg: Leg,
    jupiter: Address,
    /// The snapshot's SOL/USD price update.
    sol_usd: Address,
}

impl Swap {
    fn new(snapshot: &Snapshot, example: &Example) -> Swap {
        let mut svm = snapshot.svm();
        let trader = wallet::wallet();
        let creator = keypair(b"ballista-protocol-tests-creator1");
        for key in [trader.pubkey(), creator.pubkey()] {
            fund(&mut svm, &key, 10 * SOL);
        }
        let template = upload(&mut svm, &creator, TEMPLATE_ID, &example.payload);
        let leg = snapshot.route(ROUTE).legs[0].clone();
        let head: Vec<Address> = leg.instructions.swap.accounts[..ROUTE_HEAD]
            .iter()
            .map(|meta| meta.pubkey)
            .collect();
        assert_eq!(
            head,
            [
                TOKEN_PROGRAM_ID,
                trader.pubkey(),
                leg.source_token_account,
                leg.destination_token_account
            ]
        );
        Swap {
            svm,
            trader,
            template,
            leg,
            jupiter: snapshot.named("jupiter"),
            sol_usd: snapshot.named("pythSolUsd"),
        }
    }

    /// The run, valued at the price in `price_update`, which it requires to carry `feed_id`.
    fn run(
        &self,
        example: &Example,
        price_update: Address,
        feed_id: Address,
        tolerance_bps: u64,
    ) -> Instruction {
        let leg = &self.leg;
        Run::new(self.template, example)
            .account("jupiter", self.jupiter, false, false)
            .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
            .account("priceUpdate", price_update, false, false)
            .account("trader", self.trader.pubkey(), true, true)
            .account("sourceAta", leg.source_token_account, true, false)
            .account("destinationAta", leg.destination_token_account, true, false)
            .account("sourceMint", leg.input_mint, false, false)
            .account("destinationMint", leg.output_mint, false, false)
            .input_pubkey("feedId", feed_id)
            .input_bytes("routeArgs", &leg.route.args)
            .input_u64("toleranceBps", tolerance_bps)
            .group(
                "routeAccounts",
                leg.instructions.swap.accounts[ROUTE_HEAD..].to_vec(),
            )
            .build()
    }

    /// The run at the SOL/USD price update, as a trader selling SOL would send it.
    fn sol_usd_run(&self, example: &Example) -> Instruction {
        self.run(example, self.sol_usd, SOL_USD_FEED_ID, TOLERANCE_BPS)
    }

    /// Sends `run` in `route`'s place, between Jupiter's own setup and cleanup.
    fn send(&mut self, run: Instruction) -> Result<Outcome, Failure> {
        let instructions = self.leg.instructions.with_swap(run);
        tx::send(
            &mut self.svm,
            &self.trader,
            &[],
            &instructions,
            &self.leg.lookup_tables,
        )
    }

    /// The USDC the trader holds: after a run that landed, the fill.
    fn usdc(&self) -> u64 {
        token_balance(&self.svm, &self.leg.destination_token_account)
    }

    /// The power of ten the template scales `sold × price` by: the destination's decimals plus the
    /// feed's exponent less the source's decimals, each read from the SVM as the template reads
    /// them. SOL has 9, USDC 6, and SOL/USD's exponent is −8, so −11.
    fn scale(&self, price_update: &Address) -> i32 {
        let decimals =
            |mint: &Address| i32::from(self.svm.get_account(mint).unwrap().data[MINT_DECIMALS]);
        decimals(&self.leg.output_mint) + pyth_price(&self.svm, price_update).exponent
            - decimals(&self.leg.input_mint)
    }
}

/// The template's floor: `sold × price × 10^scale`, less `tolerance_bps`, rounded down at each
/// step as `multiplyDivide` rounds.
fn oracle_floor(sold: u64, price: i64, scale: i32, tolerance_bps: u64) -> u128 {
    let value = u128::from(sold) * u128::try_from(price).expect("a positive price");
    let power = 10u128.pow(scale.unsigned_abs());
    let valued = if scale >= 0 {
        value * power
    } else {
        value / power
    };
    valued * u128::from(10_000 - tolerance_bps) / 10_000
}

/// The highest price whose floor a fill of `received` for `sold` still clears. The floor never
/// falls as the price rises, so this searches for the edge.
fn highest_price_cleared(sold: u64, received: u64, scale: i32, tolerance_bps: u64) -> i64 {
    let clears = |price| oracle_floor(sold, price, scale, tolerance_bps) <= u128::from(received);
    let (mut cleared, mut refused) = (0, i64::MAX);
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
fn the_fill_clears_the_oracle_floor() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut swap = Swap::new(&snapshot, example);
    let market = pyth_price(&swap.svm, &swap.sol_usd);
    let scale = swap.scale(&swap.sol_usd);
    assert_eq!(scale, -11);

    let run = swap.sol_usd_run(example);
    let outcome = swap
        .send(run)
        .unwrap_or_else(|failure| panic!("{failure:?}"));

    let leg = &swap.leg;
    let received = swap.usdc();
    let floor = oracle_floor(leg.in_amount, market.price, scale, TOLERANCE_BPS);
    assert!(
        u128::from(received) >= floor && received >= leg.other_amount_threshold,
        "bought {received} USDC units, under the oracle's floor of {floor} or Jupiter's {}",
        leg.other_amount_threshold
    );
    // The cleanup closed the wrapped SOL account, which the route had emptied.
    assert_eq!(swap.svm.get_account(&leg.source_token_account), None);

    // The run fills exactly as Jupiter's own transaction does: the template changes what is
    // checked, not what is traded.
    let (alone, bought_alone) = jupiter_alone(&snapshot, leg);
    assert_eq!(received, bought_alone);
    let jupiter = swap.jupiter;
    let run_units = outcome.compute_units_of(&ballista_sdk::ID).unwrap();
    let route_units = outcome.compute_units_of(&jupiter).unwrap();
    println!(
        "sold {} lamports for {received} USDC units; floor {floor} at price {} (exponent {})",
        leg.in_amount, market.price, market.exponent,
    );
    println!(
        "run: {} CU in the transaction, {run_units} in Ballista's run, {route_units} of them \
         Jupiter's; {} bytes",
        outcome.compute_units, outcome.size,
    );
    println!(
        "Jupiter alone: {} CU in the transaction, {} in route; {} bytes",
        alone.compute_units,
        alone.compute_units_of(&jupiter).unwrap(),
        alone.size,
    );
}

/// The floor is exact. At the highest oracle price the fill still clears, the floor equals the fill
/// and the run lands; one unit higher, a hundred-millionth of a dollar, the floor is one USDC base
/// unit more and the run fails at `fillBeatTheOracle`. So the template's on-chain decimals and
/// exponent, and both of its rounding steps, agree with [`oracle_floor`] to the last unit.
#[test]
fn the_floor_is_exact_to_the_last_unit() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    // The fill does not depend on the oracle, so a run at the snapshot's price measures it.
    let mut probe = Swap::new(&snapshot, example);
    let run = probe.sol_usd_run(example);
    probe
        .send(run)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    let received = probe.usdc();
    let sold = probe.leg.in_amount;
    let scale = probe.scale(&probe.sol_usd);
    let highest = highest_price_cleared(sold, received, scale, TOLERANCE_BPS);
    assert_eq!(
        oracle_floor(sold, highest, scale, TOLERANCE_BPS),
        u128::from(received)
    );
    assert_eq!(
        oracle_floor(sold, highest + 1, scale, TOLERANCE_BPS),
        u128::from(received) + 1
    );

    let mut swap = Swap::new(&snapshot, example);
    let market = pyth_price(&swap.svm, &swap.sol_usd);
    // Write rule 2. The fill is about the snapshot's price, so the edge sits about the 1% tolerance
    // above it.
    assert!((market.price..market.price * 102 / 100).contains(&highest));
    set_pyth_price(
        &mut swap.svm,
        &swap.sol_usd,
        PythPrice {
            price: highest + 1,
            ..market
        },
    );
    let failure = swap.send(swap.sol_usd_run(example)).unwrap_err();
    assert_requirement_failed(&failure, example, "fillBeatTheOracle");

    // A failed transaction changes nothing but the fee payer's balance, so the same fill follows.
    set_pyth_price(
        &mut swap.svm,
        &swap.sol_usd,
        PythPrice {
            price: highest,
            ..market
        },
    );
    swap.send(swap.sol_usd_run(example))
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(swap.usdc(), received);
}

/// The oracle 5% above the market the route fills at (write rule 2): the swap runs, the fill falls
/// short of the oracle's valuation, and the whole transaction reverts at `fillBeatTheOracle`.
#[test]
fn an_oracle_above_the_market_fails_the_fill_check() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut swap = Swap::new(&snapshot, example);
    let market = pyth_price(&swap.svm, &swap.sol_usd);
    set_pyth_price(
        &mut swap.svm,
        &swap.sol_usd,
        PythPrice {
            price: market.price * 105 / 100,
            ..market
        },
    );
    let trader = swap.trader.pubkey();
    let before = swap.svm.get_balance(&trader).unwrap();

    let failure = swap.send(swap.sol_usd_run(example)).unwrap_err();
    assert_requirement_failed(&failure, example, "fillBeatTheOracle");
    // The check came after a swap that completed: Jupiter's `route` returned before it.
    let jupiter_returned = format!("Program {} success", swap.jupiter);
    assert!(failure.logs.contains(&jupiter_returned), "{failure:?}");
    // Everything reverted but the fee, the setup's wrapped SOL and USDC accounts included.
    let leg = &swap.leg;
    assert_eq!(swap.svm.get_account(&leg.source_token_account), None);
    assert_eq!(swap.svm.get_account(&leg.destination_token_account), None);
    assert_eq!(swap.svm.get_balance(&trader), Some(before - failure.fee));
}

/// A price from another feed is refused before anything else reads it.
///
/// The account is mainnet's USDC/USD feed, written by copying SOL/USD's with that feed's id and
/// address and giving it USDC's price of $0.9999 (write rule 2). The receiver owns both and both
/// are fully verified, so only the feed id tells them apart.
#[test]
fn a_price_from_another_feed_fails_at_the_feed_pin() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut swap = Swap::new(&snapshot, example);
    let usdc_usd = copy_pyth_feed(&mut swap.svm, &swap.sol_usd, &USDC_USD_FEED_ID);
    let copied = pyth_price(&swap.svm, &usdc_usd);
    set_pyth_price(
        &mut swap.svm,
        &usdc_usd,
        PythPrice {
            price: 99_990_000,
            conf: 70_000,
            ..copied
        },
    );

    let run = swap.run(example, usdc_usd, SOL_USD_FEED_ID, TOLERANCE_BPS);
    let failure = swap.send(run).unwrap_err();
    assert_requirement_failed(&failure, example, "priceIsTheExpectedFeed");

    // What the pin stops. Told to expect USDC/USD, the template values the SOL sold at $0.9999,
    // and a fill of a dollar would clear that. Before the pin, nothing read the feed id, so the run
    // above landed like this one.
    let run = swap.run(example, usdc_usd, USDC_USD_FEED_ID, TOLERANCE_BPS);
    swap.send(run)
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    let floor = oracle_floor(
        swap.leg.in_amount,
        99_990_000,
        swap.scale(&usdc_usd),
        TOLERANCE_BPS,
    );
    assert_eq!(floor, 989_901);
    assert!(u128::from(swap.usdc()) > 100 * floor);
}
