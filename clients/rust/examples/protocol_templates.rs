//! The twelve live-protocol templates, authored in Rust with `ProgramBuilder`.
//!
//! Each function builds the same bytes the TypeScript compiler produces for the file of the same
//! name in `clients/js/examples/protocols/`; `tests/protocol_templates.rs` checks every one
//! against `fixtures/protocol-examples.json`.
//!
//! ```bash
//! cargo run -p ballista-sdk --example protocol_templates
//! ```
//!
//! The builder does what the TypeScript compiler does, in the same order: it loads each input the
//! steps use, then each distinct constant, then emits the steps. Registers are numbered in the
//! order they are written, so the calls below follow that order too.

use ballista_sdk::{
    ballista_common::template::*, template_hash, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
};
use solana_program::{pubkey, pubkey::Pubkey};

// Account flags, for both account declarations and CPI account lists.
const READ: u8 = 0;
const WRITE: u8 = ACCOUNT_WRITABLE;
const SIGN: u8 = ACCOUNT_SIGNER;
const PROGRAM: u8 = ACCOUNT_EXECUTABLE;

// Programs. Re-derive these from each protocol's current IDL before uploading.
const JUPITER_V6: Pubkey = pubkey!("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");
const KAMINO_LEND: Pubkey = pubkey!("KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD");
const ORCA_WHIRLPOOL: Pubkey = pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");
const MARGINFI_V2: Pubkey = pubkey!("MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA");
const DRIFT_V2: Pubkey = pubkey!("dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH");
const PYTH_RECEIVER: Pubkey = pubkey!("rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ");

// Layouts.
const TOKEN_ACCOUNT_LEN: u32 = 165;
const TOKEN_AMOUNT: u64 = 64;
const PYTH_LEN: u32 = 134;
const PYTH_VERIFICATION_LEVEL: u64 = 40;
const PYTH_PRICE: u64 = 73;
const PYTH_CONFIDENCE: u64 = 81;
const PYTH_EXPONENT: u64 = 89;
const PYTH_PUBLISH_TIME: u64 = 93;
const ORCA_POSITION_LEN: u32 = 216;
const ORCA_FEE_OWED_A: u64 = 112;
const ORCA_FEE_OWED_B: u64 = 136;

/// Jupiter `route`'s arguments after its discriminator: at most 512 bytes.
const ROUTE_ARGS_MAX: u16 = 512;

/// An Anchor instruction discriminator: the first eight bytes of `sha256("global:<handler>")`.
fn anchor(handler: &str) -> [u8; 8] {
    let hash = solana_sha256_hasher::hash(format!("global:{handler}").as_bytes()).to_bytes();
    hash[..8].try_into().unwrap()
}

fn program(builder: &mut ProgramBuilder, address: Pubkey) -> u8 {
    builder.account(PROGRAM, Some(address.to_bytes()), None, 0)
}

fn token_account(builder: &mut ProgramBuilder) -> u8 {
    builder.account(WRITE, None, Some(TOKEN_PROGRAM_ID.to_bytes()), TOKEN_ACCOUNT_LEN)
}

// #region jupiter-deposit
/// Swap on Jupiter, then deposit exactly what the swap produced into Kamino.
pub fn jupiter_deposit_exact_output() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let jupiter = program(&mut b, JUPITER_V6);
    let kamino = program(&mut b, KAMINO_LEND);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let owner = b.account(SIGN | WRITE, None, None, 0);
    let source_ata = b.account(WRITE, None, None, 0);
    let destination_ata = token_account(&mut b);
    let obligation = b.account(WRITE, None, None, 0);
    let lending_market = b.account(READ, None, None, 0);
    let lending_market_authority = b.account(READ, None, None, 0);
    let reserve = b.account(WRITE, None, None, 0);
    let reserve_liquidity_supply = b.account(WRITE, None, None, 0);
    let reserve_collateral_mint = b.account(WRITE, None, None, 0);
    let reserve_destination_collateral = b.account(WRITE, None, None, 0);
    b.account_groups(1); // routeAccounts
    let route_args = b.input(VALUE_BYTES, ROUTE_ARGS_MAX);
    let minimum_out = b.input(VALUE_U64, 0);

    let route_args = b.load_input(route_args);
    let minimum_out = b.load_input(minimum_out);

    let balance_before = b.read(OP_READ_U64, destination_ata, TOKEN_AMOUNT);
    let route = b.blob(&anchor("route"));
    let swap = b.cpi_with_group(
        jupiter,
        &[
            (token_program, READ),
            (owner, SIGN),
            (source_ata, WRITE),
            (destination_ata, WRITE),
        ],
        &[Segment::Literal(route), Segment::Register(DATA_REG_BYTES, route_args)],
        0,
    );
    b.set_cpi_max_data_len(swap, 8 + ROUTE_ARGS_MAX);
    b.invoke(swap, None);

    let balance_after = b.read(OP_READ_U64, destination_ata, TOKEN_AMOUNT);
    let received = b.binary(OP_SUB, balance_after, balance_before);
    let met_floor = b.binary(OP_GTE, received, minimum_out);
    b.require(met_floor);

    let deposit = b.blob(&anchor("deposit_reserve_liquidity_and_obligation_collateral_v2"));
    let deposit = b.cpi(
        kamino,
        &[
            (owner, SIGN | WRITE),
            (obligation, WRITE),
            (lending_market, READ),
            (lending_market_authority, READ),
            (reserve, WRITE),
            (reserve_liquidity_supply, WRITE),
            (reserve_collateral_mint, WRITE),
            (reserve_destination_collateral, WRITE),
            (destination_ata, WRITE),
            (token_program, READ),
        ],
        &[Segment::Literal(deposit), Segment::Register(DATA_REG_U64, received)],
    );
    b.invoke(deposit, None);
    b.build().unwrap()
}
// #endregion jupiter-deposit

// #region jupiter-oracle-swap
/// Swap on Jupiter and require the fill to beat the Pyth price, less a tolerance.
pub fn jupiter_oracle_checked_swap() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let jupiter = program(&mut b, JUPITER_V6);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let price_update = b.account(READ, None, Some(PYTH_RECEIVER.to_bytes()), PYTH_LEN);
    let trader = b.account(SIGN | WRITE, None, None, 0);
    let source_ata = token_account(&mut b);
    let destination_ata = token_account(&mut b);
    b.account_groups(1); // routeAccounts
    let route_args = b.input(VALUE_BYTES, ROUTE_ARGS_MAX);
    let price_exponent = b.input(VALUE_I64, 0);
    let scale_divisor = b.input(VALUE_U128, 0);
    let tolerance_bps = b.input(VALUE_U64, 0);

    let route_args = b.load_input(route_args);
    let price_exponent = b.load_input(price_exponent);
    let scale_divisor = b.load_input(scale_divisor);
    let tolerance_bps = b.load_input(tolerance_bps);
    let full = b.const_u64(1);
    let sixty = b.const_i64(60);
    let two_pow_32 = b.const_i64(1 << 32);
    let zero = b.const_i64(0);
    let bps = b.const_u64(10_000);
    let bps_wide = b.const_u128(10_000);

    // The verification level decides where every other field sits.
    let level = b.read(OP_READ_U8, price_update, PYTH_VERIFICATION_LEVEL);
    let is_full = b.binary(OP_EQ, level, full);
    b.require(is_full);

    let now = b.clock_timestamp();
    let published = b.read(OP_READ_I64, price_update, PYTH_PUBLISH_TIME);
    let age = b.binary(OP_SUB, now, published);
    let fresh = b.binary(OP_LTE, age, sixty);
    b.require(fresh);

    // The i32 exponent's bits, compared with 2^32 + the exponent the divisor assumes.
    let exponent_bits = b.read(OP_READ_U32, price_update, PYTH_EXPONENT);
    let expected = b.binary(OP_ADD, two_pow_32, price_exponent);
    let expected = b.cast(OP_CAST_U64, expected);
    let same_exponent = b.binary(OP_EQ, exponent_bits, expected);
    b.require(same_exponent);

    let oracle_price = b.read(OP_READ_I64, price_update, PYTH_PRICE);
    let positive = b.binary(OP_GT, oracle_price, zero);
    b.require(positive);

    let source_before = b.read(OP_READ_U64, source_ata, TOKEN_AMOUNT);
    let balance_before = b.read(OP_READ_U64, destination_ata, TOKEN_AMOUNT);

    let route = b.blob(&anchor("route"));
    let swap = b.cpi_with_group(
        jupiter,
        &[
            (token_program, READ),
            (trader, SIGN),
            (source_ata, WRITE),
            (destination_ata, WRITE),
        ],
        &[Segment::Literal(route), Segment::Register(DATA_REG_BYTES, route_args)],
        0,
    );
    b.set_cpi_max_data_len(swap, 8 + ROUTE_ARGS_MAX);
    b.invoke(swap, None);

    let source_after = b.read(OP_READ_U64, source_ata, TOKEN_AMOUNT);
    let sold = b.binary(OP_SUB, source_before, source_after);

    // fairOut = sold × price / divisor × (10,000 − tolerance) / 10,000, in u128.
    let sold_wide = b.cast(OP_CAST_U128, sold);
    let price_wide = b.cast(OP_CAST_U128, oracle_price);
    let value = b.binary(OP_MUL, sold_wide, price_wide);
    let at_oracle = b.binary(OP_DIV, value, scale_divisor);
    let kept_bps = b.binary(OP_SUB, bps, tolerance_bps);
    let kept_bps = b.cast(OP_CAST_U128, kept_bps);
    let scaled = b.binary(OP_MUL, at_oracle, kept_bps);
    let fair_out = b.binary(OP_DIV, scaled, bps_wide);
    let fair_out = b.cast(OP_CAST_U64, fair_out);

    let balance_after = b.read(OP_READ_U64, destination_ata, TOKEN_AMOUNT);
    let received = b.binary(OP_SUB, balance_after, balance_before);
    let beat_oracle = b.binary(OP_GTE, received, fair_out);
    b.require(beat_oracle);
    b.build().unwrap()
}
// #endregion jupiter-oracle-swap

// #region token-sweep
/// Sell a token account's whole balance through Jupiter, rescaling the quote to that balance.
pub fn token_sweep_into_swap() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let jupiter = program(&mut b, JUPITER_V6);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let seller = b.account(SIGN | WRITE, None, None, 0);
    let source_ata = token_account(&mut b);
    let destination_ata = token_account(&mut b);
    b.account_groups(1); // routeAccounts
    let route_plan = b.input(VALUE_BYTES, ROUTE_ARGS_MAX);
    let quoted_in_amount = b.input(VALUE_U64, 0);
    let quoted_out_amount = b.input(VALUE_U64, 0);
    let slippage_bps = b.input(VALUE_U64, 0);
    let platform_fee_bps = b.input(VALUE_U64, 0);
    let dust_floor = b.input(VALUE_U64, 0);

    let route_plan = b.load_input(route_plan);
    let quoted_in_amount = b.load_input(quoted_in_amount);
    let quoted_out_amount = b.load_input(quoted_out_amount);
    let slippage_bps = b.load_input(slippage_bps);
    let platform_fee_bps = b.load_input(platform_fee_bps);
    let dust_floor = b.load_input(dust_floor);
    let bps = b.const_u64(10_000);
    let bps_wide = b.const_u128(10_000);

    let available = b.read(OP_READ_U64, source_ata, TOKEN_AMOUNT);
    let worth_selling = b.binary(OP_GT, available, dust_floor);
    b.require(worth_selling);

    // quotedOut = quotedOutAmount × available / quotedInAmount.
    let out_wide = b.cast(OP_CAST_U128, quoted_out_amount);
    let available_wide = b.cast(OP_CAST_U128, available);
    let product = b.binary(OP_MUL, out_wide, available_wide);
    let in_wide = b.cast(OP_CAST_U128, quoted_in_amount);
    let quoted_out = b.binary(OP_DIV, product, in_wide);
    let quoted_out = b.cast(OP_CAST_U64, quoted_out);

    let proceeds_before = b.read(OP_READ_U64, destination_ata, TOKEN_AMOUNT);

    let route = b.blob(&anchor("route"));
    let sell = b.cpi_with_group(
        jupiter,
        &[
            (token_program, READ),
            (seller, SIGN),
            (source_ata, WRITE),
            (destination_ata, WRITE),
        ],
        &[
            Segment::Literal(route),
            Segment::Register(DATA_REG_BYTES, route_plan),
            Segment::Register(DATA_REG_U64, available),
            Segment::Register(DATA_REG_U64, quoted_out),
            Segment::Register(DATA_REG_U16, slippage_bps),
            Segment::Register(DATA_REG_U8, platform_fee_bps),
        ],
        0,
    );
    b.set_cpi_max_data_len(sell, 8 + ROUTE_ARGS_MAX + 8 + 8 + 2 + 1);
    b.invoke(sell, None);

    let proceeds_after = b.read(OP_READ_U64, destination_ata, TOKEN_AMOUNT);
    let proceeds = b.binary(OP_SUB, proceeds_after, proceeds_before);
    let quoted_wide = b.cast(OP_CAST_U128, quoted_out);
    let kept_bps = b.binary(OP_SUB, bps, slippage_bps);
    let kept_bps = b.cast(OP_CAST_U128, kept_bps);
    let floor = b.binary(OP_MUL, quoted_wide, kept_bps);
    let floor = b.binary(OP_DIV, floor, bps_wide);
    let floor = b.cast(OP_CAST_U64, floor);
    let met_quote = b.binary(OP_GTE, proceeds, floor);
    b.require(met_quote);

    let left_behind = b.read(OP_READ_U64, source_ata, TOKEN_AMOUNT);
    let nothing_left = b.binary(OP_LTE, left_behind, dust_floor);
    b.require(nothing_left);
    b.build().unwrap()
}
// #endregion token-sweep

// #region jito-tip
/// Run a Jupiter strategy, then pay a Jito tip only if the searcher's lamports grew enough.
pub fn jito_profit_guarded_tip() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let system_program = program(&mut b, SYSTEM_PROGRAM_ID);
    let strategy_program = program(&mut b, JUPITER_V6);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let searcher = b.account(SIGN | WRITE, None, None, 0);
    let jito_tip = b.account(WRITE, None, None, 0);
    b.account_groups(1); // strategyAccounts
    let strategy_data = b.input(VALUE_BYTES, ROUTE_ARGS_MAX);
    let tip_lamports = b.input(VALUE_U64, 0);
    let minimum_edge = b.input(VALUE_U64, 0);

    let strategy_data = b.load_input(strategy_data);
    let tip_lamports = b.load_input(tip_lamports);
    let minimum_edge = b.load_input(minimum_edge);

    let lamports_before = b.account_lamports(searcher);
    let route = b.blob(&anchor("route"));
    let strategy = b.cpi_with_group(
        strategy_program,
        &[(token_program, READ), (searcher, SIGN)],
        &[Segment::Literal(route), Segment::Register(DATA_REG_BYTES, strategy_data)],
        0,
    );
    b.set_cpi_max_data_len(strategy, 8 + ROUTE_ARGS_MAX);
    b.invoke(strategy, None);

    let lamports_after = b.account_lamports(searcher);
    let profit = b.binary(OP_SUB, lamports_after, lamports_before);
    let needed = b.binary(OP_ADD, tip_lamports, minimum_edge);
    let covers_tip = b.binary(OP_GTE, profit, needed);
    b.require(covers_tip);

    let transfer = b.blob(&[2, 0, 0, 0]); // SystemInstruction::Transfer
    let tip = b.cpi(
        system_program,
        &[(searcher, SIGN | WRITE), (jito_tip, WRITE)],
        &[Segment::Literal(transfer), Segment::Register(DATA_REG_U64, tip_lamports)],
    );
    b.invoke(tip, None);
    b.build().unwrap()
}
// #endregion jito-tip

// #region pyth-gate
/// Call Jupiter only while a Pyth price is fully verified, fresh, precise and inside a band.
pub fn pyth_fresh_price_gate() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let price_update = b.account(READ, None, Some(PYTH_RECEIVER.to_bytes()), PYTH_LEN);
    let action_program = program(&mut b, JUPITER_V6);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let actor = b.account(SIGN | WRITE, None, None, 0);
    b.account_groups(1); // actionAccounts
    let maximum_age = b.input(VALUE_I64, 0);
    let maximum_confidence = b.input(VALUE_U64, 0);
    let floor_price = b.input(VALUE_I64, 0);
    let ceiling_price = b.input(VALUE_I64, 0);
    let action_data = b.input(VALUE_BYTES, ROUTE_ARGS_MAX);

    let maximum_age = b.load_input(maximum_age);
    let maximum_confidence = b.load_input(maximum_confidence);
    let floor_price = b.load_input(floor_price);
    let ceiling_price = b.load_input(ceiling_price);
    let action_data = b.load_input(action_data);
    let full = b.const_u64(1);

    let level = b.read(OP_READ_U8, price_update, PYTH_VERIFICATION_LEVEL);
    let is_full = b.binary(OP_EQ, level, full);
    b.require(is_full);

    let now = b.clock_timestamp();
    let published = b.read(OP_READ_I64, price_update, PYTH_PUBLISH_TIME);
    let age = b.binary(OP_SUB, now, published);
    let fresh = b.binary(OP_LTE, age, maximum_age);
    b.require(fresh);

    let confidence = b.read(OP_READ_U64, price_update, PYTH_CONFIDENCE);
    let agree = b.binary(OP_LTE, confidence, maximum_confidence);
    b.require(agree);

    let price = b.read(OP_READ_I64, price_update, PYTH_PRICE);
    let above_floor = b.binary(OP_GTE, price, floor_price);
    b.require(above_floor);
    let price = b.read(OP_READ_I64, price_update, PYTH_PRICE);
    let below_ceiling = b.binary(OP_LTE, price, ceiling_price);
    b.require(below_ceiling);

    let route = b.blob(&anchor("route"));
    let act = b.cpi_with_group(
        action_program,
        &[(token_program, READ), (actor, SIGN)],
        &[Segment::Literal(route), Segment::Register(DATA_REG_BYTES, action_data)],
        0,
    );
    b.set_cpi_max_data_len(act, 8 + ROUTE_ARGS_MAX);
    b.invoke(act, None);
    b.build().unwrap()
}
// #endregion pyth-gate

// #region orca-compound
/// Collect an Orca position's fees and add them back as liquidity, when they clear a floor.
pub fn orca_compound_fees() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let whirlpool_program = program(&mut b, ORCA_WHIRLPOOL);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let position_authority = b.account(SIGN, None, None, 0);
    let whirlpool = b.account(WRITE, None, None, 0);
    let position = b.account(WRITE, None, Some(ORCA_WHIRLPOOL.to_bytes()), ORCA_POSITION_LEN);
    let position_token_account = b.account(READ, None, None, 0);
    let token_owner_a = b.account(WRITE, None, None, 0);
    let token_owner_b = b.account(WRITE, None, None, 0);
    let token_vault_a = b.account(WRITE, None, None, 0);
    let token_vault_b = b.account(WRITE, None, None, 0);
    let tick_array_lower = b.account(WRITE, None, None, 0);
    let tick_array_upper = b.account(WRITE, None, None, 0);
    let liquidity_amount = b.input(VALUE_U128, 0);
    let dust_floor = b.input(VALUE_U64, 0);

    let liquidity_amount = b.load_input(liquidity_amount);
    let dust_floor = b.load_input(dust_floor);

    // Read the owed fees before collecting, because collecting zeroes them.
    let owed_a = b.read(OP_READ_U64, position, ORCA_FEE_OWED_A);
    let owed_b = b.read(OP_READ_U64, position, ORCA_FEE_OWED_B);

    let collect = b.blob(&anchor("collect_fees"));
    let collect = b.cpi(
        whirlpool_program,
        &[
            (whirlpool, WRITE),
            (position_authority, SIGN),
            (position, WRITE),
            (position_token_account, READ),
            (token_owner_a, WRITE),
            (token_vault_a, WRITE),
            (token_owner_b, WRITE),
            (token_vault_b, WRITE),
            (token_program, READ),
        ],
        &[Segment::Literal(collect)],
    );
    let worth_it = b.binary(OP_GT, owed_a, dust_floor);
    b.invoke(collect, Some(worth_it));

    let increase = b.blob(&anchor("increase_liquidity"));
    let increase = b.cpi(
        whirlpool_program,
        &[
            (whirlpool, WRITE),
            (token_program, READ),
            (position_authority, SIGN),
            (position, WRITE),
            (position_token_account, READ),
            (token_owner_a, WRITE),
            (token_owner_b, WRITE),
            (token_vault_a, WRITE),
            (token_vault_b, WRITE),
            (tick_array_lower, WRITE),
            (tick_array_upper, WRITE),
        ],
        &[
            Segment::Literal(increase),
            Segment::Register(DATA_REG_U128, liquidity_amount),
            Segment::Register(DATA_REG_U64, owed_a),
            Segment::Register(DATA_REG_U64, owed_b),
        ],
    );
    let worth_it = b.binary(OP_GT, owed_a, dust_floor);
    b.invoke(increase, Some(worth_it));
    b.build().unwrap()
}
// #endregion orca-compound

// #region orca-harvest
/// Collect fees from up to twelve Orca positions, skipping any that earned less than a floor.
pub fn orca_harvest_many_positions() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let whirlpool_program = program(&mut b, ORCA_WHIRLPOOL);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let position_authority = b.account(SIGN, None, None, 0);
    let whirlpool = b.account(WRITE, None, None, 0);
    let token_owner_a = b.account(WRITE, None, None, 0);
    let token_owner_b = b.account(WRITE, None, None, 0);
    let token_vault_a = b.account(WRITE, None, None, 0);
    let token_vault_b = b.account(WRITE, None, None, 0);
    // One row per position: the position and the token account that proves ownership of it.
    let position = b.row_account(WRITE, None, Some(ORCA_WHIRLPOOL.to_bytes()), ORCA_POSITION_LEN);
    let position_token_account = b.row_account(READ, None, None, 0);
    b.batch(12, 1);
    let dust_floor = b.input(VALUE_U64, 0);

    let dust_floor = b.load_input(dust_floor);

    let collect = b.blob(&anchor("collect_fees"));
    let collect = b.cpi(
        whirlpool_program,
        &[
            (whirlpool, WRITE),
            (position_authority, SIGN),
            (position, WRITE),
            (position_token_account, READ),
            (token_owner_a, WRITE),
            (token_vault_a, WRITE),
            (token_owner_b, WRITE),
            (token_vault_b, WRITE),
            (token_program, READ),
        ],
        &[Segment::Literal(collect)],
    );
    b.for_each(0, |body| {
        // This row's own earnings decide whether this row does anything.
        let owed_a = body.read(OP_READ_U64, position, ORCA_FEE_OWED_A);
        let worth_it = body.binary(OP_GT, owed_a, dust_floor);
        body.invoke(collect, Some(worth_it));
    });
    b.build().unwrap()
}
// #endregion orca-harvest

// #region kamino-repay
/// Swap collateral into the borrowed asset on Jupiter, then repay exactly what the swap produced.
pub fn kamino_repay_swap_output() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let jupiter = program(&mut b, JUPITER_V6);
    let kamino = program(&mut b, KAMINO_LEND);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let borrower = b.account(SIGN | WRITE, None, None, 0);
    let collateral_ata = b.account(WRITE, None, None, 0);
    let borrowed_asset_ata = token_account(&mut b);
    let obligation = b.account(WRITE, None, None, 0);
    let lending_market = b.account(READ, None, None, 0);
    let repay_reserve = b.account(WRITE, None, None, 0);
    let reserve_liquidity_supply = b.account(WRITE, None, None, 0);
    let reserve_price_feed = b.account(READ, None, None, 0);
    b.account_groups(1); // routeAccounts
    let route_args = b.input(VALUE_BYTES, ROUTE_ARGS_MAX);
    let minimum_repayment = b.input(VALUE_U64, 0);

    let route_args = b.load_input(route_args);
    let minimum_repayment = b.load_input(minimum_repayment);

    let balance_before = b.read(OP_READ_U64, borrowed_asset_ata, TOKEN_AMOUNT);
    let route = b.blob(&anchor("route"));
    let swap = b.cpi_with_group(
        jupiter,
        &[
            (token_program, READ),
            (borrower, SIGN),
            (collateral_ata, WRITE),
            (borrowed_asset_ata, WRITE),
        ],
        &[Segment::Literal(route), Segment::Register(DATA_REG_BYTES, route_args)],
        0,
    );
    b.set_cpi_max_data_len(swap, 8 + ROUTE_ARGS_MAX);
    b.invoke(swap, None);

    let balance_after = b.read(OP_READ_U64, borrowed_asset_ata, TOKEN_AMOUNT);
    let swapped = b.binary(OP_SUB, balance_after, balance_before);
    let worth_repaying = b.binary(OP_GTE, swapped, minimum_repayment);
    b.require(worth_repaying);

    let refresh = b.blob(&anchor("refresh_reserve"));
    let refresh = b.cpi(
        kamino,
        &[
            (repay_reserve, WRITE),
            (lending_market, READ),
            (reserve_price_feed, READ),
        ],
        &[Segment::Literal(refresh)],
    );
    b.invoke(refresh, None);

    let repay = b.blob(&anchor("repay_obligation_liquidity_v2"));
    let repay = b.cpi(
        kamino,
        &[
            (borrower, SIGN | WRITE),
            (obligation, WRITE),
            (lending_market, READ),
            (repay_reserve, WRITE),
            (reserve_liquidity_supply, WRITE),
            (borrowed_asset_ata, WRITE),
            (token_program, READ),
        ],
        &[Segment::Literal(repay), Segment::Register(DATA_REG_U64, swapped)],
    );
    b.invoke(repay, None);
    b.build().unwrap()
}
// #endregion kamino-repay

// #region kamino-liquidate
/// Refresh, liquidate on Kamino, and require the liquidator to net a minimum in collateral.
pub fn kamino_liquidate_with_proof() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let kamino = program(&mut b, KAMINO_LEND);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let liquidator = b.account(SIGN | WRITE, None, None, 0);
    let obligation = b.account(WRITE, None, None, 0);
    let lending_market = b.account(READ, None, None, 0);
    let lending_market_authority = b.account(READ, None, None, 0);
    let repay_reserve = b.account(WRITE, None, None, 0);
    let repay_reserve_liquidity_supply = b.account(WRITE, None, None, 0);
    let withdraw_reserve = b.account(WRITE, None, None, 0);
    let withdraw_reserve_collateral_mint = b.account(WRITE, None, None, 0);
    let withdraw_reserve_liquidity_supply = b.account(WRITE, None, None, 0);
    let reserve_price_feed = b.account(READ, None, None, 0);
    let user_source_liquidity = b.account(WRITE, None, None, 0);
    let user_destination_collateral = token_account(&mut b);
    let liquidity_amount = b.input(VALUE_U64, 0);
    let min_acceptable_received = b.input(VALUE_U64, 0);
    let minimum_bounty = b.input(VALUE_U64, 0);

    let liquidity_amount = b.load_input(liquidity_amount);
    let min_acceptable_received = b.load_input(min_acceptable_received);
    let minimum_bounty = b.load_input(minimum_bounty);
    let zero = b.const_u64(0);

    let refresh_reserve = b.blob(&anchor("refresh_reserve"));
    let refresh_reserve = b.cpi(
        kamino,
        &[
            (withdraw_reserve, WRITE),
            (lending_market, READ),
            (reserve_price_feed, READ),
        ],
        &[Segment::Literal(refresh_reserve)],
    );
    b.invoke(refresh_reserve, None);

    let refresh_obligation = b.blob(&anchor("refresh_obligation"));
    let refresh_obligation = b.cpi(
        kamino,
        &[(lending_market, READ), (obligation, WRITE)],
        &[Segment::Literal(refresh_obligation)],
    );
    b.invoke(refresh_obligation, None);

    let collateral_before = b.read(OP_READ_U64, user_destination_collateral, TOKEN_AMOUNT);
    let liquidate = b.blob(&anchor("liquidate_obligation_and_redeem_reserve_collateral_v2"));
    let liquidate = b.cpi(
        kamino,
        &[
            (liquidator, SIGN | WRITE),
            (obligation, WRITE),
            (lending_market, READ),
            (lending_market_authority, READ),
            (repay_reserve, WRITE),
            (repay_reserve_liquidity_supply, WRITE),
            (withdraw_reserve, WRITE),
            (withdraw_reserve_collateral_mint, WRITE),
            (withdraw_reserve_liquidity_supply, WRITE),
            (user_source_liquidity, WRITE),
            (user_destination_collateral, WRITE),
            (token_program, READ),
        ],
        &[
            Segment::Literal(liquidate),
            Segment::Register(DATA_REG_U64, liquidity_amount),
            Segment::Register(DATA_REG_U64, min_acceptable_received),
            Segment::Register(DATA_REG_U64, zero), // no LTV override
        ],
    );
    b.invoke(liquidate, None);

    let collateral_after = b.read(OP_READ_U64, user_destination_collateral, TOKEN_AMOUNT);
    let bounty = b.binary(OP_SUB, collateral_after, collateral_before);
    let paid = b.binary(OP_GTE, bounty, minimum_bounty);
    b.require(paid);
    b.build().unwrap()
}
// #endregion kamino-liquidate

// #region marginfi-withdraw
/// Withdraw a whole marginfi position, require a minimum, and sweep it to a treasury.
pub fn marginfi_withdraw_all_with_floor() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let marginfi = program(&mut b, MARGINFI_V2);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let marginfi_group = b.account(READ, None, None, 0);
    let marginfi_account = b.account(WRITE, None, None, 0);
    let authority = b.account(SIGN, None, None, 0);
    let bank = b.account(WRITE, None, None, 0);
    let bank_liquidity_vault = b.account(WRITE, None, None, 0);
    let bank_liquidity_vault_authority = b.account(WRITE, None, None, 0);
    let destination_ata = token_account(&mut b);
    let treasury_ata = token_account(&mut b);
    let minimum_withdrawn = b.input(VALUE_U64, 0);

    let minimum_withdrawn = b.load_input(minimum_withdrawn);
    let zero = b.const_u64(0);

    let balance_before = b.read(OP_READ_U64, destination_ata, TOKEN_AMOUNT);
    let withdraw = b.blob(&anchor("lending_account_withdraw"));
    let withdraw_all = b.blob(&[1, 1]); // Option::Some(true)
    let withdraw = b.cpi(
        marginfi,
        &[
            (marginfi_group, READ),
            (marginfi_account, WRITE),
            (authority, SIGN),
            (bank, WRITE),
            (destination_ata, WRITE),
            (bank_liquidity_vault_authority, WRITE),
            (bank_liquidity_vault, WRITE),
            (token_program, READ),
        ],
        &[
            Segment::Literal(withdraw),
            Segment::Register(DATA_REG_U64, zero), // ignored when withdraw_all is set
            Segment::Literal(withdraw_all),
        ],
    );
    b.invoke(withdraw, None);

    let balance_after = b.read(OP_READ_U64, destination_ata, TOKEN_AMOUNT);
    let withdrawn = b.binary(OP_SUB, balance_after, balance_before);
    let met_floor = b.binary(OP_GTE, withdrawn, minimum_withdrawn);
    b.require(met_floor);

    let transfer = b.blob(&[3]); // SPL Token Transfer
    let sweep = b.cpi(
        token_program,
        &[(destination_ata, WRITE), (treasury_ata, WRITE), (authority, SIGN)],
        &[Segment::Literal(transfer), Segment::Register(DATA_REG_U64, withdrawn)],
    );
    b.invoke(sweep, None);
    b.build().unwrap()
}
// #endregion marginfi-withdraw

// #region drift-rebalance
/// Withdraw everything from marginfi and deposit exactly what came out into Drift.
pub fn drift_rebalance_exact() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let marginfi = program(&mut b, MARGINFI_V2);
    let drift = program(&mut b, DRIFT_V2);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let owner = b.account(SIGN | WRITE, None, None, 0);
    let wallet_ata = token_account(&mut b);
    let marginfi_group = b.account(READ, None, None, 0);
    let marginfi_account = b.account(WRITE, None, None, 0);
    let marginfi_bank = b.account(WRITE, None, None, 0);
    let marginfi_vault = b.account(WRITE, None, None, 0);
    let marginfi_vault_authority = b.account(WRITE, None, None, 0);
    let drift_state = b.account(READ, None, None, 0);
    let drift_user = b.account(WRITE, None, None, 0);
    let drift_user_stats = b.account(WRITE, None, None, 0);
    let drift_spot_market_vault = b.account(WRITE, None, None, 0);
    let minimum_moved = b.input(VALUE_U64, 0);

    let minimum_moved = b.load_input(minimum_moved);
    let zero = b.const_u64(0);

    let wallet_before = b.read(OP_READ_U64, wallet_ata, TOKEN_AMOUNT);
    let withdraw = b.blob(&anchor("lending_account_withdraw"));
    let withdraw_all = b.blob(&[1, 1]); // Option::Some(true)
    let withdraw = b.cpi(
        marginfi,
        &[
            (marginfi_group, READ),
            (marginfi_account, WRITE),
            (owner, SIGN),
            (marginfi_bank, WRITE),
            (wallet_ata, WRITE),
            (marginfi_vault_authority, WRITE),
            (marginfi_vault, WRITE),
            (token_program, READ),
        ],
        &[
            Segment::Literal(withdraw),
            Segment::Register(DATA_REG_U64, zero),
            Segment::Literal(withdraw_all),
        ],
    );
    b.invoke(withdraw, None);

    let wallet_after = b.read(OP_READ_U64, wallet_ata, TOKEN_AMOUNT);
    let moved = b.binary(OP_SUB, wallet_after, wallet_before);
    let worth_it = b.binary(OP_GTE, moved, minimum_moved);
    b.require(worth_it);

    let deposit = b.blob(&anchor("deposit"));
    let market_index = b.blob(&0u16.to_le_bytes()); // spot market 0, USDC on mainnet
    let reduce_only = b.blob(&[0]); // false
    let deposit = b.cpi(
        drift,
        &[
            (drift_state, READ),
            (drift_user, WRITE),
            (drift_user_stats, WRITE),
            (owner, SIGN),
            (drift_spot_market_vault, WRITE),
            (wallet_ata, WRITE),
            (token_program, READ),
        ],
        &[
            Segment::Literal(deposit),
            Segment::Literal(market_index),
            Segment::Register(DATA_REG_U64, moved),
            Segment::Literal(reduce_only),
        ],
    );
    b.invoke(deposit, None);
    b.build().unwrap()
}
// #endregion drift-rebalance

// #region drift-settle
/// Settle a Drift perp position's PnL, then withdraw it with `reduce_only` set.
pub fn drift_settle_when_profitable() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let drift = program(&mut b, DRIFT_V2);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let owner = b.account(SIGN | WRITE, None, None, 0);
    let drift_state = b.account(READ, None, None, 0);
    let drift_user = b.account(WRITE, None, None, 0);
    let drift_user_stats = b.account(WRITE, None, None, 0);
    let drift_spot_market_vault = b.account(WRITE, None, None, 0);
    let drift_signer = b.account(READ, None, None, 0);
    let perp_market = b.account(WRITE, None, None, 0);
    let spot_market = b.account(WRITE, None, None, 0);
    let destination_ata = token_account(&mut b);
    let minimum_settled = b.input(VALUE_U64, 0);

    let minimum_settled = b.load_input(minimum_settled);

    let balance_before = b.read(OP_READ_U64, destination_ata, TOKEN_AMOUNT);
    let settle = b.blob(&anchor("settle_pnl"));
    let perp_market_index = b.blob(&0u16.to_le_bytes());
    let settle = b.cpi(
        drift,
        &[
            (drift_state, READ),
            (owner, SIGN),
            (drift_user, WRITE),
            (spot_market, WRITE),
            (perp_market, WRITE),
        ],
        &[Segment::Literal(settle), Segment::Literal(perp_market_index)],
    );
    b.invoke(settle, None);

    let withdraw = b.blob(&anchor("withdraw"));
    let spot_market_index = b.blob(&0u16.to_le_bytes());
    let reduce_only = b.blob(&[1]); // true: draw down a deposit, never open a borrow
    let withdraw = b.cpi(
        drift,
        &[
            (drift_state, READ),
            (drift_user, WRITE),
            (drift_user_stats, WRITE),
            (owner, SIGN),
            (drift_spot_market_vault, WRITE),
            (drift_signer, READ),
            (destination_ata, WRITE),
            (token_program, READ),
        ],
        &[
            Segment::Literal(withdraw),
            Segment::Literal(spot_market_index),
            Segment::Register(DATA_REG_U64, minimum_settled),
            Segment::Literal(reduce_only),
        ],
    );
    b.invoke(withdraw, None);

    let balance_after = b.read(OP_READ_U64, destination_ata, TOKEN_AMOUNT);
    let settled = b.binary(OP_SUB, balance_after, balance_before);
    let landed = b.binary(OP_GTE, settled, minimum_settled);
    b.require(landed);
    b.build().unwrap()
}
// #endregion drift-settle

/// Every template, under the name `fixtures/protocol-examples.json` records it by.
pub const TEMPLATES: [(&str, fn() -> Vec<u8>); 12] = [
    ("driftRebalanceExact", drift_rebalance_exact),
    ("driftSettleWhenProfitable", drift_settle_when_profitable),
    ("jitoProfitGuardedTip", jito_profit_guarded_tip),
    ("jupiterDepositExactOutput", jupiter_deposit_exact_output),
    ("jupiterOracleCheckedSwap", jupiter_oracle_checked_swap),
    ("kaminoLiquidateWithProof", kamino_liquidate_with_proof),
    ("kaminoRepaySwapOutput", kamino_repay_swap_output),
    ("marginfiWithdrawAllWithFloor", marginfi_withdraw_all_with_floor),
    ("orcaCompoundFees", orca_compound_fees),
    ("orcaHarvestManyPositions", orca_harvest_many_positions),
    ("pythFreshPriceGate", pyth_fresh_price_gate),
    ("tokenSweepIntoSwap", token_sweep_into_swap),
];

#[allow(dead_code)]
fn main() {
    for (name, build) in TEMPLATES {
        let payload = build();
        let stats = ProgramView::parse(&payload)
            .expect("payload parses")
            .verify()
            .expect("payload verifies");
        let hash: String = template_hash(&payload).iter().map(|b| format!("{b:02x}")).collect();
        println!(
            "{name:<30} {:>5} bytes  {:>3} instructions  sha256 {hash}",
            payload.len(),
            stats.instructions,
        );
    }
}
