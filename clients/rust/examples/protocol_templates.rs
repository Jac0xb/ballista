//! The live-protocol templates, written with the declarative API in `ballista_sdk::template`: one
//! function per template, each the Rust twin of the TypeScript file in
//! `clients/js/examples/protocols/`.
//!
//! `tests/protocol_templates.rs` compiles every one and checks the bytes against
//! `fixtures/protocol-examples.json`. The runs are in `protocol_templates_run.rs`.
//!
//! ```bash
//! cargo run -p ballista-sdk --example protocol_templates
//! ```
//!
//! Every program address and every account offset here was read from the protocol's own source.
//! A protocol upgrade can move a field, and a moved field is a silently wrong read, so re-derive
//! these from the current IDL before you upload a template.

#![allow(dead_code)]

use ballista_sdk::anchor_discriminator;
use ballista_sdk::template::prelude::*;

fn main() {
    for (name, build) in ALL {
        let compiled = build()
            .compile()
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        println!(
            "{name:32} {:4} bytes  {:3} instructions  {:2} registers",
            compiled.stats.payload_bytes, compiled.stats.instructions, compiled.stats.registers
        );
    }
}

/// A template's name, as `fixtures/protocol-examples.json` spells it, and its definition.
pub type Example = (&'static str, fn() -> Template);

pub const ALL: &[Example] = &[
    ("jupiterDailyCapSwap", jupiter_daily_cap_swap),
    ("jupiterDepositExactOutput", jupiter_deposit_exact_output),
    ("jupiterOracleCheckedSwap", jupiter_oracle_checked_swap),
    ("kaminoLiquidateWithProof", kamino_liquidate_with_proof),
    ("kaminoRepaySwapOutput", kamino_repay_swap_output),
    ("orcaCompoundFees", orca_compound_fees),
    ("orcaHarvestManyPositions", orca_harvest_many_positions),
    ("pythFreshPriceGate", pyth_fresh_price_gate),
    ("signedQuoteSettlement", signed_quote_settlement),
    ("tokenSweepIntoSwap", token_sweep_into_swap),
];

// #region helpers
// ======================================================================== shared constants

// ------------------------------------------------------------------------------- programs

/// Jupiter aggregator v6.
const JUPITER_V6: Pubkey = pubkey!("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");
/// Kamino Lend, the primary market program.
const KAMINO_LEND: Pubkey = pubkey!("KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD");
/// Orca Whirlpools.
const ORCA_WHIRLPOOL: Pubkey = pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");
/// Pyth Solana receiver, the non-`pro-compatible` build.
const PYTH_RECEIVER: Pubkey = pubkey!("rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ");
/// SPL Memo. Orca's v2 instructions take it.
const MEMO_PROGRAM: Pubkey = pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
/// The wrapped SOL mint. Its token accounts count their balance in lamports.
const WRAPPED_SOL_MINT: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
/// Circle's USDC mint.
const USDC_MINT: Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

// -------------------------------------------------------------------------------- layouts

/// SPL Token account: the mint, then the owner, then the `u64` amount, in 165 bytes.
const TOKEN_ACCOUNT_MINT_OFFSET: u32 = 0;
const TOKEN_ACCOUNT_OWNER_OFFSET: u32 = 32;
const TOKEN_ACCOUNT_AMOUNT_OFFSET: u32 = 64;
const TOKEN_ACCOUNT_LENGTH: u32 = 165;

/// SPL Token `Mint`: `decimals` is the `u8` at offset 44 of the 82-byte layout.
const SPL_MINT_LENGTH: u32 = 82;
const SPL_MINT_DECIMALS: u32 = 44;

/// Pyth `PriceUpdateV2`, at the offsets of a `Full`-verification account. Read
/// `verificationLevel` first: a `Partial` account shifts every later field by one byte.
const PYTH_LENGTH: u32 = 134;
const PYTH_VERIFICATION_LEVEL: u32 = 40;
const PYTH_VERIFICATION_LEVEL_FULL: u64 = 1;
const PYTH_FEED_ID: u32 = 41;
const PYTH_PRICE: u32 = 73;
const PYTH_CONFIDENCE: u32 = 81;
const PYTH_EXPONENT: u32 = 89;
const PYTH_PUBLISH_TIME: u32 = 93;

/// Orca `Position`: liquidity, then the fees owed in each token.
const ORCA_POSITION_LENGTH: u32 = 216;
const ORCA_POSITION_LIQUIDITY: u32 = 72;
const ORCA_POSITION_FEE_OWED_A: u32 = 112;
const ORCA_POSITION_FEE_OWED_B: u32 = 136;

// --------------------------------------------------------------------------- instructions

/// Jupiter v6 `route(route_plan, in_amount, quoted_out_amount, slippage_bps, platform_fee_bps)`.
fn jupiter_route() -> [u8; 8] {
    anchor_discriminator("route")
}

/// Kamino `deposit_reserve_liquidity_and_obligation_collateral_v2(liquidity_amount: u64)`.
fn kamino_deposit() -> [u8; 8] {
    anchor_discriminator("deposit_reserve_liquidity_and_obligation_collateral_v2")
}

/// Kamino `repay_obligation_liquidity_v2(liquidity_amount: u64)`.
fn kamino_repay() -> [u8; 8] {
    anchor_discriminator("repay_obligation_liquidity_v2")
}

/// Kamino `liquidate_obligation_and_redeem_reserve_collateral_v2(liquidity_amount,
/// min_acceptable_received_liquidity_amount, max_allowed_ltv_override_percent)`.
fn kamino_liquidate() -> [u8; 8] {
    anchor_discriminator("liquidate_obligation_and_redeem_reserve_collateral_v2")
}

/// Orca `collect_fees()`.
fn orca_collect_fees() -> [u8; 8] {
    anchor_discriminator("collect_fees")
}

/// Orca `update_fees_and_rewards()`: folds the pool's fee growth into a position's fees owed.
fn orca_update_fees_and_rewards() -> [u8; 8] {
    anchor_discriminator("update_fees_and_rewards")
}

/// Orca `increase_liquidity_by_token_amounts_v2(method, remaining_accounts_info)`.
fn orca_increase_liquidity_by_token_amounts_v2() -> [u8; 8] {
    anchor_discriminator("increase_liquidity_by_token_amounts_v2")
}

/// `IncreaseLiquidityMethod::ByTokenAmounts`, the enum's only variant, as its Borsh tag.
const ORCA_BY_TOKEN_AMOUNTS: [u8; 1] = [0];

/// Borsh `Option::None`.
const OPTION_NONE: [u8; 1] = [0];

/// The route's platform fee account and rate are chosen by whoever builds the run: cap the rate.
const MAX_PLATFORM_FEE_BPS: u64 = 0;

/// A token account of the SPL Token program, read as data.
fn token_account() -> Account {
    account::writable()
        .owner(TOKEN_PROGRAM_ID)
        .min_data_length(TOKEN_ACCOUNT_LENGTH)
}

/// The `u64` balance of the token account `name`.
fn balance_of(name: &str) -> Expr {
    account_data(name, TOKEN_ACCOUNT_AMOUNT_OFFSET, ReadType::U64)
}

/// The fee account sits in the route's own accounts: any nonzero rate pays whoever chose it.
fn platform_fee_within_cap() -> Step {
    step::require(input("platformFeeBps").lte(u64(MAX_PLATFORM_FEE_BPS)))
        .label("platformFeeWithinCap")
}

/// Jupiter `route` data: the discriminator, the plan as the Swap API encoded it, then the tail.
fn jupiter_route_data(in_amount: Expr, quoted_out_amount: Expr) -> [DataPart; 6] {
    [
        data::literal(jupiter_route()),
        data::bytes(input("routePlan")),
        data::u64(in_amount),
        data::u64(quoted_out_amount),
        data::u16(input("slippageBps")),
        data::u8(input("platformFeeBps")),
    ]
}

// #endregion helpers

// ================================================================================== Jupiter

// #region jupiter-daily-cap
/// 1.728 SOL, in lamports: the most a caller can sell at once.
const DAILY_CAP: u64 = 1_728_000_000;
/// The cap over 86,400 seconds, so a caller can sell about twice the cap in any 24 hours.
const REFILL_PER_SECOND: u64 = 20_000;

/// Cap each caller's Jupiter sales of wrapped SOL.
pub fn jupiter_daily_cap_swap() -> Template {
    let source_balance = balance_of("sourceAta");
    Template::new()
        .input("routePlan", Type::Bytes(512))
        // What the route sells, what the cap is charged, and what must leave `sourceAta`.
        .input("inAmount", Type::U64)
        .input("quotedOutAmount", Type::U64)
        .input("slippageBps", Type::U64)
        .input("platformFeeBps", Type::U64)
        .registry(
            "dailySpend",
            [("spent", Type::U64), ("lastSpend", Type::I64)],
        )
        .account("actionProgram", account::program(JUPITER_V6))
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account("actor", account::signer().writable())
        // The actor's wrapped-SOL token account, which the route sells from.
        .account("sourceAta", token_account())
        .account(
            "spend",
            account::registry("dailySpend", "actor").key(account_key("actor")),
        )
        .account("systemProgram", account::system_program())
        .account_group("actionAccounts")
        // The cap counts lamports: a route that sells another mint would be charged in its units.
        .step(
            step::require(
                account_data("sourceAta", TOKEN_ACCOUNT_MINT_OFFSET, ReadType::Pubkey)
                    .eq(pubkey(WRAPPED_SOL_MINT)),
            )
            .label("spendsWrappedSol"),
        )
        .step(
            step::require(
                account_data("sourceAta", TOKEN_ACCOUNT_OWNER_OFFSET, ReadType::Pubkey)
                    .eq(account_key("actor")),
            )
            .label("sourceBelongsToTheCaller"),
        )
        .steps(rate_limit(
            "spend",
            u64(DAILY_CAP),
            u64(REFILL_PER_SECOND),
            input("inAmount"),
        ))
        .step(step::snapshot("sourceBefore", &source_balance).label("readSourceBeforeSwap"))
        .step(platform_fee_within_cap())
        .step(
            step::invoke("actionProgram")
                .readonly("tokenProgram")
                .signer("actor")
                .writable("sourceAta")
                .account_group("actionAccounts")
                .data_parts(jupiter_route_data(
                    input("inAmount"),
                    input("quotedOutAmount"),
                ))
                .label("swapWithinTheCap"),
        )
        // Jupiter required the source to hold `inAmount`, so the subtraction cannot underflow.
        .step(
            step::require((snapshot("sourceBefore") - input("inAmount")).eq(source_balance))
                .label("soldWhatTheCapCharged"),
        )
}
// #endregion jupiter-daily-cap

// #region jupiter-deposit
/// Deposit into Kamino exactly what a Jupiter swap produced.
pub fn jupiter_deposit_exact_output() -> Template {
    Template::new()
        .input("routePlan", Type::Bytes(512))
        .input("inAmount", Type::U64)
        .input("quotedOutAmount", Type::U64)
        .input("slippageBps", Type::U64)
        .input("platformFeeBps", Type::U64)
        // Below this the route is not worth depositing and the run fails instead.
        .input("minimumOut", Type::U64)
        .account("jupiter", account::program(JUPITER_V6))
        .account("kamino", account::program(KAMINO_LEND))
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account(
            "instructionsSysvar",
            account::readonly().address(INSTRUCTIONS_SYSVAR_ID),
        )
        .account("owner", account::signer().writable())
        // What the route sells from.
        .account("sourceAta", account::writable())
        // The route's destination, and the account the deposit draws from.
        .account("destinationAta", token_account())
        .account("obligation", account::writable())
        .account("lendingMarket", account::readonly())
        .account("lendingMarketAuthority", account::readonly())
        .account("reserve", account::writable())
        .account("reserveLiquidityMint", account::readonly())
        .account("reserveLiquiditySupply", account::writable())
        .account("reserveCollateralMint", account::writable())
        .account("reserveDestinationDepositCollateral", account::writable())
        // Jupiter's own accounts, and Kamino's v2 farm tail.
        .account_group("routeAccounts")
        .account_group("farmAccounts")
        .step(
            step::snapshot("balanceBefore", balance_of("destinationAta"))
                .label("readBalanceBeforeSwap"),
        )
        .step(platform_fee_within_cap())
        .step(
            step::invoke("jupiter")
                .readonly("tokenProgram")
                .signer("owner")
                .writable("sourceAta")
                .writable("destinationAta")
                .account_group("routeAccounts")
                .data_parts(jupiter_route_data(
                    input("inAmount"),
                    input("quotedOutAmount"),
                ))
                .label("swap"),
        )
        .step(
            step::let_(
                "received",
                balance_of("destinationAta") - snapshot("balanceBefore"),
            )
            .label("measureSwapOutput"),
        )
        .step(step::require(var("received").gte(input("minimumOut"))).label("swapMetItsFloor"))
        // Kamino's v2 deposit: v1 refuses calls from other programs (`CpiDisabled`).
        .step(
            step::invoke("kamino")
                .writable_signer("owner")
                .writable("obligation")
                .readonly("lendingMarket")
                .readonly("lendingMarketAuthority")
                .writable("reserve")
                .readonly("reserveLiquidityMint")
                .writable("reserveLiquiditySupply")
                .writable("reserveCollateralMint")
                .writable("reserveDestinationDepositCollateral")
                // The deposit draws from the account the swap paid into.
                .writable("destinationAta")
                // `placeholder_user_destination_collateral`: the Kamino program means "none".
                .readonly("kamino")
                // `collateral_token_program`, then `liquidity_token_program`.
                .readonly("tokenProgram")
                .readonly("tokenProgram")
                .readonly("instructionsSysvar")
                .account_group("farmAccounts")
                .data(data::literal(kamino_deposit()))
                // Exactly what the swap produced, measured a moment ago.
                .data(data::u64(var("received")))
                .label("depositSwapOutput"),
        )
}
// #endregion jupiter-deposit

// #region jupiter-oracle-swap
/// The Pyth feed id for SOL/USD.
const FEED_ID: [u8; 32] = [
    0xef, 0x0d, 0x8b, 0x6f, 0xda, 0x2c, 0xeb, 0xa4, 0x1d, 0xa1, 0x5d, 0x40, 0x95, 0xd1, 0xda, 0x39,
    0x2a, 0x0d, 0x2f, 0x8e, 0xd0, 0xc6, 0xc7, 0xbc, 0x0f, 0x4c, 0xfa, 0xc8, 0xc2, 0x80, 0xb5, 0x6d,
];
/// How far below the oracle's price the fill may land, in basis points.
const TOLERANCE_BPS: u128 = 100;

/// Swap through Jupiter only at a fill the Pyth price backs.
pub fn jupiter_oracle_checked_swap() -> Template {
    let pinned_mint = |address: Pubkey| {
        account::readonly()
            .address(address)
            .owner(TOKEN_PROGRAM_ID)
            .min_data_length(SPL_MINT_LENGTH)
    };
    Template::new()
        .input("routePlan", Type::Bytes(512))
        .input("inAmount", Type::U64)
        .input("quotedOutAmount", Type::U64)
        .input("slippageBps", Type::U64)
        .input("platformFeeBps", Type::U64)
        .account("jupiter", account::program(JUPITER_V6))
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account(
            "priceUpdate",
            account::readonly()
                .owner(PYTH_RECEIVER)
                .min_data_length(PYTH_LENGTH),
        )
        .account("trader", account::signer().writable())
        .account("sourceAta", token_account())
        .account("destinationAta", token_account())
        // The pair `FEED_ID` prices: what the route sells, and what it buys.
        .account("sourceMint", pinned_mint(WRAPPED_SOL_MINT))
        .account("destinationMint", pinned_mint(USDC_MINT))
        .account_group("routeAccounts")
        // Pin the verification level first: it decides where every other field sits.
        .step(
            step::require(
                account_data("priceUpdate", PYTH_VERIFICATION_LEVEL, ReadType::U8)
                    .eq(u64(PYTH_VERIFICATION_LEVEL_FULL)),
            )
            .label("priceIsFullyVerified"),
        )
        // The owner pin takes any feed's price; the feed id is what says which one this is.
        .step(
            step::require(
                account_data("priceUpdate", PYTH_FEED_ID, ReadType::Pubkey).eq(pubkey(FEED_ID)),
            )
            .label("priceIsTheExpectedFeed"),
        )
        .step(
            step::require(
                (clock_unix_timestamp()
                    - account_data("priceUpdate", PYTH_PUBLISH_TIME, ReadType::I64))
                .lte(i64(60)),
            )
            .label("oracleIsFresh"),
        )
        // Each token account must hold the mint whose decimals scale it.
        .step(
            step::require(
                account_data("sourceAta", TOKEN_ACCOUNT_MINT_OFFSET, ReadType::Pubkey)
                    .eq(key("sourceMint")),
            )
            .label("sourceHoldsTheSourceMint"),
        )
        .step(
            step::require(
                account_data(
                    "destinationAta",
                    TOKEN_ACCOUNT_MINT_OFFSET,
                    ReadType::Pubkey,
                )
                .eq(key("destinationMint")),
            )
            .label("destinationHoldsTheDestinationMint"),
        )
        // Both ends of the swap are the trader's. The key is read once, into a register both
        // checks share: without register reuse, the template uses 62 of the runtime's 64.
        .step(step::let_("traderKey", account_key("trader")))
        .step(
            step::require(
                account_data("sourceAta", TOKEN_ACCOUNT_OWNER_OFFSET, ReadType::Pubkey)
                    .eq(var("traderKey")),
            )
            .label("sellsTheTradersOwnTokens"),
        )
        .step(
            step::require(
                account_data(
                    "destinationAta",
                    TOKEN_ACCOUNT_OWNER_OFFSET,
                    ReadType::Pubkey,
                )
                .eq(var("traderKey")),
            )
            .label("proceedsGoToTheTrader"),
        )
        // In base units the fill is worth sold × price × 10^(destinationDecimals + exponent −
        // sourceDecimals).
        .step(
            step::let_(
                "scale",
                account_data("destinationMint", SPL_MINT_DECIMALS, ReadType::U8).cast(Type::I64)
                    + account_data("priceUpdate", PYTH_EXPONENT, ReadType::I32)
                    - account_data("sourceMint", SPL_MINT_DECIMALS, ReadType::U8).cast(Type::I64),
            )
            .label("computeDecimalScale"),
        )
        // Pyth prices are signed; a negative or zero price means the feed is unusable here.
        .step(
            step::let_(
                "oraclePrice",
                account_data("priceUpdate", PYTH_PRICE, ReadType::I64),
            )
            .label("readOraclePrice"),
        )
        .step(step::require(var("oraclePrice").gt(i64(0))).label("oraclePriceIsPositive"))
        .step(step::snapshot("sourceBefore", balance_of("sourceAta")).label("readSourceBeforeSwap"))
        .step(
            step::snapshot("balanceBefore", balance_of("destinationAta"))
                .label("readBalanceBeforeSwap"),
        )
        .step(platform_fee_within_cap())
        .step(
            step::invoke("jupiter")
                .readonly("tokenProgram")
                .signer("trader")
                .writable("sourceAta")
                .writable("destinationAta")
                .account_group("routeAccounts")
                .data_parts(jupiter_route_data(
                    input("inAmount"),
                    input("quotedOutAmount"),
                ))
                .label("swap"),
        )
        // What actually left, whatever the route data claimed it would sell.
        .step(
            step::let_("sold", snapshot("sourceBefore") - balance_of("sourceAta"))
                .label("measureAmountSold"),
        )
        .step(step::require(var("sold").eq(input("inAmount"))).label("soldTheRouteInput"))
        // sold × price, scaled by 10^scale, less `TOLERANCE_BPS`. Taking the max with zero makes
        // one of the two powers of ten 1, so no select is needed.
        .step(
            step::let_(
                "fairOut",
                multiply_divide(
                    multiply_divide(
                        var("sold").cast(Type::U128) * var("oraclePrice").cast(Type::U128),
                        power_of_ten(var("scale").max(i64(0)).cast(Type::U64)),
                        power_of_ten((i64(0) - var("scale")).max(i64(0)).cast(Type::U64)),
                    ),
                    u128(10_000 - TOLERANCE_BPS),
                    u128(10_000),
                )
                .cast(Type::U64),
            )
            .label("computeOracleFloor"),
        )
        .step(
            step::require(
                (balance_of("destinationAta") - snapshot("balanceBefore")).gte(var("fairOut")),
            )
            .label("fillBeatTheOracle"),
        )
}
// #endregion jupiter-oracle-swap

// =================================================================================== Kamino

// #region kamino-liquidate
/// Liquidate a Kamino obligation only if the liquidator walks away with the bounty.
pub fn kamino_liquidate_with_proof() -> Template {
    Template::new()
        .input("liquidityAmount", Type::U64)
        .input("minAcceptableReceived", Type::U64)
        .input("minimumBounty", Type::U64)
        .account("kamino", account::program(KAMINO_LEND))
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account(
            "instructionsSysvar",
            account::readonly().address(INSTRUCTIONS_SYSVAR_ID),
        )
        .account("liquidator", account::signer())
        .account("obligation", account::writable())
        .account("lendingMarket", account::readonly())
        .account("lendingMarketAuthority", account::readonly())
        .account("repayReserve", account::writable())
        .account("repayReserveLiquidityMint", account::readonly())
        .account("repayReserveLiquiditySupply", account::writable())
        .account("withdrawReserve", account::writable())
        .account("withdrawReserveLiquidityMint", account::readonly())
        .account("withdrawReserveCollateralMint", account::writable())
        .account("withdrawReserveCollateralSupply", account::writable())
        .account("withdrawReserveLiquiditySupply", account::writable())
        .account("withdrawReserveFeeReceiver", account::writable())
        .account("userSourceLiquidity", account::writable())
        .account("userDestinationCollateral", token_account())
        .account("userDestinationLiquidity", token_account())
        .account_group("farmAccounts")
        // Kamino checks the mints of the accounts it pays, not whose they are.
        .step(
            step::require(
                account_data(
                    "userDestinationLiquidity",
                    TOKEN_ACCOUNT_OWNER_OFFSET,
                    ReadType::Pubkey,
                )
                .eq(key("liquidator")),
            )
            .label("bountyGoesToTheLiquidator"),
        )
        .step(
            step::require(
                account_data(
                    "userDestinationCollateral",
                    TOKEN_ACCOUNT_OWNER_OFFSET,
                    ReadType::Pubkey,
                )
                .eq(key("liquidator")),
            )
            .label("seizedCollateralGoesToTheLiquidator"),
        )
        // Kamino redeems the seized cTokens in the same instruction and pays the underlying here.
        .step(
            step::snapshot("payoutBefore", balance_of("userDestinationLiquidity"))
                .label("readPayoutBefore"),
        )
        // v2: the v1 handler refuses every caller but Kamino itself and a short whitelist.
        .step(
            step::invoke("kamino")
                .signer("liquidator")
                .writable("obligation")
                .readonly("lendingMarket")
                .readonly("lendingMarketAuthority")
                .writable("repayReserve")
                .readonly("repayReserveLiquidityMint")
                .writable("repayReserveLiquiditySupply")
                .writable("withdrawReserve")
                .readonly("withdrawReserveLiquidityMint")
                .writable("withdrawReserveCollateralMint")
                .writable("withdrawReserveCollateralSupply")
                .writable("withdrawReserveLiquiditySupply")
                .writable("withdrawReserveFeeReceiver")
                .writable("userSourceLiquidity")
                .writable("userDestinationCollateral")
                .writable("userDestinationLiquidity")
                // The collateral, repay and withdraw token programs.
                .readonly("tokenProgram")
                .readonly("tokenProgram")
                .readonly("tokenProgram")
                .readonly("instructionsSysvar")
                .account_group("farmAccounts")
                .data(data::literal(kamino_liquidate()))
                .data(data::u64(input("liquidityAmount")))
                .data(data::u64(input("minAcceptableReceived")))
                // No LTV override: liquidate on the protocol's own terms.
                .data(data::u64(u64(0)))
                .label("liquidate"),
        )
        .step(
            step::require(
                (balance_of("userDestinationLiquidity") - snapshot("payoutBefore"))
                    .gte(input("minimumBounty")),
            )
            .label("liquidationPaidTheBounty"),
        )
}
// #endregion kamino-liquidate

// #region kamino-repay
/// Swap collateral into the borrowed asset and repay exactly what the swap produced.
pub fn kamino_repay_swap_output() -> Template {
    Template::new()
        .input("routePlan", Type::Bytes(512))
        .input("inAmount", Type::U64)
        .input("quotedOutAmount", Type::U64)
        .input("slippageBps", Type::U64)
        .input("platformFeeBps", Type::U64)
        .input("minimumRepayment", Type::U64)
        .account("jupiter", account::program(JUPITER_V6))
        .account("kamino", account::program(KAMINO_LEND))
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account(
            "instructionsSysvar",
            account::readonly().address(INSTRUCTIONS_SYSVAR_ID),
        )
        .account("borrower", account::signer())
        .account("collateralAta", account::writable())
        .account("borrowedAssetAta", token_account())
        .account("obligation", account::writable())
        .account("lendingMarket", account::readonly())
        .account("repayReserve", account::writable())
        .account("reserveLiquidityMint", account::readonly())
        .account("reserveLiquiditySupply", account::writable())
        .account_group("routeAccounts")
        .account_group("farmAccounts")
        // The swap pays into `borrowedAssetAta` and Kamino repays from it.
        .step(
            step::require(
                account_data(
                    "borrowedAssetAta",
                    TOKEN_ACCOUNT_OWNER_OFFSET,
                    ReadType::Pubkey,
                )
                .eq(key("borrower")),
            )
            .label("swapPaysTheBorrower"),
        )
        .step(
            step::snapshot("balanceBefore", balance_of("borrowedAssetAta"))
                .label("readBalanceBeforeSwap"),
        )
        .step(platform_fee_within_cap())
        .step(
            step::invoke("jupiter")
                .readonly("tokenProgram")
                .signer("borrower")
                .writable("collateralAta")
                .writable("borrowedAssetAta")
                .account_group("routeAccounts")
                .data_parts(jupiter_route_data(
                    input("inAmount"),
                    input("quotedOutAmount"),
                ))
                .label("swapCollateralIntoDebtAsset"),
        )
        .step(
            step::let_(
                "swapped",
                balance_of("borrowedAssetAta") - snapshot("balanceBefore"),
            )
            .label("measureSwapOutput"),
        )
        .step(
            step::require(var("swapped").gte(input("minimumRepayment"))).label("swapWorthRepaying"),
        )
        // v2: the v1 handler refuses every caller but Kamino itself and a short whitelist.
        .step(
            step::invoke("kamino")
                .signer("borrower")
                .writable("obligation")
                .readonly("lendingMarket")
                .writable("repayReserve")
                .readonly("reserveLiquidityMint")
                .writable("reserveLiquiditySupply")
                // The repayment draws from the account the swap paid into.
                .writable("borrowedAssetAta")
                .readonly("tokenProgram")
                .readonly("instructionsSysvar")
                .account_group("farmAccounts")
                .data(data::literal(kamino_repay()))
                // Exactly what the swap produced, measured a moment ago.
                .data(data::u64(var("swapped")))
                .label("repayWhatTheSwapProduced"),
        )
}
// #endregion kamino-repay

// ===================================================================================== Orca

// #region orca-compound
/// Collect a Whirlpools position's fees and add them back as liquidity.
pub fn orca_compound_fees() -> Template {
    Template::new()
        .input("dustFloor", Type::U64)
        .input("minSqrtPrice", Type::U128)
        .input("maxSqrtPrice", Type::U128)
        .account("whirlpoolProgram", account::program(ORCA_WHIRLPOOL))
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account("memoProgram", account::program(MEMO_PROGRAM))
        .account("positionAuthority", account::signer())
        .account("whirlpool", account::writable())
        .account(
            "position",
            account::writable()
                .owner(ORCA_WHIRLPOOL)
                .min_data_length(ORCA_POSITION_LENGTH),
        )
        .account(
            "positionTokenAccount",
            account::readonly()
                .unsafe_unpinned()
                .min_data_length(TOKEN_ACCOUNT_LENGTH),
        )
        .account("tokenMintA", account::readonly())
        .account("tokenMintB", account::readonly())
        .account("tokenOwnerAccountA", token_account())
        .account("tokenOwnerAccountB", token_account())
        .account("tokenVaultA", account::writable())
        .account("tokenVaultB", account::writable())
        .account("tickArrayLower", account::writable())
        .account("tickArrayUpper", account::writable())
        // The holder is positionTokenAccount's owner, not positionAuthority, which may only be its
        // delegate.
        .step(
            step::let_(
                "positionHolder",
                account_data(
                    "positionTokenAccount",
                    TOKEN_ACCOUNT_OWNER_OFFSET,
                    ReadType::Pubkey,
                ),
            )
            .label("readPositionHolder"),
        )
        .step(
            step::require(
                account_data(
                    "tokenOwnerAccountA",
                    TOKEN_ACCOUNT_OWNER_OFFSET,
                    ReadType::Pubkey,
                )
                .eq(var("positionHolder"))
                .and(
                    account_data(
                        "tokenOwnerAccountB",
                        TOKEN_ACCOUNT_OWNER_OFFSET,
                        ReadType::Pubkey,
                    )
                    .eq(var("positionHolder")),
                ),
            )
            .label("feesGoToThePositionHolder"),
        )
        .step(
            step::let_(
                "hasLiquidity",
                account_data("position", ORCA_POSITION_LIQUIDITY, ReadType::U128).gt(u128(0)),
            )
            .label("readLiquidity"),
        )
        // Folds the pool's fee growth into the position, so the owed fees are current.
        .step(
            step::invoke("whirlpoolProgram")
                .writable("whirlpool")
                .writable("position")
                .readonly("tickArrayLower")
                .readonly("tickArrayUpper")
                .data(data::literal(orca_update_fees_and_rewards()))
                .when(var("hasLiquidity"))
                .label("updateFees"),
        )
        // Read after the update, which makes them current, and before the collect, which zeroes them.
        .step(
            step::let_(
                "owedA",
                account_data("position", ORCA_POSITION_FEE_OWED_A, ReadType::U64),
            )
            .label("readFeesOwedA"),
        )
        .step(
            step::let_(
                "owedB",
                account_data("position", ORCA_POSITION_FEE_OWED_B, ReadType::U64),
            )
            .label("readFeesOwedB"),
        )
        .step(step::let_("earnedA", var("owedA").gt(input("dustFloor"))))
        .step(step::let_("earnedB", var("owedB").gt(input("dustFloor"))))
        .step(
            step::invoke("whirlpoolProgram")
                .readonly("whirlpool")
                .signer("positionAuthority")
                .writable("position")
                .readonly("positionTokenAccount")
                .writable("tokenOwnerAccountA")
                .writable("tokenVaultA")
                .writable("tokenOwnerAccountB")
                .writable("tokenVaultB")
                .readonly("tokenProgram")
                .data(data::literal(orca_collect_fees()))
                // Either fee is worth collecting.
                .when(var("earnedA").or(var("earnedB")))
                .label("collectFees"),
        )
        // By token amounts: with the fees as caps, Whirlpools works out the most liquidity they buy
        // at the price when it runs.
        .step(
            step::invoke("whirlpoolProgram")
                .writable("whirlpool")
                .readonly("tokenProgram")
                .readonly("tokenProgram")
                .readonly("memoProgram")
                .signer("positionAuthority")
                .writable("position")
                .readonly("positionTokenAccount")
                .readonly("tokenMintA")
                .readonly("tokenMintB")
                .writable("tokenOwnerAccountA")
                .writable("tokenOwnerAccountB")
                .writable("tokenVaultA")
                .writable("tokenVaultB")
                .writable("tickArrayLower")
                .writable("tickArrayUpper")
                .data(data::literal(orca_increase_liquidity_by_token_amounts_v2()))
                .data(data::literal(ORCA_BY_TOKEN_AMOUNTS))
                .data(data::u64(var("owedA")))
                .data(data::u64(var("owedB")))
                .data(data::u128(input("minSqrtPrice")))
                .data(data::u128(input("maxSqrtPrice")))
                .data(data::literal(OPTION_NONE))
                // In range, liquidity needs both tokens; an emptied position stays empty.
                .when(var("hasLiquidity").and(var("earnedA").and(var("earnedB"))))
                .label("compoundFees"),
        )
}
// #endregion orca-compound

// #region orca-harvest
/// Collect the fees of many Whirlpools positions, each only when it is worth it.
pub fn orca_harvest_many_positions() -> Template {
    let position = account::iteration("position");
    let above_floor =
        |offset: u32| account_data(position.clone(), offset, ReadType::U64).gt(input("dustFloor"));
    Template::new()
        .input("dustFloor", Type::U64)
        .account("whirlpoolProgram", account::program(ORCA_WHIRLPOOL))
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account("positionAuthority", account::signer())
        .account("whirlpool", account::writable())
        .account("tokenOwnerAccountA", token_account())
        .account("tokenOwnerAccountB", token_account())
        .account("tokenVaultA", account::writable())
        .account("tokenVaultB", account::writable())
        .batch(
            Batch::new(12)
                .min_iterations(1)
                .account(
                    "position",
                    account::writable()
                        .owner(ORCA_WHIRLPOOL)
                        .min_data_length(ORCA_POSITION_LENGTH),
                )
                .account(
                    "positionTokenAccount",
                    account::readonly()
                        .unsafe_unpinned()
                        .min_data_length(TOKEN_ACCOUNT_LENGTH),
                )
                .account("tickArrayLower", account::readonly())
                .account("tickArrayUpper", account::readonly()),
        )
        // Fixed accounts, shared by every row: read once for the whole batch, not once per row.
        .step(
            step::let_(
                "feeOwnerA",
                account_data(
                    "tokenOwnerAccountA",
                    TOKEN_ACCOUNT_OWNER_OFFSET,
                    ReadType::Pubkey,
                ),
            )
            .label("readFeeOwnerA"),
        )
        .step(
            step::let_(
                "feeOwnerB",
                account_data(
                    "tokenOwnerAccountB",
                    TOKEN_ACCOUNT_OWNER_OFFSET,
                    ReadType::Pubkey,
                ),
            )
            .label("readFeeOwnerB"),
        )
        .step(
            step::for_each()
                .step(
                    step::let_(
                        "positionHolder",
                        account_data(
                            account::iteration("positionTokenAccount"),
                            TOKEN_ACCOUNT_OWNER_OFFSET,
                            ReadType::Pubkey,
                        ),
                    )
                    .label("readPositionHolder"),
                )
                // The holder is the NFT account's owner, not `positionAuthority`, which may be a
                // delegate.
                .step(
                    step::require(
                        var("positionHolder")
                            .eq(var("feeOwnerA"))
                            .and(var("positionHolder").eq(var("feeOwnerB"))),
                    )
                    .label("positionBelongsToTheFeeOwner"),
                )
                // Folds the pool's fee growth into the position, so the owed fees are current.
                .step(
                    step::invoke("whirlpoolProgram")
                        .writable("whirlpool")
                        .writable(position.clone())
                        .readonly(account::iteration("tickArrayLower"))
                        .readonly(account::iteration("tickArrayUpper"))
                        .data(data::literal(orca_update_fees_and_rewards()))
                        .when(
                            account_data(position.clone(), ORCA_POSITION_LIQUIDITY, ReadType::U128)
                                .gt(u128(0)),
                        )
                        .label("updateIfLiquid"),
                )
                .step(
                    step::invoke("whirlpoolProgram")
                        .readonly("whirlpool")
                        .signer("positionAuthority")
                        .writable(position.clone())
                        .readonly(account::iteration("positionTokenAccount"))
                        .writable("tokenOwnerAccountA")
                        .writable("tokenVaultA")
                        .writable("tokenOwnerAccountB")
                        .writable("tokenVaultB")
                        .readonly("tokenProgram")
                        .data(data::literal(orca_collect_fees()))
                        // This row's own fees, just updated, decide whether it collects.
                        .when(
                            above_floor(ORCA_POSITION_FEE_OWED_A)
                                .or(above_floor(ORCA_POSITION_FEE_OWED_B)),
                        )
                        .label("collectIfWorthIt"),
                )
                .label("everyPosition"),
        )
}
// #endregion orca-harvest

// ===================================================================================== Pyth

// #region pyth-gate
/// Act on a Jupiter route only while a fresh Pyth price sits inside a band.
pub fn pyth_fresh_price_gate() -> Template {
    // Valid only once the verification level has been pinned to `Full`; see the first require.
    let price = account_data("priceUpdate", PYTH_PRICE, ReadType::I64);
    let confidence = account_data("priceUpdate", PYTH_CONFIDENCE, ReadType::U64);
    let publish_time = account_data("priceUpdate", PYTH_PUBLISH_TIME, ReadType::I64);
    Template::new()
        // The feed the price must come from, as its 32-byte id.
        .input("feedId", Type::Pubkey)
        // The feed's exponent, which the bounds are in units of: SOL/USD's is −8.
        .input("exponent", Type::I64)
        // How stale a price may be, in seconds.
        .input("maximumAge", Type::I64)
        // The widest confidence interval the caller will act on.
        .input("maximumConfidence", Type::U64)
        .input("floorPrice", Type::I64)
        .input("ceilingPrice", Type::I64)
        .input("routePlan", Type::Bytes(512))
        .input("inAmount", Type::U64)
        .input("quotedOutAmount", Type::U64)
        .input("slippageBps", Type::U64)
        .input("platformFeeBps", Type::U64)
        // Pinning the owner is what makes the offsets meaningful.
        .account(
            "priceUpdate",
            account::readonly()
                .owner(PYTH_RECEIVER)
                .min_data_length(PYTH_LENGTH),
        )
        .account("actionProgram", account::program(JUPITER_V6))
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account("actor", account::signer().writable())
        .account_group("actionAccounts")
        // Fixes the layout. Without this the offsets below are a guess.
        .step(
            step::require(
                account_data("priceUpdate", PYTH_VERIFICATION_LEVEL, ReadType::U8)
                    .eq(u64(PYTH_VERIFICATION_LEVEL_FULL)),
            )
            .label("priceIsFullyVerified"),
        )
        // Which feed the price belongs to.
        .step(
            step::require(
                account_data("priceUpdate", PYTH_FEED_ID, ReadType::Pubkey).eq(input("feedId")),
            )
            .label("priceIsTheExpectedFeed"),
        )
        // What the raw integers below mean.
        .step(
            step::require(
                account_data("priceUpdate", PYTH_EXPONENT, ReadType::I32).eq(input("exponent")),
            )
            .label("priceExponentIsExpected"),
        )
        .step(
            step::require((clock_unix_timestamp() - publish_time).lte(input("maximumAge")))
                .label("priceIsFresh"),
        )
        // A wide confidence interval means the publishers disagree; treat it as no price at all.
        .step(step::require(confidence.lte(input("maximumConfidence"))).label("publishersAgree"))
        .step(step::require(price.clone().gte(input("floorPrice"))).label("priceAboveFloor"))
        .step(step::require(price.lte(input("ceilingPrice"))).label("priceBelowCeiling"))
        .step(platform_fee_within_cap())
        .step(
            step::invoke("actionProgram")
                .readonly("tokenProgram")
                .signer("actor")
                .account_group("actionAccounts")
                .data_parts(jupiter_route_data(
                    input("inAmount"),
                    input("quotedOutAmount"),
                ))
                .label("actOnTheOracle"),
        )
}
// #endregion pyth-gate

// ========================================================================== signed quotes

// #region signed-quote
/// The signed quote's layout. Integers are little-endian; keys are their 32 raw bytes.
const QUOTE_LENGTH: u32 = 128;
const QUOTE_TAG_OFFSET: u32 = 0;
const QUOTE_PRICE: u32 = 8;
const QUOTE_MAX_AMOUNT: u32 = 16;
const QUOTE_EXPIRY: u32 = 24;
const QUOTE_TAKER: u32 = 32;
const QUOTE_BASE_MINT: u32 = 64;
const QUOTE_QUOTE_MINT: u32 = 96;

/// The eight bytes every quote starts with.
const QUOTE_TAG: [u8; 8] = *b"BLSTQT01";

/// Prices carry six decimals: a price of 1,000,000 is one quote unit per base unit.
const PRICE_SCALE: u64 = 1_000_000;

/// Settle a trade at a price the maker signed, in the instruction before this run.
pub fn signed_quote_settlement() -> Template {
    // The maker's signature, in the instruction directly before this template's run.
    let quote = ed25519_signature(
        "instructions",
        current_instruction_index("instructions") - u64(1),
        key("maker"),
        QUOTE_LENGTH,
    )
    .name("quote");

    Template::new()
        // Base-token base units to take, up to the quoted maximum.
        .input("amount", Type::U64)
        .account(
            "instructions",
            account::readonly().address(INSTRUCTIONS_SYSVAR_ID),
        )
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account("taker", account::signer())
        .account("maker", account::signer())
        // Pays, in the quote mint.
        .account("takerQuoteAccount", token_account())
        // Is paid, in the quote mint.
        .account("makerQuoteAccount", token_account())
        // Delivers, in the base mint.
        .account("makerBaseAccount", token_account())
        // Receives, in the base mint.
        .account("takerBaseAccount", token_account())
        .steps(quote.steps())
        .step(
            step::require(
                quote
                    .field(QUOTE_TAG_OFFSET, ReadType::U64)
                    .eq(u64(u64::from_le_bytes(QUOTE_TAG))),
            )
            .label("quoteIsTagged"),
        )
        .step(
            step::require(clock_unix_timestamp().lte(quote.field(QUOTE_EXPIRY, ReadType::I64)))
                .label("quoteHasNotExpired"),
        )
        .step(
            step::require(quote.field(QUOTE_TAKER, ReadType::Pubkey).eq(key("taker")))
                .label("quoteIsForThisTaker"),
        )
        .step(
            step::require(input("amount").lte(quote.field(QUOTE_MAX_AMOUNT, ReadType::U64)))
                .label("withinTheQuotedSize"),
        )
        // A token `transfer` moves only between two accounts of one mint, so pinning one side of
        // each leg pins both.
        .step(
            step::require(
                account_data(
                    "takerQuoteAccount",
                    TOKEN_ACCOUNT_MINT_OFFSET,
                    ReadType::Pubkey,
                )
                .eq(quote.field(QUOTE_QUOTE_MINT, ReadType::Pubkey)),
            )
            .label("paysInTheQuotedMint"),
        )
        .step(
            step::require(
                account_data(
                    "makerBaseAccount",
                    TOKEN_ACCOUNT_MINT_OFFSET,
                    ReadType::Pubkey,
                )
                .eq(quote.field(QUOTE_BASE_MINT, ReadType::Pubkey)),
            )
            .label("deliversTheQuotedMint"),
        )
        // The payment reaches an account the maker owns, not one the taker picked.
        .step(
            step::require(
                account_data(
                    "makerQuoteAccount",
                    TOKEN_ACCOUNT_OWNER_OFFSET,
                    ReadType::Pubkey,
                )
                .eq(key("maker")),
            )
            .label("paymentReachesTheMaker"),
        )
        .step(
            step::let_(
                "payment",
                input("amount")
                    .mul_div_up(quote.field(QUOTE_PRICE, ReadType::U64), u64(PRICE_SCALE)),
            )
            .label("priceTheFill"),
        )
        .step(
            token_transfer(
                "tokenProgram",
                "takerQuoteAccount",
                "makerQuoteAccount",
                "taker",
                var("payment"),
            )
            .label("takerPays"),
        )
        .step(
            token_transfer(
                "tokenProgram",
                "makerBaseAccount",
                "takerBaseAccount",
                "maker",
                input("amount"),
            )
            .label("makerDelivers"),
        )
}
// #endregion signed-quote

// ============================================================================ token sweep

// #region token-sweep
/// Sell a token account's whole balance through Jupiter, at the quote rescaled to it.
pub fn token_sweep_into_swap() -> Template {
    Template::new()
        .input("routePlan", Type::Bytes(512))
        // The `in_amount` the route was quoted for.
        .input("quotedInAmount", Type::U64)
        // The quote's `quoted_out_amount` for that input.
        .input("quotedOutAmount", Type::U64)
        .input("slippageBps", Type::U64)
        .input("platformFeeBps", Type::U64)
        // Do not sell less than this.
        .input("dustFloor", Type::U64)
        .account("jupiter", account::program(JUPITER_V6))
        .account("tokenProgram", account::program(TOKEN_PROGRAM_ID))
        .account("seller", account::signer().writable())
        .account("sourceAta", token_account())
        .account("destinationAta", token_account())
        .account_group("routeAccounts")
        // Both ends of the sale are the seller's.
        .step(
            step::require(
                account_data("sourceAta", TOKEN_ACCOUNT_OWNER_OFFSET, ReadType::Pubkey)
                    .eq(key("seller")),
            )
            .label("sweepsTheSellersOwnBalance"),
        )
        .step(
            step::require(
                account_data(
                    "destinationAta",
                    TOKEN_ACCOUNT_OWNER_OFFSET,
                    ReadType::Pubkey,
                )
                .eq(key("seller")),
            )
            .label("proceedsGoToTheSeller"),
        )
        .step(step::let_("available", balance_of("sourceAta")).label("readSellableBalance"))
        .step(step::require(var("available").gt(input("dustFloor"))).label("worthSelling"))
        // The quote was for `quotedInAmount`; selling `available` instead should fetch
        // proportionally more or less.
        .step(
            step::let_(
                "quotedOut",
                (input("quotedOutAmount").cast(Type::U128) * var("available").cast(Type::U128)
                    / input("quotedInAmount").cast(Type::U128))
                .cast(Type::U64),
            )
            .label("rescaleQuoteToBalance"),
        )
        .step(
            step::snapshot("proceedsBefore", balance_of("destinationAta"))
                .label("readProceedsBefore"),
        )
        .step(platform_fee_within_cap())
        .step(
            step::invoke("jupiter")
                .readonly("tokenProgram")
                .signer("seller")
                .writable("sourceAta")
                .writable("destinationAta")
                .account_group("routeAccounts")
                .data_parts(jupiter_route_data(var("available"), var("quotedOut")))
                .label("sell"),
        )
        // Jupiter checks this too. Checking it here, on the balances, holds whatever the route did.
        .step(
            step::require(
                (balance_of("destinationAta") - snapshot("proceedsBefore")).gte(
                    (var("quotedOut").cast(Type::U128)
                        * (u64(10_000) - input("slippageBps")).cast(Type::U128)
                        / u128(10_000))
                    .cast(Type::U64),
                ),
            )
            .label("saleMetTheQuote"),
        )
        // The whole balance was the input, so anything left means the route did not take it all.
        .step(
            step::require(balance_of("sourceAta").lte(input("dustFloor")))
                .label("nothingMeaningfulLeftBehind"),
        )
}
// #endregion token-sweep
