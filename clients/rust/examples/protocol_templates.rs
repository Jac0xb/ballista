//! The thirteen live-protocol templates, authored in Rust with `ProgramBuilder`.
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
//! order they are written, so the calls below follow that order too. The compiler also records a
//! constant pubkey before any account's address or owner, so a template that compares against
//! one interns it before declaring its accounts.

use ballista_sdk::{
    ballista_common::template::*, template_hash, ProgramBuilder, Segment, ED25519_PROGRAM_ID,
    SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID,
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
const PYTH_RECEIVER: Pubkey = pubkey!("rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ");
/// Owns all eight Jito tip accounts.
const JITO_TIP_PAYMENT: Pubkey = pubkey!("T1pyyaTNZsKv2WcRAB8oVnk93mLJw2XzjtVYqCsaHqt");
/// Every Kamino v2 lending instruction takes the instructions sysvar.
const SYSVAR_INSTRUCTIONS: Pubkey = pubkey!("Sysvar1nstructions1111111111111111111111111");
const WRAPPED_SOL_MINT: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
/// Orca's v2 instructions take the memo program, for Token-2022 transfers that need a memo.
const MEMO_PROGRAM: Pubkey = pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");

// Layouts.
const TOKEN_ACCOUNT_LEN: u32 = 165;
const TOKEN_MINT: u64 = 0;
const TOKEN_OWNER: u64 = 32;
const TOKEN_AMOUNT: u64 = 64;
const MINT_LEN: u32 = 82;
const MINT_DECIMALS: u64 = 44;
const PYTH_LEN: u32 = 134;
// A `Full` price update's offsets; `Partial` shifts each by one, so templates pin the level first.
const PYTH_VERIFICATION_LEVEL: u64 = 40;
const PYTH_FEED_ID: u64 = 41;
const PYTH_PRICE: u64 = 73;
const PYTH_CONFIDENCE: u64 = 81;
const PYTH_EXPONENT: u64 = 89;
const PYTH_PUBLISH_TIME: u64 = 93;
const ORCA_POSITION_LEN: u32 = 216;
const ORCA_LIQUIDITY: u64 = 72;
const ORCA_FEE_OWED_A: u64 = 112;
const ORCA_FEE_OWED_B: u64 = 136;
// The signed quote: 128 bytes, integers little-endian, keys as their 32 raw bytes.
const QUOTE_LEN: u16 = 128;
const QUOTE_TAG: [u8; 8] = *b"BLSTQT01";
const QUOTE_PRICE: u64 = 8;
const QUOTE_MAX_AMOUNT: u64 = 16;
const QUOTE_EXPIRY: u64 = 24;
const QUOTE_TAKER: u64 = 32;
const QUOTE_BASE_MINT: u64 = 64;
const QUOTE_QUOTE_MINT: u64 = 96;
/// Quote prices carry six decimals: 1,000,000 is one quote unit per base unit.
const PRICE_SCALE: u64 = 1_000_000;
// An Ed25519 precompile instruction's header: a u8 signature count and a padding byte, then the
// signature's u16 offsets. These are byte offsets into the instruction's data.
const ED25519_PUBLIC_KEY_OFFSET: u64 = 6;
const ED25519_MESSAGE_DATA_OFFSET: u64 = 10;

/// Jupiter `route`'s arguments after its discriminator, or its `route_plan`: at most 512 bytes.
const ROUTE_ARGS_MAX: u16 = 512;
/// What follows the plan in `route`'s data: `in_amount`, `quoted_out_amount` (u64 each),
/// `slippage_bps` (u16) and `platform_fee_bps` (u8).
const ROUTE_TAIL_LEN: u16 = 8 + 8 + 2 + 1;

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

fn mint(builder: &mut ProgramBuilder) -> u8 {
    builder.account(READ, None, Some(TOKEN_PROGRAM_ID.to_bytes()), MINT_LEN)
}

/// Requires the pubkey at `offset` in `account`'s data to be the address of account `expected`.
fn require_key_at(builder: &mut ProgramBuilder, account: u8, offset: u64, expected: u8) {
    let stored = builder.read(OP_READ_PUBKEY, account, offset);
    let expected = builder.account_key(expected);
    let same = builder.binary(OP_EQ, stored, expected);
    builder.require(same);
}

/// Requires an SPL token account to belong to `owner`.
fn require_owner(builder: &mut ProgramBuilder, token_account: u8, owner: u8) {
    require_key_at(builder, token_account, TOKEN_OWNER, owner);
}

/// The first 16 bytes of an Ed25519 precompile instruction holding one signature whose key,
/// signature and message are in its own data, over `message_len` bytes, as `(mask, expected)`:
/// the signature count, the three instruction indexes (`u16::MAX` names the precompile
/// instruction itself) and the message size, compared as one masked `u128`.
fn ed25519_header(message_len: u16) -> (u128, u128) {
    let fields = [
        (0, 1, 1),                   // signature count
        (4, 2, 0xffff),              // signature instruction index
        (8, 2, 0xffff),              // public key instruction index
        (12, 2, message_len.into()), // message data size
        (14, 2, 0xffff),             // message instruction index
    ];
    let (mut mask, mut expected) = (0u128, 0u128);
    for (offset, width, value) in fields {
        mask |= ((1u128 << (8 * width)) - 1) << (8 * offset);
        expected |= value << (8 * offset);
    }
    (mask, expected)
}

/// Requires an SPL token account to hold `mint`.
fn require_mint(builder: &mut ProgramBuilder, token_account: u8, mint: u8) {
    require_key_at(builder, token_account, TOKEN_MINT, mint);
}

// #region jupiter-deposit
/// Swap on Jupiter, then deposit exactly what the swap produced into Kamino.
///
/// Send it after Kamino's `refresh_reserve`s and `refresh_obligation`, in the same transaction.
pub fn jupiter_deposit_exact_output() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let jupiter = program(&mut b, JUPITER_V6);
    let kamino = program(&mut b, KAMINO_LEND);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let instructions_sysvar = b.account(READ, Some(SYSVAR_INSTRUCTIONS.to_bytes()), None, 0);
    let owner = b.account(SIGN | WRITE, None, None, 0);
    let source_ata = b.account(WRITE, None, None, 0);
    let destination_ata = token_account(&mut b);
    let obligation = b.account(WRITE, None, None, 0);
    let lending_market = b.account(READ, None, None, 0);
    let lending_market_authority = b.account(READ, None, None, 0);
    let reserve = b.account(WRITE, None, None, 0);
    let reserve_liquidity_mint = b.account(READ, None, None, 0);
    let reserve_liquidity_supply = b.account(WRITE, None, None, 0);
    let reserve_collateral_mint = b.account(WRITE, None, None, 0);
    let reserve_destination_collateral = b.account(WRITE, None, None, 0);
    b.account_groups(2); // routeAccounts, farmAccounts
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

    // Kamino's v2 deposit: these 14 accounts, then farmAccounts.
    let deposit = b.blob(&anchor("deposit_reserve_liquidity_and_obligation_collateral_v2"));
    let deposit = b.cpi_with_group(
        kamino,
        &[
            (owner, SIGN | WRITE),
            (obligation, WRITE),
            (lending_market, READ),
            (lending_market_authority, READ),
            (reserve, WRITE),
            (reserve_liquidity_mint, READ),
            (reserve_liquidity_supply, WRITE),
            (reserve_collateral_mint, WRITE),
            (reserve_destination_collateral, WRITE),
            (destination_ata, WRITE),
            (kamino, READ), // placeholder_user_destination_collateral: Kamino's ID means "none"
            (token_program, READ), // collateral_token_program
            (token_program, READ), // liquidity_token_program
            (instructions_sysvar, READ),
        ],
        &[Segment::Literal(deposit), Segment::Register(DATA_REG_U64, received)],
        1,
    );
    b.invoke(deposit, None);
    b.build().unwrap()
}
// #endregion jupiter-deposit

// #region jupiter-oracle-swap
/// Swap on Jupiter and require the fill to beat a Pyth price, less a tolerance.
pub fn jupiter_oracle_checked_swap() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let jupiter = program(&mut b, JUPITER_V6);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let price_update = b.account(READ, None, Some(PYTH_RECEIVER.to_bytes()), PYTH_LEN);
    let trader = b.account(SIGN | WRITE, None, None, 0);
    let source_ata = token_account(&mut b);
    let destination_ata = token_account(&mut b);
    let source_mint = mint(&mut b);
    let destination_mint = mint(&mut b);
    b.account_groups(1); // routeAccounts
    let feed_id = b.input(VALUE_PUBKEY, 0);
    let route_plan = b.input(VALUE_BYTES, ROUTE_ARGS_MAX);
    let in_amount = b.input(VALUE_U64, 0);
    let quoted_out_amount = b.input(VALUE_U64, 0);
    let slippage_bps = b.input(VALUE_U64, 0);
    let platform_fee_bps = b.input(VALUE_U64, 0);
    let tolerance_bps = b.input(VALUE_U64, 0);

    let feed_id = b.load_input(feed_id);
    let route_plan = b.load_input(route_plan);
    let in_amount = b.load_input(in_amount);
    let quoted_out_amount = b.load_input(quoted_out_amount);
    let slippage_bps = b.load_input(slippage_bps);
    let platform_fee_bps = b.load_input(platform_fee_bps);
    let tolerance_bps = b.load_input(tolerance_bps);
    let full = b.const_u64(1);
    let sixty = b.const_i64(60);
    let zero = b.const_i64(0);
    let bps = b.const_u64(10_000);
    let bps_wide = b.const_u128(10_000);

    // The verification level decides where every other field sits.
    let level = b.read(OP_READ_U8, price_update, PYTH_VERIFICATION_LEVEL);
    let is_full = b.binary(OP_EQ, level, full);
    b.require(is_full);

    // The receiver owns every feed's account alike: only the feed id says which price this is.
    let feed = b.read(OP_READ_PUBKEY, price_update, PYTH_FEED_ID);
    let expected_feed = b.binary(OP_EQ, feed, feed_id);
    b.require(expected_feed);

    let now = b.clock_timestamp();
    let published = b.read(OP_READ_I64, price_update, PYTH_PUBLISH_TIME);
    let age = b.binary(OP_SUB, now, published);
    let fresh = b.binary(OP_LTE, age, sixty);
    b.require(fresh);

    // The decimals below are read from these mints, and both accounts are the trader's.
    require_mint(&mut b, source_ata, source_mint);
    require_mint(&mut b, destination_ata, destination_mint);
    require_owner(&mut b, source_ata, trader);
    require_owner(&mut b, destination_ata, trader);

    // scale = destination decimals + the price's exponent − source decimals.
    let destination_decimals = b.read(OP_READ_U8, destination_mint, MINT_DECIMALS);
    let destination_decimals = b.cast(OP_CAST_I64, destination_decimals);
    let exponent = b.read(OP_READ_I32, price_update, PYTH_EXPONENT);
    let scale = b.binary(OP_ADD, destination_decimals, exponent);
    let source_decimals = b.read(OP_READ_U8, source_mint, MINT_DECIMALS);
    let source_decimals = b.cast(OP_CAST_I64, source_decimals);
    let scale = b.binary(OP_SUB, scale, source_decimals);

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
        &[
            Segment::Literal(route),
            Segment::Register(DATA_REG_BYTES, route_plan),
            Segment::Register(DATA_REG_U64, in_amount),
            Segment::Register(DATA_REG_U64, quoted_out_amount),
            Segment::Register(DATA_REG_U16, slippage_bps),
            Segment::Register(DATA_REG_U8, platform_fee_bps),
        ],
        0,
    );
    b.set_cpi_max_data_len(swap, 8 + ROUTE_ARGS_MAX + ROUTE_TAIL_LEN);
    b.invoke(swap, None);

    let source_after = b.read(OP_READ_U64, source_ata, TOKEN_AMOUNT);
    let sold = b.binary(OP_SUB, source_before, source_after);
    let sold_the_input = b.binary(OP_EQ, sold, in_amount);
    b.require(sold_the_input);

    // fairOut = sold × price × 10^max(scale, 0) ÷ 10^max(−scale, 0), less the tolerance.
    let sold_wide = b.cast(OP_CAST_U128, sold);
    let price_wide = b.cast(OP_CAST_U128, oracle_price);
    let value = b.binary(OP_MUL, sold_wide, price_wide);
    let up = b.binary(OP_MAX, scale, zero);
    let up = b.cast(OP_CAST_U64, up);
    let up = b.pow10(up);
    let down = b.binary(OP_SUB, zero, scale);
    let down = b.binary(OP_MAX, down, zero);
    let down = b.cast(OP_CAST_U64, down);
    let down = b.pow10(down);
    let at_oracle = b.mul_div(value, up, down);
    let kept_bps = b.binary(OP_SUB, bps, tolerance_bps);
    let kept_bps = b.cast(OP_CAST_U128, kept_bps);
    let fair_out = b.mul_div(at_oracle, kept_bps, bps_wide);
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

    // Both ends of the sale are the seller's.
    require_owner(&mut b, source_ata, seller);
    require_owner(&mut b, destination_ata, seller);

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
    b.set_cpi_max_data_len(sell, 8 + ROUTE_ARGS_MAX + ROUTE_TAIL_LEN);
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
/// Run a Jupiter round trip from the searcher's wrapped SOL back to it, then pay a Jito tip only
/// if that balance grew by the tip plus a minimum edge.
pub fn jito_profit_guarded_tip() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    // The compiler records a constant pubkey before the accounts' addresses.
    b.pubkey(WRAPPED_SOL_MINT.to_bytes());
    let system_program = program(&mut b, SYSTEM_PROGRAM_ID);
    let strategy_program = program(&mut b, JUPITER_V6);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let searcher = b.account(SIGN | WRITE, None, None, 0);
    let wsol_account = token_account(&mut b);
    let jito_tip = b.account(WRITE, None, Some(JITO_TIP_PAYMENT.to_bytes()), 0);
    b.account_groups(1); // strategyAccounts
    let strategy_data = b.input(VALUE_BYTES, ROUTE_ARGS_MAX);
    let tip_lamports = b.input(VALUE_U64, 0);
    let minimum_edge = b.input(VALUE_U64, 0);

    let strategy_data = b.load_input(strategy_data);
    let tip_lamports = b.load_input(tip_lamports);
    let minimum_edge = b.load_input(minimum_edge);
    let wrapped_sol = b.const_pubkey(WRAPPED_SOL_MINT.to_bytes());

    // Wrapped SOL counts in lamports, so the profit is in the tip's own unit.
    let held = b.read(OP_READ_PUBKEY, wsol_account, TOKEN_MINT);
    let holds_wsol = b.binary(OP_EQ, held, wrapped_sol);
    b.require(holds_wsol);
    // Only profit that reaches the searcher, who pays the tip, may cover it.
    require_owner(&mut b, wsol_account, searcher);

    let balance_before = b.read(OP_READ_U64, wsol_account, TOKEN_AMOUNT);
    let route = b.blob(&anchor("route"));
    let strategy = b.cpi_with_group(
        strategy_program,
        &[
            (token_program, READ),
            (searcher, SIGN),
            (wsol_account, WRITE), // the round trip's source
            (wsol_account, WRITE), // and its destination
        ],
        &[Segment::Literal(route), Segment::Register(DATA_REG_BYTES, strategy_data)],
        0,
    );
    b.set_cpi_max_data_len(strategy, 8 + ROUTE_ARGS_MAX);
    b.invoke(strategy, None);

    // balanceAfter ≥ balanceBefore + tip + edge: a loss fails here instead of underflowing.
    let balance_after = b.read(OP_READ_U64, wsol_account, TOKEN_AMOUNT);
    let needed = b.binary(OP_ADD, balance_before, tip_lamports);
    let needed = b.binary(OP_ADD, needed, minimum_edge);
    let covers_tip = b.binary(OP_GTE, balance_after, needed);
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
/// Call Jupiter only while a Pyth price is the expected feed at the expected exponent, fully
/// verified, fresh, precise and inside a band.
pub fn pyth_fresh_price_gate() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let price_update = b.account(READ, None, Some(PYTH_RECEIVER.to_bytes()), PYTH_LEN);
    let action_program = program(&mut b, JUPITER_V6);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let actor = b.account(SIGN | WRITE, None, None, 0);
    b.account_groups(1); // actionAccounts
    let feed_id = b.input(VALUE_PUBKEY, 0);
    let exponent = b.input(VALUE_I64, 0);
    let maximum_age = b.input(VALUE_I64, 0);
    let maximum_confidence = b.input(VALUE_U64, 0);
    let floor_price = b.input(VALUE_I64, 0);
    let ceiling_price = b.input(VALUE_I64, 0);
    let action_data = b.input(VALUE_BYTES, ROUTE_ARGS_MAX);

    let feed_id = b.load_input(feed_id);
    let exponent = b.load_input(exponent);
    let maximum_age = b.load_input(maximum_age);
    let maximum_confidence = b.load_input(maximum_confidence);
    let floor_price = b.load_input(floor_price);
    let ceiling_price = b.load_input(ceiling_price);
    let action_data = b.load_input(action_data);
    let full = b.const_u64(1);

    let level = b.read(OP_READ_U8, price_update, PYTH_VERIFICATION_LEVEL);
    let is_full = b.binary(OP_EQ, level, full);
    b.require(is_full);

    let feed = b.read(OP_READ_PUBKEY, price_update, PYTH_FEED_ID);
    let expected_feed = b.binary(OP_EQ, feed, feed_id);
    b.require(expected_feed);

    // The bounds are raw integers at this exponent; at another, each is off by a power of ten.
    let stored_exponent = b.read(OP_READ_I32, price_update, PYTH_EXPONENT);
    let expected_exponent = b.binary(OP_EQ, stored_exponent, exponent);
    b.require(expected_exponent);

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
/// Update an Orca position's fees, collect them when either clears a floor, and reinvest them in
/// the position by token amounts. The fee accounts must belong to whoever holds the position's
/// NFT: `positionAuthority` may be only a delegate.
pub fn orca_compound_fees() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let whirlpool_program = program(&mut b, ORCA_WHIRLPOOL);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let memo_program = program(&mut b, MEMO_PROGRAM);
    let position_authority = b.account(SIGN, None, None, 0);
    let whirlpool = b.account(WRITE, None, None, 0);
    let position = b.account(WRITE, None, Some(ORCA_WHIRLPOOL.to_bytes()), ORCA_POSITION_LEN);
    // Token or Token-2022, so only its length is pinned; Whirlpools checks its mint and amount.
    let position_token_account = b.account(READ, None, None, TOKEN_ACCOUNT_LEN);
    let token_mint_a = b.account(READ, None, None, 0);
    let token_mint_b = b.account(READ, None, None, 0);
    let token_owner_a = token_account(&mut b);
    let token_owner_b = token_account(&mut b);
    let token_vault_a = b.account(WRITE, None, None, 0);
    let token_vault_b = b.account(WRITE, None, None, 0);
    let tick_array_lower = b.account(WRITE, None, None, 0);
    let tick_array_upper = b.account(WRITE, None, None, 0);
    let dust_floor = b.input(VALUE_U64, 0);
    let min_sqrt_price = b.input(VALUE_U128, 0);
    let max_sqrt_price = b.input(VALUE_U128, 0);

    let dust_floor = b.load_input(dust_floor);
    let min_sqrt_price = b.load_input(min_sqrt_price);
    let max_sqrt_price = b.load_input(max_sqrt_price);
    let zero = b.const_u128(0);

    // collect_fees checks only the fee accounts' mint, so the template checks their owner.
    let holder = b.read(OP_READ_PUBKEY, position_token_account, TOKEN_OWNER);
    let owner_a = b.read(OP_READ_PUBKEY, token_owner_a, TOKEN_OWNER);
    let pays_holder = b.binary(OP_EQ, owner_a, holder);
    b.require(pays_holder);
    let owner_b = b.read(OP_READ_PUBKEY, token_owner_b, TOKEN_OWNER);
    let pays_holder = b.binary(OP_EQ, owner_b, holder);
    b.require(pays_holder);

    // Fold the pool's fee growth into the position; it fails on a position without liquidity.
    let liquidity = b.read(OP_READ_U128, position, ORCA_LIQUIDITY);
    let has_liquidity = b.binary(OP_GT, liquidity, zero);
    let update = b.blob(&anchor("update_fees_and_rewards"));
    let update = b.cpi(
        whirlpool_program,
        &[
            (whirlpool, WRITE),
            (position, WRITE),
            (tick_array_lower, READ),
            (tick_array_upper, READ),
        ],
        &[Segment::Literal(update)],
    );
    b.invoke(update, Some(has_liquidity));

    // Read after the update, which makes them current, and before the collect, which zeroes them.
    let owed_a = b.read(OP_READ_U64, position, ORCA_FEE_OWED_A);
    let owed_b = b.read(OP_READ_U64, position, ORCA_FEE_OWED_B);
    let earned_a = b.binary(OP_GT, owed_a, dust_floor);
    let earned_b = b.binary(OP_GT, owed_b, dust_floor);

    let collect = b.blob(&anchor("collect_fees"));
    let collect = b.cpi(
        whirlpool_program,
        &[
            (whirlpool, READ),
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
    let either = b.binary(OP_OR, earned_a, earned_b);
    b.invoke(collect, Some(either));

    // The fees as caps: Orca works out the most liquidity they buy at the price when it runs.
    let increase = b.blob(&anchor("increase_liquidity_by_token_amounts_v2"));
    let by_token_amounts = b.blob(&[0]); // IncreaseLiquidityMethod::ByTokenAmounts
    let no_remaining_accounts = b.blob(&[0]); // Option::None
    let increase = b.cpi(
        whirlpool_program,
        &[
            (whirlpool, WRITE),
            (token_program, READ), // token_program_a
            (token_program, READ), // token_program_b
            (memo_program, READ),
            (position_authority, SIGN),
            (position, WRITE),
            (position_token_account, READ),
            (token_mint_a, READ),
            (token_mint_b, READ),
            (token_owner_a, WRITE),
            (token_owner_b, WRITE),
            (token_vault_a, WRITE),
            (token_vault_b, WRITE),
            (tick_array_lower, WRITE),
            (tick_array_upper, WRITE),
        ],
        &[
            Segment::Literal(increase),
            Segment::Literal(by_token_amounts),
            Segment::Register(DATA_REG_U64, owed_a), // token_max_a
            Segment::Register(DATA_REG_U64, owed_b), // token_max_b
            Segment::Register(DATA_REG_U128, min_sqrt_price),
            Segment::Register(DATA_REG_U128, max_sqrt_price),
            Segment::Literal(no_remaining_accounts),
        ],
    );
    // In range, liquidity needs both tokens; an emptied position stays empty.
    let both = b.binary(OP_AND, earned_a, earned_b);
    let reinvest = b.binary(OP_AND, has_liquidity, both);
    b.invoke(increase, Some(reinvest));
    b.build().unwrap()
}
// #endregion orca-compound

// #region orca-harvest
/// Update and collect the fees of up to twelve Orca positions of one holder, skipping any whose
/// fees are all at or below a floor.
pub fn orca_harvest_many_positions() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let whirlpool_program = program(&mut b, ORCA_WHIRLPOOL);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let position_authority = b.account(SIGN, None, None, 0);
    let whirlpool = b.account(WRITE, None, None, 0);
    let token_owner_a = token_account(&mut b);
    let token_owner_b = token_account(&mut b);
    let token_vault_a = b.account(WRITE, None, None, 0);
    let token_vault_b = b.account(WRITE, None, None, 0);
    // One row per position: the position, the token account holding its NFT, and the tick arrays
    // holding its lower and upper ticks.
    let position = b.row_account(WRITE, None, Some(ORCA_WHIRLPOOL.to_bytes()), ORCA_POSITION_LEN);
    let position_token_account = b.row_account(READ, None, None, TOKEN_ACCOUNT_LEN);
    let tick_array_lower = b.row_account(READ, None, None, 0);
    let tick_array_upper = b.row_account(READ, None, None, 0);
    b.batch(12, 1);
    let dust_floor = b.input(VALUE_U64, 0);

    let dust_floor = b.load_input(dust_floor);
    let zero = b.const_u128(0);

    // Every row shares the fee accounts, so their owners are read once.
    let fee_owner_a = b.read(OP_READ_PUBKEY, token_owner_a, TOKEN_OWNER);
    let fee_owner_b = b.read(OP_READ_PUBKEY, token_owner_b, TOKEN_OWNER);
    b.for_each(0, |body| {
        // Each row's position NFT must be held by the fee accounts' owner.
        let holder = body.read(OP_READ_PUBKEY, position_token_account, TOKEN_OWNER);
        let same_a = body.binary(OP_EQ, holder, fee_owner_a);
        body.require(same_a);
        let same_b = body.binary(OP_EQ, holder, fee_owner_b);
        body.require(same_b);

        let update = body.blob(&anchor("update_fees_and_rewards"));
        let update = body.cpi(
            whirlpool_program,
            &[
                (whirlpool, WRITE),
                (position, WRITE),
                (tick_array_lower, READ),
                (tick_array_upper, READ),
            ],
            &[Segment::Literal(update)],
        );
        let liquidity = body.read(OP_READ_U128, position, ORCA_LIQUIDITY);
        let liquid = body.binary(OP_GT, liquidity, zero);
        body.invoke(update, Some(liquid));

        let collect = body.blob(&anchor("collect_fees"));
        let collect = body.cpi(
            whirlpool_program,
            &[
                (whirlpool, READ),
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
        // This row's own fees, just updated, decide whether it collects.
        let owed_a = body.read(OP_READ_U64, position, ORCA_FEE_OWED_A);
        let earned_a = body.binary(OP_GT, owed_a, dust_floor);
        let owed_b = body.read(OP_READ_U64, position, ORCA_FEE_OWED_B);
        let earned_b = body.binary(OP_GT, owed_b, dust_floor);
        let worth_it = body.binary(OP_OR, earned_a, earned_b);
        body.invoke(collect, Some(worth_it));
    });
    b.build().unwrap()
}
// #endregion orca-harvest

// #region kamino-repay
/// Swap collateral into the borrowed asset on Jupiter, then repay exactly what the swap produced.
///
/// Send it after Kamino's `refresh_reserve`s and `refresh_obligation`, in the same transaction.
pub fn kamino_repay_swap_output() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let jupiter = program(&mut b, JUPITER_V6);
    let kamino = program(&mut b, KAMINO_LEND);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let instructions_sysvar = b.account(READ, Some(SYSVAR_INSTRUCTIONS.to_bytes()), None, 0);
    let borrower = b.account(SIGN, None, None, 0);
    let collateral_ata = b.account(WRITE, None, None, 0);
    let borrowed_asset_ata = token_account(&mut b);
    let obligation = b.account(WRITE, None, None, 0);
    let lending_market = b.account(READ, None, None, 0);
    let repay_reserve = b.account(WRITE, None, None, 0);
    let reserve_liquidity_mint = b.account(READ, None, None, 0);
    let reserve_liquidity_supply = b.account(WRITE, None, None, 0);
    b.account_groups(2); // routeAccounts, farmAccounts
    let route_args = b.input(VALUE_BYTES, ROUTE_ARGS_MAX);
    let minimum_repayment = b.input(VALUE_U64, 0);

    let route_args = b.load_input(route_args);
    let minimum_repayment = b.load_input(minimum_repayment);

    // Kamino repays from any account the borrower may spend, so the swap must pay the borrower.
    require_owner(&mut b, borrowed_asset_ata, borrower);

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

    // Kamino's v2 repayment: these 9 accounts, then farmAccounts.
    let repay = b.blob(&anchor("repay_obligation_liquidity_v2"));
    let repay = b.cpi_with_group(
        kamino,
        &[
            (borrower, SIGN),
            (obligation, WRITE),
            (lending_market, READ),
            (repay_reserve, WRITE),
            (reserve_liquidity_mint, READ),
            (reserve_liquidity_supply, WRITE),
            (borrowed_asset_ata, WRITE),
            (token_program, READ),
            (instructions_sysvar, READ),
        ],
        &[Segment::Literal(repay), Segment::Register(DATA_REG_U64, swapped)],
        1,
    );
    b.invoke(repay, None);
    b.build().unwrap()
}
// #endregion kamino-repay

// #region kamino-liquidate
/// Liquidate on Kamino and require the liquidator to net a minimum in the collateral's
/// underlying token.
///
/// Send it after Kamino's `refresh_reserve`s and `refresh_obligation`, in the same transaction.
pub fn kamino_liquidate_with_proof() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let kamino = program(&mut b, KAMINO_LEND);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let instructions_sysvar = b.account(READ, Some(SYSVAR_INSTRUCTIONS.to_bytes()), None, 0);
    let liquidator = b.account(SIGN, None, None, 0);
    let obligation = b.account(WRITE, None, None, 0);
    let lending_market = b.account(READ, None, None, 0);
    let lending_market_authority = b.account(READ, None, None, 0);
    let repay_reserve = b.account(WRITE, None, None, 0);
    let repay_reserve_liquidity_mint = b.account(READ, None, None, 0);
    let repay_reserve_liquidity_supply = b.account(WRITE, None, None, 0);
    let withdraw_reserve = b.account(WRITE, None, None, 0);
    let withdraw_reserve_liquidity_mint = b.account(READ, None, None, 0);
    let withdraw_reserve_collateral_mint = b.account(WRITE, None, None, 0);
    let withdraw_reserve_collateral_supply = b.account(WRITE, None, None, 0);
    let withdraw_reserve_liquidity_supply = b.account(WRITE, None, None, 0);
    let withdraw_reserve_fee_receiver = b.account(WRITE, None, None, 0);
    let user_source_liquidity = b.account(WRITE, None, None, 0);
    let user_destination_collateral = token_account(&mut b);
    let user_destination_liquidity = token_account(&mut b);
    b.account_groups(1); // farmAccounts
    let liquidity_amount = b.input(VALUE_U64, 0);
    let min_acceptable_received = b.input(VALUE_U64, 0);
    let minimum_bounty = b.input(VALUE_U64, 0);

    let liquidity_amount = b.load_input(liquidity_amount);
    let min_acceptable_received = b.load_input(min_acceptable_received);
    let minimum_bounty = b.load_input(minimum_bounty);
    let zero = b.const_u64(0);

    // Kamino checks the mints of the accounts it pays, not whose they are.
    require_owner(&mut b, user_destination_liquidity, liquidator);
    require_owner(&mut b, user_destination_collateral, liquidator);

    let payout_before = b.read(OP_READ_U64, user_destination_liquidity, TOKEN_AMOUNT);
    // Kamino's v2 liquidation: these 20 accounts, then farmAccounts.
    let liquidate = b.blob(&anchor("liquidate_obligation_and_redeem_reserve_collateral_v2"));
    let liquidate = b.cpi_with_group(
        kamino,
        &[
            (liquidator, SIGN),
            (obligation, WRITE),
            (lending_market, READ),
            (lending_market_authority, READ),
            (repay_reserve, WRITE),
            (repay_reserve_liquidity_mint, READ),
            (repay_reserve_liquidity_supply, WRITE),
            (withdraw_reserve, WRITE),
            (withdraw_reserve_liquidity_mint, READ),
            (withdraw_reserve_collateral_mint, WRITE),
            (withdraw_reserve_collateral_supply, WRITE),
            (withdraw_reserve_liquidity_supply, WRITE),
            (withdraw_reserve_fee_receiver, WRITE),
            (user_source_liquidity, WRITE),
            (user_destination_collateral, WRITE),
            (user_destination_liquidity, WRITE),
            (token_program, READ), // collateral_token_program
            (token_program, READ), // repay_liquidity_token_program
            (token_program, READ), // withdraw_liquidity_token_program
            (instructions_sysvar, READ),
        ],
        &[
            Segment::Literal(liquidate),
            Segment::Register(DATA_REG_U64, liquidity_amount),
            Segment::Register(DATA_REG_U64, min_acceptable_received),
            Segment::Register(DATA_REG_U64, zero), // no LTV override
        ],
        0,
    );
    b.invoke(liquidate, None);

    // The seized collateral, redeemed: what the liquidator actually received.
    let payout_after = b.read(OP_READ_U64, user_destination_liquidity, TOKEN_AMOUNT);
    let bounty = b.binary(OP_SUB, payout_after, payout_before);
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
    let bank_liquidity_vault_authority = b.account(READ, None, None, 0);
    let destination_ata = token_account(&mut b);
    let treasury_ata = token_account(&mut b);
    b.account_groups(1); // healthAccounts
    let minimum_withdrawn = b.input(VALUE_U64, 0);

    let minimum_withdrawn = b.load_input(minimum_withdrawn);
    let zero = b.const_u64(0);

    // marginfi pays whichever account it is given, and the sweep pays whichever treasury the
    // run names: both must be the authority's.
    require_owner(&mut b, destination_ata, authority);
    require_owner(&mut b, treasury_ata, authority);

    let balance_before = b.read(OP_READ_U64, destination_ata, TOKEN_AMOUNT);
    let withdraw = b.blob(&anchor("lending_account_withdraw"));
    let withdraw_all = b.blob(&[1, 1]); // Option::Some(true)
    let withdraw = b.cpi_with_group(
        marginfi,
        &[
            (marginfi_group, READ),
            (marginfi_account, WRITE),
            (authority, SIGN),
            (bank, WRITE),
            (destination_ata, WRITE),
            (bank_liquidity_vault_authority, READ),
            (bank_liquidity_vault, WRITE),
            (token_program, READ),
        ],
        &[
            Segment::Literal(withdraw),
            Segment::Register(DATA_REG_U64, zero), // ignored when withdraw_all is set
            Segment::Literal(withdraw_all),
        ],
        0,
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

// #region marginfi-to-kamino
/// Withdraw everything from marginfi and deposit exactly what came out into Kamino.
///
/// Send it after Kamino's `refresh_reserve`s and `refresh_obligation`, in the same transaction.
pub fn marginfi_to_kamino_rebalance() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let marginfi = program(&mut b, MARGINFI_V2);
    let kamino = program(&mut b, KAMINO_LEND);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let instructions_sysvar = b.account(READ, Some(SYSVAR_INSTRUCTIONS.to_bytes()), None, 0);
    let owner = b.account(SIGN | WRITE, None, None, 0);
    let wallet_ata = token_account(&mut b);
    let marginfi_group = b.account(READ, None, None, 0);
    let marginfi_account = b.account(WRITE, None, None, 0);
    let marginfi_bank = b.account(WRITE, None, None, 0);
    let marginfi_vault = b.account(WRITE, None, None, 0);
    let marginfi_vault_authority = b.account(READ, None, None, 0);
    let obligation = b.account(WRITE, None, None, 0);
    let lending_market = b.account(READ, None, None, 0);
    let lending_market_authority = b.account(READ, None, None, 0);
    let reserve = b.account(WRITE, None, None, 0);
    let reserve_liquidity_mint = b.account(READ, None, None, 0);
    let reserve_liquidity_supply = b.account(WRITE, None, None, 0);
    let reserve_collateral_mint = b.account(WRITE, None, None, 0);
    let reserve_destination_collateral = b.account(WRITE, None, None, 0);
    b.account_groups(2); // healthAccounts, farmAccounts
    let minimum_moved = b.input(VALUE_U64, 0);

    let minimum_moved = b.load_input(minimum_moved);
    let zero = b.const_u64(0);

    let wallet_before = b.read(OP_READ_U64, wallet_ata, TOKEN_AMOUNT);
    let withdraw = b.blob(&anchor("lending_account_withdraw"));
    let withdraw_all = b.blob(&[1, 1]); // Option::Some(true)
    let withdraw = b.cpi_with_group(
        marginfi,
        &[
            (marginfi_group, READ),
            (marginfi_account, WRITE),
            (owner, SIGN),
            (marginfi_bank, WRITE),
            (wallet_ata, WRITE),
            (marginfi_vault_authority, READ),
            (marginfi_vault, WRITE),
            (token_program, READ),
        ],
        &[
            Segment::Literal(withdraw),
            Segment::Register(DATA_REG_U64, zero), // ignored when withdraw_all is set
            Segment::Literal(withdraw_all),
        ],
        0,
    );
    b.invoke(withdraw, None);

    let wallet_after = b.read(OP_READ_U64, wallet_ata, TOKEN_AMOUNT);
    let moved = b.binary(OP_SUB, wallet_after, wallet_before);
    let worth_it = b.binary(OP_GTE, moved, minimum_moved);
    b.require(worth_it);

    // Kamino's v2 deposit: these 14 accounts, then farmAccounts.
    let deposit = b.blob(&anchor("deposit_reserve_liquidity_and_obligation_collateral_v2"));
    let deposit = b.cpi_with_group(
        kamino,
        &[
            (owner, SIGN | WRITE),
            (obligation, WRITE),
            (lending_market, READ),
            (lending_market_authority, READ),
            (reserve, WRITE),
            (reserve_liquidity_mint, READ),
            (reserve_liquidity_supply, WRITE),
            (reserve_collateral_mint, WRITE),
            (reserve_destination_collateral, WRITE),
            (wallet_ata, WRITE),
            (kamino, READ), // placeholder_user_destination_collateral: Kamino's ID means "none"
            (token_program, READ), // collateral_token_program
            (token_program, READ), // liquidity_token_program
            (instructions_sysvar, READ),
        ],
        &[Segment::Literal(deposit), Segment::Register(DATA_REG_U64, moved)],
        1,
    );
    b.invoke(deposit, None);
    b.build().unwrap()
}
// #endregion marginfi-to-kamino

// #region signed-quote
/// Settle a maker's Ed25519-signed quote: the taker pays the signed price for what it takes, and
/// the maker delivers it, within the quote's size, before its expiry, in its mints.
///
/// The Ed25519 precompile instruction carrying the maker's signature goes directly before the run.
pub fn signed_quote_settlement() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    // The compiler records a constant pubkey before the accounts' addresses.
    b.pubkey(ED25519_PROGRAM_ID.to_bytes());
    let instructions = b.account(READ, Some(SYSVAR_INSTRUCTIONS.to_bytes()), None, 0);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let taker = b.account(SIGN, None, None, 0);
    let maker = b.account(SIGN, None, None, 0);
    let taker_quote_account = token_account(&mut b); // pays, in the quote mint
    let maker_quote_account = token_account(&mut b); // is paid, in the quote mint
    let maker_base_account = token_account(&mut b); // delivers, in the base mint
    let taker_base_account = token_account(&mut b); // receives, in the base mint
    let amount = b.input(VALUE_U64, 0);

    let amount = b.load_input(amount);
    let one = b.const_u64(1);
    let ed25519 = b.const_pubkey(ED25519_PROGRAM_ID.to_bytes());
    let zero = b.const_u64(0);
    let (mask, expected) = ed25519_header(QUOTE_LEN);
    let mask = b.const_u128(mask);
    let expected = b.const_u128(expected);
    let public_key_offset = b.const_u64(ED25519_PUBLIC_KEY_OFFSET);
    let message_offset = b.const_u64(ED25519_MESSAGE_DATA_OFFSET);
    let tag = b.const_u64(u64::from_le_bytes(QUOTE_TAG));
    let expiry_at = b.const_u64(QUOTE_EXPIRY);
    let taker_at = b.const_u64(QUOTE_TAKER);
    let max_amount_at = b.const_u64(QUOTE_MAX_AMOUNT);
    let quote_mint_at = b.const_u64(QUOTE_QUOTE_MINT);
    let base_mint_at = b.const_u64(QUOTE_BASE_MINT);
    let price_at = b.const_u64(QUOTE_PRICE);
    let price_scale = b.const_u64(PRICE_SCALE);

    // The signature is in the instruction directly before this run.
    let current = b.introspect(OP_INSTRUCTION_INDEX, instructions, NO_INDEX, NO_INDEX);
    let signature = b.binary(OP_SUB, current, one);
    let verifier = b.introspect(OP_INSTRUCTION_PROGRAM, instructions, signature, NO_INDEX);
    let is_ed25519 = b.binary(OP_EQ, verifier, ed25519);
    b.require(is_ed25519);
    // One signature, over a 128-byte message, with its key and message in its own data.
    let header = b.read_instruction_data(OP_READ_U128, instructions, signature, zero);
    let header = b.binary(OP_BIT_AND, header, mask);
    let self_contained = b.binary(OP_EQ, header, expected);
    b.require(self_contained);
    // Signed by the maker, who also signs the transaction.
    let key_at = b.read_instruction_data(OP_READ_U16, instructions, signature, public_key_offset);
    let key = b.read_instruction_data(OP_READ_PUBKEY, instructions, signature, key_at);
    let maker_key = b.account_key(maker);
    let by_maker = b.binary(OP_EQ, key, maker_key);
    b.require(by_maker);
    // Where the signed message starts in that instruction's data.
    let message = b.read_instruction_data(OP_READ_U16, instructions, signature, message_offset);

    // The tag separates quotes from anything else the maker signs.
    let signed_tag = b.read_instruction_data(OP_READ_U64, instructions, signature, message);
    let tagged = b.binary(OP_EQ, signed_tag, tag);
    b.require(tagged);

    let now = b.clock_timestamp();
    let at = b.binary(OP_ADD, message, expiry_at);
    let expiry = b.read_instruction_data(OP_READ_I64, instructions, signature, at);
    let not_expired = b.binary(OP_LTE, now, expiry);
    b.require(not_expired);

    let at = b.binary(OP_ADD, message, taker_at);
    let quoted_taker = b.read_instruction_data(OP_READ_PUBKEY, instructions, signature, at);
    let taker_key = b.account_key(taker);
    let for_this_taker = b.binary(OP_EQ, quoted_taker, taker_key);
    b.require(for_this_taker);

    let at = b.binary(OP_ADD, message, max_amount_at);
    let max_amount = b.read_instruction_data(OP_READ_U64, instructions, signature, at);
    let within_size = b.binary(OP_LTE, amount, max_amount);
    b.require(within_size);

    // A token transfer moves only between accounts of one mint, so pinning one side of each leg
    // pins both.
    let paid_in = b.read(OP_READ_PUBKEY, taker_quote_account, TOKEN_MINT);
    let at = b.binary(OP_ADD, message, quote_mint_at);
    let quote_mint = b.read_instruction_data(OP_READ_PUBKEY, instructions, signature, at);
    let in_quote_mint = b.binary(OP_EQ, paid_in, quote_mint);
    b.require(in_quote_mint);
    let delivered = b.read(OP_READ_PUBKEY, maker_base_account, TOKEN_MINT);
    let at = b.binary(OP_ADD, message, base_mint_at);
    let base_mint = b.read_instruction_data(OP_READ_PUBKEY, instructions, signature, at);
    let in_base_mint = b.binary(OP_EQ, delivered, base_mint);
    b.require(in_base_mint);
    // The payment reaches an account the maker owns, not one the taker picked.
    require_owner(&mut b, maker_quote_account, maker);

    // payment = amount × price ÷ PRICE_SCALE, rounded up in the maker's favour.
    let at = b.binary(OP_ADD, message, price_at);
    let price = b.read_instruction_data(OP_READ_U64, instructions, signature, at);
    let payment = b.mul_div_ceil(amount, price, price_scale);

    let transfer = b.blob(&[3]); // SPL Token Transfer
    let taker_pays = b.cpi(
        token_program,
        &[(taker_quote_account, WRITE), (maker_quote_account, WRITE), (taker, SIGN)],
        &[Segment::Literal(transfer), Segment::Register(DATA_REG_U64, payment)],
    );
    b.invoke(taker_pays, None);
    let transfer = b.blob(&[3]);
    let maker_delivers = b.cpi(
        token_program,
        &[(maker_base_account, WRITE), (taker_base_account, WRITE), (maker, SIGN)],
        &[Segment::Literal(transfer), Segment::Register(DATA_REG_U64, amount)],
    );
    b.invoke(maker_delivers, None);
    b.build().unwrap()
}
// #endregion signed-quote

// #region jupiter-daily-cap
/// A per-caller daily cap on a Jupiter swap: the route's `inAmount` is charged against 1.728 SOL
/// that refills at 20,000 lamports a second, in a registry entry keyed by the actor. The route
/// must sell the actor's own wrapped SOL, exactly `inAmount` of it.
pub fn jupiter_daily_cap_swap() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    // The compiler records a constant pubkey before the accounts' addresses.
    b.pubkey(WRAPPED_SOL_MINT.to_bytes());
    let action_program = program(&mut b, JUPITER_V6);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let actor = b.account(SIGN | WRITE, None, None, 0);
    let source_ata = token_account(&mut b);
    let spend = b.account(WRITE, None, None, 0);
    let system_program = program(&mut b, SYSTEM_PROGRAM_ID);
    b.account_groups(1); // actionAccounts
    let route_plan = b.input(VALUE_BYTES, ROUTE_ARGS_MAX);
    let in_amount = b.input(VALUE_U64, 0);
    let quoted_out_amount = b.input(VALUE_U64, 0);
    let slippage_bps = b.input(VALUE_U64, 0);
    let platform_fee_bps = b.input(VALUE_U64, 0);

    // The TypeScript compiler loads the inputs, then the constants in the order the steps use
    // them, then opens the entry, all before the first step.
    let route_plan = b.load_input(route_plan);
    let in_amount = b.load_input(in_amount);
    let quoted_out_amount = b.load_input(quoted_out_amount);
    let slippage_bps = b.load_input(slippage_bps);
    let platform_fee_bps = b.load_input(platform_fee_bps);
    let wrapped_sol = b.const_pubkey(WRAPPED_SOL_MINT.to_bytes());
    let refill_per_second = b.const_u64(20_000);
    let cap = b.const_u64(1_728_000_000);
    let key = b.account_key(actor);
    b.open_registry(spend, Some(key), actor, 0, 16, system_program);

    // The cap counts lamports: a route that sold another mint would be charged in its units.
    let held = b.read(OP_READ_PUBKEY, source_ata, TOKEN_MINT);
    let holds_wsol = b.binary(OP_EQ, held, wrapped_sol);
    b.require(holds_wsol);
    require_owner(&mut b, source_ata, actor);

    // rateLimit: `now` never reads earlier than `lastSpend`, so it, and the `lastSpend` written
    // back (being `now`), never move backward: a clock step-back refills nothing and never
    // double-refills once the clock recovers. The refill itself is computed in u128, then charged
    // against the cap.
    let last = b.read_registry(spend, 8, OP_READ_I64);
    let now = b.clock_timestamp();
    let now = b.binary(OP_MAX, now, last);
    let spent = b.read_registry(spend, 0, OP_READ_U64);
    let spent = b.cast(OP_CAST_U128, spent);
    let elapsed = b.binary(OP_SUB, now, last);
    let elapsed = b.cast(OP_CAST_U128, elapsed);
    let rate = b.cast(OP_CAST_U128, refill_per_second);
    let refill = b.binary(OP_MUL, elapsed, rate);
    let refilled = b.binary(OP_MIN, spent, refill);
    let kept = b.binary(OP_SUB, spent, refilled);
    let amount = b.cast(OP_CAST_U128, in_amount);
    let total = b.binary(OP_ADD, kept, amount);
    let cap = b.cast(OP_CAST_U128, cap);
    let within = b.binary(OP_LTE, total, cap);
    b.require(within);
    let total = b.cast(OP_CAST_U64, total);
    b.write_registry(spend, 0, OP_READ_U64, total);
    b.write_registry(spend, 8, OP_READ_I64, now);

    let source_before = b.read(OP_READ_U64, source_ata, TOKEN_AMOUNT);
    let route = b.blob(&anchor("route"));
    let swap = b.cpi_with_group(
        action_program,
        &[(token_program, READ), (actor, SIGN), (source_ata, WRITE)],
        &[
            Segment::Literal(route),
            Segment::Register(DATA_REG_BYTES, route_plan),
            Segment::Register(DATA_REG_U64, in_amount),
            Segment::Register(DATA_REG_U64, quoted_out_amount),
            Segment::Register(DATA_REG_U16, slippage_bps),
            Segment::Register(DATA_REG_U8, platform_fee_bps),
        ],
        0,
    );
    b.set_cpi_max_data_len(swap, 8 + ROUTE_ARGS_MAX + ROUTE_TAIL_LEN);
    b.invoke(swap, None);

    // Jupiter moves the accounts its steps name, not the source it was handed. It required the
    // source to hold `inAmount`, so the subtraction cannot underflow.
    let expected = b.binary(OP_SUB, source_before, in_amount);
    let source_after = b.read(OP_READ_U64, source_ata, TOKEN_AMOUNT);
    let sold_the_charge = b.binary(OP_EQ, expected, source_after);
    b.require(sold_the_charge);
    b.build().unwrap()
}
// #endregion jupiter-daily-cap

/// Every template, under the name `fixtures/protocol-examples.json` records it by.
pub const TEMPLATES: [(&str, fn() -> Vec<u8>); 13] = [
    ("jitoProfitGuardedTip", jito_profit_guarded_tip),
    ("jupiterDailyCapSwap", jupiter_daily_cap_swap),
    ("jupiterDepositExactOutput", jupiter_deposit_exact_output),
    ("jupiterOracleCheckedSwap", jupiter_oracle_checked_swap),
    ("kaminoLiquidateWithProof", kamino_liquidate_with_proof),
    ("kaminoRepaySwapOutput", kamino_repay_swap_output),
    ("marginfiToKaminoRebalance", marginfi_to_kamino_rebalance),
    ("marginfiWithdrawAllWithFloor", marginfi_withdraw_all_with_floor),
    ("orcaCompoundFees", orca_compound_fees),
    ("orcaHarvestManyPositions", orca_harvest_many_positions),
    ("pythFreshPriceGate", pyth_fresh_price_gate),
    ("signedQuoteSettlement", signed_quote_settlement),
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
