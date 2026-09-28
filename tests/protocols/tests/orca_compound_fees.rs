//! `orcaCompoundFees` against the real Whirlpool program. Fees come from real swaps through the
//! SOL/USDC pool, and the template has to collect and reinvest them.

use {
    ballista_protocol_tests::{
        orca::{
            self, Nft, Pool, Position, PositionState, TokenWallet, COLLECT_FEES, SOL_USDC, UNNAMED,
            UPDATE_FEES, USDC, WHIRLPOOL,
        },
        template::{examples, upload, Example, Run},
        tx::{self, ballista_error, Failure, Outcome},
        wallet::{fund, keypair, token_balance, SOL},
    },
    ballista_sdk::TOKEN_PROGRAM_ID,
    litesvm::LiteSVM,
    orca_whirlpools_core as oq,
    solana_address::Address,
    solana_signer::Signer,
};

const EXAMPLE: &str = "orcaCompoundFees";

/// The pool as snapshotted, a position of the owner's, a trader, and the template.
struct Setup {
    svm: LiteSVM,
    pool: Pool,
    owner: TokenWallet,
    trader: TokenWallet,
    position: Position,
    template: Address,
}

/// A position 60 ticks either side of the price, holding a quarter of the pool's liquidity when
/// `funded`.
fn setup(example: &Example, funded: bool) -> Setup {
    setup_with(example, |svm, pool, owner| {
        let mint = keypair(&orca::seed("compound position"));
        let range = orca::range(svm, pool, -60, 60);
        let position = orca::open_position(svm, pool, owner, &mint, range, Nft::Token);
        if funded {
            let liquidity = orca::whirlpool(svm, &SOL_USDC).liquidity / 4;
            orca::deposit(svm, pool, owner, &position, liquidity);
        }
        position
    })
}

/// A full-range position holding a thousandth of the pool's liquidity. The tick arrays at the ends
/// of the range are far from the price and not in the snapshot, so Orca's own
/// `initialize_tick_array` creates them.
fn full_range_setup(example: &Example) -> Setup {
    setup_with(example, |svm, pool, owner| {
        let range = orca::full_range(pool);
        for tick in [range.0, range.1] {
            orca::initialize_tick_array(svm, pool, &owner.keypair, tick);
        }
        let mint = keypair(&orca::seed("compound full range"));
        let position = orca::open_position(svm, pool, owner, &mint, range, Nft::Token);
        let liquidity = orca::whirlpool(svm, &SOL_USDC).liquidity / 1_000;
        orca::deposit(svm, pool, owner, &position, liquidity);
        position
    })
}

/// The pool as snapshotted, funded wallets, the position `open` opens for the owner, and the
/// template.
fn setup_with(
    example: &Example,
    open: impl FnOnce(&mut LiteSVM, &Pool, &TokenWallet) -> Position,
) -> Setup {
    let mut svm = orca::svm();
    let pool = orca::pool(&svm, SOL_USDC);
    let owner = orca::token_wallet(
        &mut svm,
        &orca::seed("compound owner"),
        10_000 * SOL,
        2_000_000 * USDC,
    );
    let trader = orca::token_wallet(
        &mut svm,
        &orca::seed("compound trader"),
        100_000 * SOL,
        20_000_000 * USDC,
    );
    let position = open(&mut svm, &pool, &owner);
    let creator = keypair(b"ballista-protocol-tests-creator1");
    fund(&mut svm, &creator.pubkey(), 10 * SOL);
    let template = upload(&mut svm, &creator, 1, &example.payload);
    Setup {
        svm,
        pool,
        owner,
        trader,
        position,
        template,
    }
}

/// Real trades that pay the position fees in both tokens: 200 SOL sold, then 30,000 USDC.
fn earn_both_fees(setup: &mut Setup) {
    orca::swap(&mut setup.svm, &setup.pool, &setup.trader, 200 * SOL, true);
    orca::swap(
        &mut setup.svm,
        &setup.pool,
        &setup.trader,
        30_000 * USDC,
        false,
    );
}

/// The sqrt-price bounds Orca's SDK would sign with at `bps` of slippage.
fn slippage_bounds(setup: &Setup, bps: u16) -> (u128, u128) {
    let sqrt_price = orca::whirlpool(&setup.svm, &SOL_USDC).sqrt_price;
    let bounds = oq::get_sqrt_price_slippage_bounds(sqrt_price, bps);
    (bounds.min_sqrt_price, bounds.max_sqrt_price)
}

/// Runs the template for the setup's position, signed by its owner.
fn compound(
    setup: &mut Setup,
    example: &Example,
    dust_floor: u64,
    (min_sqrt_price, max_sqrt_price): (u128, u128),
) -> Result<Outcome, Failure> {
    let Setup {
        pool,
        owner,
        position,
        ..
    } = &*setup;
    let run = Run::new(setup.template, example)
        .account("whirlpoolProgram", WHIRLPOOL, false, false)
        .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
        .account("memoProgram", orca::MEMO_PROGRAM, false, false)
        .account("positionAuthority", owner.keypair.pubkey(), false, true)
        .account("whirlpool", pool.address, true, false)
        .account("position", position.address, true, false)
        .account("positionTokenAccount", position.token_account, false, false)
        .account("tokenMintA", pool.mint_a, false, false)
        .account("tokenMintB", pool.mint_b, false, false)
        .account("tokenOwnerAccountA", owner.token_a, true, false)
        .account("tokenOwnerAccountB", owner.token_b, true, false)
        .account("tokenVaultA", pool.vault_a, true, false)
        .account("tokenVaultB", pool.vault_b, true, false)
        .account(
            "tickArrayLower",
            orca::tick_array(pool, position.lower),
            true,
            false,
        )
        .account(
            "tickArrayUpper",
            orca::tick_array(pool, position.upper),
            true,
            false,
        )
        .input_u64("dustFloor", dust_floor)
        .input_u128("minSqrtPrice", min_sqrt_price)
        .input_u128("maxSqrtPrice", max_sqrt_price)
        .build();
    let payer = setup.owner.keypair.insecure_clone();
    tx::send(&mut setup.svm, &payer, &[], &[run], &[])
}

fn balances(setup: &Setup) -> (u64, u64) {
    (
        token_balance(&setup.svm, &setup.owner.token_a),
        token_balance(&setup.svm, &setup.owner.token_b),
    )
}

fn state(setup: &Setup) -> PositionState {
    orca::position_state(&setup.svm, &setup.position)
}

#[test]
fn two_sided_fees_are_collected_and_compounded() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example, true);
    earn_both_fees(&mut setup);
    let before = state(&setup);
    // M2: the position does not show what it earned until something updates it.
    assert_eq!((before.fee_owed_a, before.fee_owed_b), (0, 0));
    let (owed_a, owed_b) = orca::fees_owed_now(&setup.svm, &setup.pool, &setup.position);
    let sqrt_price = orca::whirlpool(&setup.svm, &SOL_USDC).sqrt_price;
    let (lower, upper) = (setup.position.lower, setup.position.upper);
    let from_a =
        oq::increase_liquidity_quote_a(owed_a, 0, sqrt_price, lower, upper, None, None).unwrap();
    let from_b =
        oq::increase_liquidity_quote_b(owed_b, 0, sqrt_price, lower, upper, None, None).unwrap();
    let (a0, b0) = balances(&setup);

    let bounds = slippage_bounds(&setup, 100);
    let outcome =
        compound(&mut setup, example, 0, bounds).unwrap_or_else(|failure| panic!("{failure:?}"));

    let after = state(&setup);
    assert_eq!(
        (after.fee_owed_a, after.fee_owed_b),
        (0, 0),
        "the fees were collected"
    );
    // M4: the program buys the most liquidity both fees allow at the price it runs at.
    assert_eq!(
        after.liquidity - before.liquidity,
        from_a.liquidity_delta.min(from_b.liquidity_delta)
    );
    let (a1, b1) = balances(&setup);
    let (kept_a, kept_b) = (a1 - a0, b1 - b0);
    assert!(
        kept_a < owed_a && kept_b < owed_b,
        "some of each fee was deposited"
    );
    assert!(
        kept_a == 0 || kept_b == 0,
        "one fee was used whole: kept ({kept_a}, {kept_b})"
    );
    // The deposit, `increase_liquidity_by_token_amounts_v2`, logs no name.
    assert_eq!(
        orca::whirlpool_calls(&outcome.logs),
        [UPDATE_FEES, COLLECT_FEES, UNNAMED]
    );
    println!(
        "two-sided: {} CU, {} bytes; fees ({owed_a}, {owed_b}), kept ({kept_a}, {kept_b})",
        outcome.compute_units, outcome.size
    );
}

/// A position that earned in one token only: collected, not compounded, and the run lands.
fn one_sided(a_to_b: bool, label: &str) {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example, true);
    // Fees are paid in the token sold: A for A-to-B swaps, B for B-to-A.
    let amount = if a_to_b { 50 * SOL } else { 5_000 * USDC };
    orca::swap(&mut setup.svm, &setup.pool, &setup.trader, amount, a_to_b);
    let owed = orca::fees_owed_now(&setup.svm, &setup.pool, &setup.position);
    assert_eq!(owed.0 == 0, !a_to_b, "{owed:?}");
    assert_eq!(owed.1 == 0, a_to_b, "{owed:?}");
    let before = state(&setup);
    let (a0, b0) = balances(&setup);

    let bounds = slippage_bounds(&setup, 100);
    let outcome =
        compound(&mut setup, example, 0, bounds).unwrap_or_else(|failure| panic!("{failure:?}"));

    let (a1, b1) = balances(&setup);
    assert_eq!((a1 - a0, b1 - b0), owed, "the whole fee reached the wallet");
    let after = state(&setup);
    assert_eq!(after.liquidity, before.liquidity, "nothing was reinvested");
    assert_eq!((after.fee_owed_a, after.fee_owed_b), (0, 0));
    assert_eq!(
        orca::whirlpool_calls(&outcome.logs),
        [UPDATE_FEES, COLLECT_FEES]
    );
    println!("{label}: {} CU, fees {owed:?}", outcome.compute_units);
}

#[test]
fn fees_in_token_a_alone_are_collected_not_compounded() {
    one_sided(true, "A only");
}

#[test]
fn fees_in_token_b_alone_are_collected_not_compounded() {
    one_sided(false, "B only");
}

#[test]
fn no_fees_lands_without_collecting() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example, true);
    let before = (state(&setup), balances(&setup));

    let bounds = slippage_bounds(&setup, 100);
    let outcome =
        compound(&mut setup, example, 0, bounds).unwrap_or_else(|failure| panic!("{failure:?}"));

    assert_eq!((state(&setup), balances(&setup)), before);
    assert_eq!(orca::whirlpool_calls(&outcome.logs), [UPDATE_FEES]);
    println!("no fees: {} CU", outcome.compute_units);
}

#[test]
fn a_position_without_liquidity_lands() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example, false);

    let bounds = slippage_bounds(&setup, 100);
    let outcome =
        compound(&mut setup, example, 0, bounds).unwrap_or_else(|failure| panic!("{failure:?}"));

    assert_eq!(orca::whirlpool_calls(&outcome.logs), [""; 0]);
}

#[test]
fn a_price_move_inside_the_bounds_still_lands() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example, true);
    earn_both_fees(&mut setup);
    let signed_with = slippage_bounds(&setup, 100);
    // Someone trades between signing and landing.
    orca::swap(
        &mut setup.svm,
        &setup.pool,
        &setup.trader,
        1_000 * SOL,
        true,
    );
    let before = state(&setup);

    compound(&mut setup, example, 0, signed_with).unwrap_or_else(|failure| panic!("{failure:?}"));

    assert!(state(&setup).liquidity > before.liquidity);
}

#[test]
fn a_price_outside_the_bounds_fails_in_whirlpools() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example, true);
    earn_both_fees(&mut setup);
    let sqrt_price = orca::whirlpool(&setup.svm, &SOL_USDC).sqrt_price;
    let before = (state(&setup), balances(&setup));

    let failure = compound(&mut setup, example, 0, (sqrt_price + 1, sqrt_price + 2)).unwrap_err();

    assert_eq!(
        (failure.program, failure.code),
        (WHIRLPOOL, Some(6069)),
        "PriceSlippageOutOfBounds: {failure:?}"
    );
    assert_eq!(ballista_error(&failure), None);
    // The update and the collect reverted with it.
    assert_eq!((state(&setup), balances(&setup)), before);
}

#[test]
fn an_emptied_position_is_collected_not_refilled() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example, true);
    earn_both_fees(&mut setup);
    // Withdrawing records what the position earned.
    orca::withdraw_all(&mut setup.svm, &setup.pool, &setup.owner, &setup.position);
    let emptied = state(&setup);
    assert_eq!(emptied.liquidity, 0);
    assert!(emptied.fee_owed_a > 0 && emptied.fee_owed_b > 0);
    let (a0, b0) = balances(&setup);

    let bounds = slippage_bounds(&setup, 100);
    let outcome =
        compound(&mut setup, example, 0, bounds).unwrap_or_else(|failure| panic!("{failure:?}"));

    assert_eq!(
        state(&setup),
        PositionState {
            liquidity: 0,
            fee_owed_a: 0,
            fee_owed_b: 0
        }
    );
    let (a1, b1) = balances(&setup);
    assert_eq!((a1 - a0, b1 - b0), (emptied.fee_owed_a, emptied.fee_owed_b));
    assert_eq!(orca::whirlpool_calls(&outcome.logs), [COLLECT_FEES]);
}

/// With both fees at or below `dustFloor`, nothing is collected. The update records them, and they
/// stay owed for a later run.
#[test]
fn fees_at_or_below_the_floor_stay_owed() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example, true);
    earn_both_fees(&mut setup);
    let owed = orca::fees_owed_now(&setup.svm, &setup.pool, &setup.position);
    let before = (state(&setup).liquidity, balances(&setup));

    let bounds = slippage_bounds(&setup, 100);
    let floor = owed.0.max(owed.1);
    let outcome = compound(&mut setup, example, floor, bounds)
        .unwrap_or_else(|failure| panic!("{failure:?}"));

    assert_eq!(orca::whirlpool_calls(&outcome.logs), [UPDATE_FEES]);
    assert_eq!((state(&setup).liquidity, balances(&setup)), before);
    let recorded = state(&setup);
    assert_eq!((recorded.fee_owed_a, recorded.fee_owed_b), owed);
}

/// `dustFloor` is compared with each fee in that token's base units. At a floor between the two,
/// the smaller fee is exactly at it, and so dust. The larger one is worth collecting, and
/// `collect_fees` takes both. The deposit needs both above the floor, so it is skipped.
#[test]
fn a_floor_between_the_fees_collects_both_and_reinvests_neither() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example, true);
    earn_both_fees(&mut setup);
    let owed = orca::fees_owed_now(&setup.svm, &setup.pool, &setup.position);
    assert_ne!(owed.0, owed.1, "a floor between the fees needs two fees");
    let before = state(&setup);
    let (a0, b0) = balances(&setup);

    let bounds = slippage_bounds(&setup, 100);
    let floor = owed.0.min(owed.1);
    let outcome = compound(&mut setup, example, floor, bounds)
        .unwrap_or_else(|failure| panic!("{failure:?}"));

    assert_eq!(
        orca::whirlpool_calls(&outcome.logs),
        [UPDATE_FEES, COLLECT_FEES]
    );
    let (a1, b1) = balances(&setup);
    assert_eq!((a1 - a0, b1 - b0), owed, "both fees reached the wallet");
    let after = state(&setup);
    assert_eq!(after.liquidity, before.liquidity, "nothing was reinvested");
    assert_eq!((after.fee_owed_a, after.fee_owed_b), (0, 0));
}

/// A fee too small to buy one unit of liquidity makes the deposit fail with `LiquidityZero`
/// (6012), and the whole run reverts with it. A full range buys the least liquidity per unit of
/// fee, so a full-range position owed a lamport shows it. With the floor at that lamport, the fee
/// is dust: the deposit is skipped, and the run lands.
#[test]
fn dust_that_buys_no_liquidity_fails_the_run_unless_the_floor_skips_it() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = full_range_setup(example);
    // Sell SOL a thousandth at a time until the position is owed a lamport, then sell USDC.
    let mut lots = 0;
    while orca::fees_owed_now(&setup.svm, &setup.pool, &setup.position).0 == 0 {
        assert!(
            lots < 100,
            "100 lots of 0.001 SOL paid the position nothing"
        );
        orca::swap(
            &mut setup.svm,
            &setup.pool,
            &setup.trader,
            SOL / 1_000,
            true,
        );
        lots += 1;
    }
    orca::swap(
        &mut setup.svm,
        &setup.pool,
        &setup.trader,
        100 * USDC,
        false,
    );
    let (owed_a, owed_b) = orca::fees_owed_now(&setup.svm, &setup.pool, &setup.position);
    let sqrt_price = orca::whirlpool(&setup.svm, &SOL_USDC).sqrt_price;
    let (lower, upper) = (setup.position.lower, setup.position.upper);
    let from_a =
        oq::increase_liquidity_quote_a(owed_a, 0, sqrt_price, lower, upper, None, None).unwrap();
    // By Orca's own math the A fee buys no liquidity, and the B fee is larger.
    assert_eq!(from_a.liquidity_delta, 0, "{owed_a} lamports buy liquidity");
    assert!(owed_b > owed_a, "({owed_a}, {owed_b})");
    let before = state(&setup);
    let (a0, b0) = balances(&setup);
    let bounds = slippage_bounds(&setup, 100);

    let failure = compound(&mut setup, example, 0, bounds).unwrap_err();

    assert_eq!(
        (failure.program, failure.code),
        (WHIRLPOOL, Some(6012)),
        "LiquidityZero: {failure:?}"
    );
    assert_eq!(ballista_error(&failure), None);
    // The update and the collect reverted with it.
    assert_eq!((state(&setup), balances(&setup)), (before, (a0, b0)));

    let outcome = compound(&mut setup, example, owed_a, bounds)
        .unwrap_or_else(|failure| panic!("{failure:?}"));

    assert_eq!(
        orca::whirlpool_calls(&outcome.logs),
        [UPDATE_FEES, COLLECT_FEES]
    );
    let (a1, b1) = balances(&setup);
    assert_eq!(
        (a1 - a0, b1 - b0),
        (owed_a, owed_b),
        "both fees reached the wallet"
    );
    let after = state(&setup);
    assert_eq!(after.liquidity, before.liquidity, "nothing was reinvested");
    assert_eq!((after.fee_owed_a, after.fee_owed_b), (0, 0));
    println!("dust: fees ({owed_a}, {owed_b}) after {lots} lots of 0.001 SOL");
}
