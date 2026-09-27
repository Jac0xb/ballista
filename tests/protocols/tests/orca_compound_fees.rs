//! `orcaCompoundFees` against the real Whirlpool program. Fees come from real swaps through the
//! SOL/USDC pool, and the template has to collect and reinvest them.

use {
    ballista_protocol_tests::{
        orca::{self, Nft, Pool, Position, PositionState, TokenWallet, SOL_USDC, USDC, WHIRLPOOL},
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

/// The pool as snapshotted, a position 60 ticks either side of the price holding a quarter of the
/// pool's liquidity, a trader, and the template.
struct Setup {
    svm: LiteSVM,
    pool: Pool,
    owner: TokenWallet,
    trader: TokenWallet,
    position: Position,
    template: Address,
}

fn setup(example: &Example, funded: bool) -> Setup {
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
    let mint = keypair(&orca::seed("compound position"));
    let range = orca::range(&svm, &pool, -60, 60);
    let position = orca::open_position(&mut svm, &pool, &owner, &mint, range, Nft::Token);
    if funded {
        let liquidity = orca::whirlpool(&svm, &SOL_USDC).liquidity / 4;
        orca::deposit(&mut svm, &pool, &owner, &position, liquidity);
    }
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
    orca::swap(&mut setup.svm, &setup.pool, &setup.trader, 200 * SOL, true);
    orca::swap(
        &mut setup.svm,
        &setup.pool,
        &setup.trader,
        30_000 * USDC,
        false,
    );
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
    assert_eq!(orca::cpis_to(&outcome.logs, &WHIRLPOOL), 3);
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
        orca::cpis_to(&outcome.logs, &WHIRLPOOL),
        2,
        "update and collect only"
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
    assert_eq!(
        orca::cpis_to(&outcome.logs, &WHIRLPOOL),
        1,
        "the update only"
    );
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

    assert_eq!(orca::cpis_to(&outcome.logs, &WHIRLPOOL), 0);
}

#[test]
fn a_price_move_inside_the_bounds_still_lands() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example, true);
    orca::swap(&mut setup.svm, &setup.pool, &setup.trader, 200 * SOL, true);
    orca::swap(
        &mut setup.svm,
        &setup.pool,
        &setup.trader,
        30_000 * USDC,
        false,
    );
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
    orca::swap(&mut setup.svm, &setup.pool, &setup.trader, 200 * SOL, true);
    orca::swap(
        &mut setup.svm,
        &setup.pool,
        &setup.trader,
        30_000 * USDC,
        false,
    );
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
    orca::swap(&mut setup.svm, &setup.pool, &setup.trader, 200 * SOL, true);
    orca::swap(
        &mut setup.svm,
        &setup.pool,
        &setup.trader,
        30_000 * USDC,
        false,
    );
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
    assert_eq!(
        orca::cpis_to(&outcome.logs, &WHIRLPOOL),
        1,
        "the collect only"
    );
}
