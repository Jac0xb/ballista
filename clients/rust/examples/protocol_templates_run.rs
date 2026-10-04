//! Build a run of each live-protocol template from Rust.
//!
//! Each run compiles its template from `protocol_templates.rs` and names every input and every
//! account the template declares. The run builder puts them in declaration order and takes each
//! account's signer and writable flags from its declaration. `tests/protocol_templates.rs` checks
//! each run below against the template it runs.
//!
//! No template refreshes Kamino: a run that deposits into, repays or liquidates an obligation goes
//! behind [`kamino_refreshes`], in the same transaction.
//!
//! ```bash
//! cargo run -p ballista-sdk --example protocol_templates_run
//! ```

#[path = "protocol_templates.rs"]
pub mod templates;

use std::error::Error;

use ballista_sdk::template::Row;
use ballista_sdk::{
    anchor_discriminator, find_registry_entry_address, ASSOCIATED_TOKEN_PROGRAM_ID,
    ED25519_PROGRAM_ID, INSTRUCTIONS_SYSVAR_ID, SYSTEM_PROGRAM_ID, TOKEN_2022_PROGRAM_ID,
    TOKEN_PROGRAM_ID,
};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey,
    pubkey::Pubkey,
};

const JUPITER_V6: Pubkey = pubkey!("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");
const KAMINO_LEND: Pubkey = pubkey!("KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD");
const KAMINO_FARMS: Pubkey = pubkey!("FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr");
const ORCA_WHIRLPOOL: Pubkey = pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");
const MEMO_PROGRAM: Pubkey = pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
const PUMP_FUN: Pubkey = pubkey!("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
const PUMP_FEES: Pubkey = pubkey!("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ");
const WRAPPED_SOL_MINT: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
const USDC_MINT: Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

/// `in_amount`, `quoted_out_amount`, `slippage_bps` and `platform_fee_bps`: `route`'s data after
/// its plan.
const ROUTE_TAIL_LEN: usize = 8 + 8 + 2 + 1;

// #region jupiter-route
/// Jupiter `route` data as the Swap API returns it with `useSharedAccounts: false`, split into
/// the plan and the four numbers after it. Every Jupiter template takes them as inputs, so that it
/// can cap `platform_fee_bps`.
pub struct RouteQuote<'a> {
    /// `route_plan`, Borsh length prefix included: the bytes between the discriminator and
    /// `in_amount`.
    pub route_plan: &'a [u8],
    pub in_amount: u64,
    pub quoted_out_amount: u64,
    pub slippage_bps: u16,
    pub platform_fee_bps: u8,
}

impl<'a> RouteQuote<'a> {
    pub fn split(route_data: &'a [u8]) -> Self {
        assert!(
            route_data.len() >= 8 + 4 + ROUTE_TAIL_LEN
                && route_data[..8] == anchor_discriminator("route"),
            "not Jupiter `route` data; ask the Swap API for useSharedAccounts: false"
        );
        let (route_plan, tail) = route_data[8..].split_at(route_data.len() - 8 - ROUTE_TAIL_LEN);
        RouteQuote {
            route_plan,
            in_amount: u64::from_le_bytes(tail[0..8].try_into().unwrap()),
            quoted_out_amount: u64::from_le_bytes(tail[8..16].try_into().unwrap()),
            slippage_bps: u16::from_le_bytes([tail[16], tail[17]]),
            platform_fee_bps: tail[18],
        }
    }
}
// #endregion jupiter-route

// #region kamino-refreshes
/// Kamino's refreshes, which go before the run in the same transaction. Kamino's v2 deposit,
/// repayment and liquidation check only that the obligation and the reserves they price were
/// refreshed in the current slot, not where.
///
/// - `held` is every reserve the obligation holds, deposits in its deposit order and then borrows
///   in its borrow order, each with the Scope price account its config names. The main market
///   prices by Scope alone, so the Pyth and Switchboard slots take the Kamino program ID, which
///   Kamino reads as "none".
/// - `touched` adds any other reserve the run needs fresh. A first deposit's needs none: Kamino's
///   deposit refreshes its own reserve.
/// - `referrer_token_states` is empty unless the obligation has a referrer; then Kamino expects
///   one per borrow after the reserves.
pub fn kamino_refreshes(
    lending_market: Pubkey,
    obligation: Pubkey,
    held: &[(Pubkey, Pubkey)],
    touched: &[(Pubkey, Pubkey)],
    referrer_token_states: &[Pubkey],
) -> Vec<Instruction> {
    let none = AccountMeta::new_readonly(KAMINO_LEND, false);
    let mut refreshed: Vec<Pubkey> = Vec::new();
    let mut instructions = Vec::new();
    for &(reserve, scope_prices) in held.iter().chain(touched) {
        if refreshed.contains(&reserve) {
            continue;
        }
        refreshed.push(reserve);
        instructions.push(Instruction {
            program_id: KAMINO_LEND,
            accounts: vec![
                AccountMeta::new(reserve, false),
                AccountMeta::new_readonly(lending_market, false),
                none.clone(), // Pyth
                none.clone(), // Switchboard price
                none.clone(), // Switchboard TWAP
                AccountMeta::new_readonly(scope_prices, false),
            ],
            data: anchor_discriminator("refresh_reserve").to_vec(),
        });
    }
    let mut accounts = vec![
        AccountMeta::new_readonly(lending_market, false),
        AccountMeta::new(obligation, false),
    ];
    accounts.extend(
        held.iter()
            .map(|&(reserve, _)| AccountMeta::new(reserve, false)),
    );
    accounts.extend(
        referrer_token_states
            .iter()
            .map(|&state| AccountMeta::new(state, false)),
    );
    instructions.push(Instruction {
        program_id: KAMINO_LEND,
        accounts,
        data: anchor_discriminator("refresh_obligation").to_vec(),
    });
    instructions
}

/// One farm in a Kamino v2 account tail: `(the obligation's user state in the farm, the farm)`,
/// both writable. When the reserve has no such farm, Kamino reads its own program ID, twice and
/// read-only, as "none".
pub fn kamino_farm_pair(farm: Option<(Pubkey, Pubkey)>) -> [AccountMeta; 2] {
    match farm {
        Some((user_state, farm_state)) => [
            AccountMeta::new(user_state, false),
            AccountMeta::new(farm_state, false),
        ],
        None => [
            AccountMeta::new_readonly(KAMINO_LEND, false),
            AccountMeta::new_readonly(KAMINO_LEND, false),
        ],
    }
}
// #endregion kamino-refreshes

// #region jupiter-deposit
pub struct JupiterDepositAccounts {
    pub owner: Pubkey,
    pub source_ata: Pubkey,
    pub destination_ata: Pubkey,
    pub obligation: Pubkey,
    pub lending_market: Pubkey,
    pub lending_market_authority: Pubkey,
    pub reserve: Pubkey,
    pub reserve_liquidity_mint: Pubkey,
    pub reserve_liquidity_supply: Pubkey,
    pub reserve_collateral_mint: Pubkey,
    pub reserve_destination_deposit_collateral: Pubkey,
    /// The reserve's collateral farm, if it has one; see [`kamino_farm_pair`].
    pub collateral_farm: Option<(Pubkey, Pubkey)>,
}

/// `route` is the Swap API's `route` data split by [`RouteQuote::split`], and `route_accounts` its
/// account list from the fifth account on: the template passes the first four (token program,
/// owner, source, destination) itself.
///
/// Send it behind [`kamino_refreshes`], in the same transaction.
pub fn run_jupiter_deposit(
    template: Pubkey,
    a: &JupiterDepositAccounts,
    route: &RouteQuote,
    minimum_out: u64,
    route_accounts: Vec<AccountMeta>,
) -> Result<Instruction, Box<dyn Error>> {
    // Kamino's v2 deposit ends in the farm pair and the Farms program.
    let mut farm_accounts = kamino_farm_pair(a.collateral_farm).to_vec();
    farm_accounts.push(AccountMeta::new_readonly(KAMINO_FARMS, false));
    let instruction = templates::jupiter_deposit_exact_output()
        .compile()?
        .run(template)
        .input("routePlan", route.route_plan)
        .input("inAmount", route.in_amount)
        .input("quotedOutAmount", route.quoted_out_amount)
        .input("slippageBps", route.slippage_bps)
        .input("platformFeeBps", route.platform_fee_bps)
        .input("minimumOut", minimum_out)
        .account("jupiter", JUPITER_V6)
        .account("kamino", KAMINO_LEND)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("instructionsSysvar", INSTRUCTIONS_SYSVAR_ID)
        .account("owner", a.owner)
        .account("sourceAta", a.source_ata)
        .account("destinationAta", a.destination_ata)
        .account("obligation", a.obligation)
        .account("lendingMarket", a.lending_market)
        .account("lendingMarketAuthority", a.lending_market_authority)
        .account("reserve", a.reserve)
        .account("reserveLiquidityMint", a.reserve_liquidity_mint)
        .account("reserveLiquiditySupply", a.reserve_liquidity_supply)
        .account("reserveCollateralMint", a.reserve_collateral_mint)
        .account(
            "reserveDestinationDepositCollateral",
            a.reserve_destination_deposit_collateral,
        )
        .group("routeAccounts", route_accounts)
        .group("farmAccounts", farm_accounts)
        .instruction()?;
    Ok(instruction)
}
// #endregion jupiter-deposit

// #region jupiter-oracle-swap
pub struct OracleSwapAccounts {
    /// A `PriceUpdateV2` for SOL/USD, the feed the template pins.
    pub price_update: Pubkey,
    pub trader: Pubkey,
    /// The trader's own wrapped SOL and USDC accounts.
    pub source_ata: Pubkey,
    pub destination_ata: Pubkey,
}

/// `route` is the Swap API's `route` data split by [`RouteQuote::split`], and `route_accounts` its
/// account list from the fifth account on. The feed, the two mints and the tolerance are the
/// template's own constants, not run inputs.
pub fn run_jupiter_oracle_swap(
    template: Pubkey,
    a: &OracleSwapAccounts,
    route: &RouteQuote,
    route_accounts: Vec<AccountMeta>,
) -> Result<Instruction, Box<dyn Error>> {
    let instruction = templates::jupiter_oracle_checked_swap()
        .compile()?
        .run(template)
        .input("routePlan", route.route_plan)
        .input("inAmount", route.in_amount)
        .input("quotedOutAmount", route.quoted_out_amount)
        .input("slippageBps", route.slippage_bps)
        .input("platformFeeBps", route.platform_fee_bps)
        .account("jupiter", JUPITER_V6)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("priceUpdate", a.price_update)
        .account("trader", a.trader)
        .account("sourceAta", a.source_ata)
        .account("destinationAta", a.destination_ata)
        .account("sourceMint", WRAPPED_SOL_MINT)
        .account("destinationMint", USDC_MINT)
        .group("routeAccounts", route_accounts)
        .instruction()?;
    Ok(instruction)
}
// #endregion jupiter-oracle-swap

// #region token-sweep
pub struct TokenSweepAccounts {
    pub seller: Pubkey,
    /// The seller's own token accounts: what is sold, and where the proceeds land.
    pub source_ata: Pubkey,
    pub destination_ata: Pubkey,
}

/// `quote` is a Swap API route for any amount, split by [`RouteQuote::split`]: the template sells
/// the whole balance and rescales the quote to it. `route_accounts` is the route's account list
/// from the fifth account on.
pub fn run_token_sweep(
    template: Pubkey,
    a: &TokenSweepAccounts,
    quote: &RouteQuote,
    dust_floor: u64,
    route_accounts: Vec<AccountMeta>,
) -> Result<Instruction, Box<dyn Error>> {
    let instruction = templates::token_sweep_into_swap()
        .compile()?
        .run(template)
        .input("routePlan", quote.route_plan)
        .input("quotedInAmount", quote.in_amount)
        .input("quotedOutAmount", quote.quoted_out_amount)
        .input("slippageBps", quote.slippage_bps)
        .input("platformFeeBps", quote.platform_fee_bps)
        .input("dustFloor", dust_floor)
        .account("jupiter", JUPITER_V6)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("seller", a.seller)
        .account("sourceAta", a.source_ata)
        .account("destinationAta", a.destination_ata)
        .group("routeAccounts", route_accounts)
        .instruction()?;
    Ok(instruction)
}
// #endregion token-sweep

// #region pyth-gate
pub struct PriceGate {
    /// The Pyth `PriceUpdateV2` account.
    pub price_update: Pubkey,
    /// The feed the price must be, as 32 bytes: SOL/USD's is
    /// `ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d`.
    pub feed_id: [u8; 32],
    /// The exponent the bounds are in units of: SOL/USD's is −8.
    pub exponent: i32,
    pub actor: Pubkey,
}

/// `route` is the Swap API's `route` data split by [`RouteQuote::split`], and `action_accounts`
/// its account list from the third account on: the template passes the token program and the
/// actor itself.
pub fn run_pyth_gate(
    template: Pubkey,
    gate: &PriceGate,
    maximum_age: i64,
    maximum_confidence: u64,
    (floor_price, ceiling_price): (i64, i64),
    route: &RouteQuote,
    action_accounts: Vec<AccountMeta>,
) -> Result<Instruction, Box<dyn Error>> {
    let instruction = templates::pyth_fresh_price_gate()
        .compile()?
        .run(template)
        .input("feedId", gate.feed_id)
        // Pyth stores the exponent as an i32; the template takes it as an i64.
        .input("exponent", gate.exponent)
        .input("maximumAge", maximum_age)
        .input("maximumConfidence", maximum_confidence)
        .input("floorPrice", floor_price)
        .input("ceilingPrice", ceiling_price)
        .input("routePlan", route.route_plan)
        .input("inAmount", route.in_amount)
        .input("quotedOutAmount", route.quoted_out_amount)
        .input("slippageBps", route.slippage_bps)
        .input("platformFeeBps", route.platform_fee_bps)
        .account("priceUpdate", gate.price_update)
        .account("actionProgram", JUPITER_V6)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("actor", gate.actor)
        .group("actionAccounts", action_accounts)
        .instruction()?;
    Ok(instruction)
}
// #endregion pyth-gate

// #region jupiter-daily-cap
pub struct DailyCapAccounts {
    /// Signs, keys the entry, and pays its rent on the first run.
    pub actor: Pubkey,
    /// The actor's wrapped-SOL token account, which the route sells from.
    pub source_ata: Pubkey,
}

/// `route` is the Swap API's `route` data split by [`RouteQuote::split`], and `action_accounts`
/// its account list from the fourth account on: the template passes the token program, the actor
/// and the source itself. The actor's entry is its `dailySpend` entry for its own address: the
/// actor's first run creates it, and pays its rent.
pub fn run_jupiter_daily_cap(
    template: Pubkey,
    a: &DailyCapAccounts,
    route: &RouteQuote,
    action_accounts: Vec<AccountMeta>,
) -> Result<Instruction, Box<dyn Error>> {
    let compiled = templates::jupiter_daily_cap_swap().compile()?;
    let daily_spend = compiled
        .registry_index("dailySpend")
        .ok_or("no dailySpend registry")?;
    let (spend, _) = find_registry_entry_address(&template, daily_spend, &a.actor.to_bytes());
    let instruction = compiled
        .run(template)
        .input("routePlan", route.route_plan)
        .input("inAmount", route.in_amount)
        .input("quotedOutAmount", route.quoted_out_amount)
        .input("slippageBps", route.slippage_bps)
        .input("platformFeeBps", route.platform_fee_bps)
        .account("actionProgram", JUPITER_V6)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("actor", a.actor)
        .account("sourceAta", a.source_ata)
        .account("spend", spend)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .group("actionAccounts", action_accounts)
        .instruction()?;
    Ok(instruction)
}
// #endregion jupiter-daily-cap

// #region orca-compound
pub struct OrcaCompoundAccounts {
    /// Signs for the position: the holder of its NFT, or a delegate approved on it.
    pub position_authority: Pubkey,
    pub whirlpool: Pubkey,
    pub position: Pubkey,
    /// The token account holding the position's NFT. Its owner is the position's holder.
    pub position_token_account: Pubkey,
    pub token_mint_a: Pubkey,
    pub token_mint_b: Pubkey,
    /// The holder's own token accounts: the fees go there and are reinvested from there.
    pub token_owner_account_a: Pubkey,
    pub token_owner_account_b: Pubkey,
    pub token_vault_a: Pubkey,
    pub token_vault_b: Pubkey,
    /// The tick arrays holding the position's lower and upper ticks.
    pub tick_array_lower: Pubkey,
    pub tick_array_upper: Pubkey,
}

/// `dust_floor` is not safe at 0: a fee too small to buy any liquidity fails the whole run.
/// `sqrt_price_bounds` is `(min, max)`, the pool sqrt prices (Q64.64) the deposit accepts:
/// Orca's `get_sqrt_price_slippage_bounds` for the current price and a tolerance.
pub fn run_orca_compound(
    template: Pubkey,
    a: &OrcaCompoundAccounts,
    dust_floor: u64,
    (min_sqrt_price, max_sqrt_price): (u128, u128),
) -> Result<Instruction, Box<dyn Error>> {
    let instruction = templates::orca_compound_fees()
        .compile()?
        .run(template)
        .input("dustFloor", dust_floor)
        .input("minSqrtPrice", min_sqrt_price)
        .input("maxSqrtPrice", max_sqrt_price)
        .account("whirlpoolProgram", ORCA_WHIRLPOOL)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("memoProgram", MEMO_PROGRAM)
        .account("positionAuthority", a.position_authority)
        .account("whirlpool", a.whirlpool)
        .account("position", a.position)
        .account("positionTokenAccount", a.position_token_account)
        .account("tokenMintA", a.token_mint_a)
        .account("tokenMintB", a.token_mint_b)
        .account("tokenOwnerAccountA", a.token_owner_account_a)
        .account("tokenOwnerAccountB", a.token_owner_account_b)
        .account("tokenVaultA", a.token_vault_a)
        .account("tokenVaultB", a.token_vault_b)
        .account("tickArrayLower", a.tick_array_lower)
        .account("tickArrayUpper", a.tick_array_upper)
        .instruction()?;
    Ok(instruction)
}
// #endregion orca-compound

// #region orca-harvest
pub struct OrcaHarvestAccounts {
    pub position_authority: Pubkey,
    pub whirlpool: Pubkey,
    /// The positions' holder's own token accounts, where every row's fees go.
    pub token_owner_account_a: Pubkey,
    pub token_owner_account_b: Pubkey,
    pub token_vault_a: Pubkey,
    pub token_vault_b: Pubkey,
}

/// One row per position.
pub struct OrcaHarvestRow {
    pub position: Pubkey,
    /// The token account holding the position's NFT.
    pub position_token_account: Pubkey,
    /// The tick arrays holding the position's lower and upper ticks.
    pub tick_array_lower: Pubkey,
    pub tick_array_upper: Pubkey,
}

/// 1 to 12 rows, all of one holder: the fee accounts are fixed for the batch, and each row's NFT
/// must be held by their owner. The row count comes from the account list, so there is no count
/// to pass. A row that collects costs about 24,000 compute units, so more than eight need a
/// compute budget.
pub fn run_orca_harvest(
    template: Pubkey,
    a: &OrcaHarvestAccounts,
    rows: &[OrcaHarvestRow],
    dust_floor: u64,
) -> Result<Instruction, Box<dyn Error>> {
    let instruction = templates::orca_harvest_many_positions()
        .compile()?
        .run(template)
        .input("dustFloor", dust_floor)
        .account("whirlpoolProgram", ORCA_WHIRLPOOL)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("positionAuthority", a.position_authority)
        .account("whirlpool", a.whirlpool)
        .account("tokenOwnerAccountA", a.token_owner_account_a)
        .account("tokenOwnerAccountB", a.token_owner_account_b)
        .account("tokenVaultA", a.token_vault_a)
        .account("tokenVaultB", a.token_vault_b)
        .rows(rows.iter().map(|row| {
            Row::new()
                .account("position", row.position)
                .account("positionTokenAccount", row.position_token_account)
                .account("tickArrayLower", row.tick_array_lower)
                .account("tickArrayUpper", row.tick_array_upper)
        }))
        .instruction()?;
    Ok(instruction)
}
// #endregion orca-harvest

// #region kamino-repay
pub struct KaminoRepayAccounts {
    pub borrower: Pubkey,
    pub collateral_ata: Pubkey,
    /// The borrower's own token account: it receives the swap and funds the repayment.
    pub borrowed_asset_ata: Pubkey,
    pub obligation: Pubkey,
    pub lending_market: Pubkey,
    /// The repayment's tail takes the market's authority PDA.
    pub lending_market_authority: Pubkey,
    pub repay_reserve: Pubkey,
    pub reserve_liquidity_mint: Pubkey,
    pub reserve_liquidity_supply: Pubkey,
    /// The reserve's debt farm, if it has one; see [`kamino_farm_pair`].
    pub debt_farm: Option<(Pubkey, Pubkey)>,
}

/// `route` is the Swap API's `route` data split by [`RouteQuote::split`], and `route_accounts` its
/// account list from the fifth account on.
///
/// Send it behind [`kamino_refreshes`], in the same transaction.
pub fn run_kamino_repay(
    template: Pubkey,
    a: &KaminoRepayAccounts,
    route: &RouteQuote,
    minimum_repayment: u64,
    route_accounts: Vec<AccountMeta>,
) -> Result<Instruction, Box<dyn Error>> {
    // Kamino's v2 repayment ends in the farm pair, the lending market authority and Farms.
    let mut farm_accounts = kamino_farm_pair(a.debt_farm).to_vec();
    farm_accounts.push(AccountMeta::new_readonly(a.lending_market_authority, false));
    farm_accounts.push(AccountMeta::new_readonly(KAMINO_FARMS, false));
    let instruction = templates::kamino_repay_swap_output()
        .compile()?
        .run(template)
        .input("routePlan", route.route_plan)
        .input("inAmount", route.in_amount)
        .input("quotedOutAmount", route.quoted_out_amount)
        .input("slippageBps", route.slippage_bps)
        .input("platformFeeBps", route.platform_fee_bps)
        .input("minimumRepayment", minimum_repayment)
        .account("jupiter", JUPITER_V6)
        .account("kamino", KAMINO_LEND)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("instructionsSysvar", INSTRUCTIONS_SYSVAR_ID)
        .account("borrower", a.borrower)
        .account("collateralAta", a.collateral_ata)
        .account("borrowedAssetAta", a.borrowed_asset_ata)
        .account("obligation", a.obligation)
        .account("lendingMarket", a.lending_market)
        .account("repayReserve", a.repay_reserve)
        .account("reserveLiquidityMint", a.reserve_liquidity_mint)
        .account("reserveLiquiditySupply", a.reserve_liquidity_supply)
        .group("routeAccounts", route_accounts)
        .group("farmAccounts", farm_accounts)
        .instruction()?;
    Ok(instruction)
}
// #endregion kamino-repay

// #region kamino-liquidate
pub struct KaminoLiquidateAccounts {
    pub liquidator: Pubkey,
    pub obligation: Pubkey,
    pub lending_market: Pubkey,
    pub lending_market_authority: Pubkey,
    pub repay_reserve: Pubkey,
    pub repay_reserve_liquidity_mint: Pubkey,
    pub repay_reserve_liquidity_supply: Pubkey,
    pub withdraw_reserve: Pubkey,
    pub withdraw_reserve_liquidity_mint: Pubkey,
    pub withdraw_reserve_collateral_mint: Pubkey,
    pub withdraw_reserve_collateral_supply: Pubkey,
    pub withdraw_reserve_liquidity_supply: Pubkey,
    /// The withdrawn reserve's fee vault, which takes Kamino's fee on the seized collateral.
    pub withdraw_reserve_fee_receiver: Pubkey,
    /// Pays the repayment.
    pub user_source_liquidity: Pubkey,
    /// The liquidator's own accounts for the seized cTokens and for what they redeem to, where
    /// the bounty is measured.
    pub user_destination_collateral: Pubkey,
    pub user_destination_liquidity: Pubkey,
    /// The withdrawn reserve's collateral farm and the repaid reserve's debt farm, if they exist;
    /// see [`kamino_farm_pair`].
    pub collateral_farm: Option<(Pubkey, Pubkey)>,
    pub debt_farm: Option<(Pubkey, Pubkey)>,
}

/// `liquidity_amount` is the most debt to repay, in the repaid token's base units.
/// `min_acceptable_received` is Kamino's own floor on its figure for the payout, net of its fee; 0
/// for none. `minimum_bounty` is what `user_destination_liquidity` must gain, in the collateral's
/// base units (lamports for SOL): what the liquidator receives, not its profit.
///
/// Send it behind [`kamino_refreshes`], in the same transaction.
pub fn run_kamino_liquidate(
    template: Pubkey,
    a: &KaminoLiquidateAccounts,
    liquidity_amount: u64,
    min_acceptable_received: u64,
    minimum_bounty: u64,
) -> Result<Instruction, Box<dyn Error>> {
    // Kamino's v2 liquidation ends in both farm pairs and Farms.
    let mut farm_accounts = kamino_farm_pair(a.collateral_farm).to_vec();
    farm_accounts.extend(kamino_farm_pair(a.debt_farm));
    farm_accounts.push(AccountMeta::new_readonly(KAMINO_FARMS, false));
    let instruction = templates::kamino_liquidate_with_proof()
        .compile()?
        .run(template)
        .input("liquidityAmount", liquidity_amount)
        .input("minAcceptableReceived", min_acceptable_received)
        .input("minimumBounty", minimum_bounty)
        .account("kamino", KAMINO_LEND)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("instructionsSysvar", INSTRUCTIONS_SYSVAR_ID)
        .account("liquidator", a.liquidator)
        .account("obligation", a.obligation)
        .account("lendingMarket", a.lending_market)
        .account("lendingMarketAuthority", a.lending_market_authority)
        .account("repayReserve", a.repay_reserve)
        .account("repayReserveLiquidityMint", a.repay_reserve_liquidity_mint)
        .account(
            "repayReserveLiquiditySupply",
            a.repay_reserve_liquidity_supply,
        )
        .account("withdrawReserve", a.withdraw_reserve)
        .account(
            "withdrawReserveLiquidityMint",
            a.withdraw_reserve_liquidity_mint,
        )
        .account(
            "withdrawReserveCollateralMint",
            a.withdraw_reserve_collateral_mint,
        )
        .account(
            "withdrawReserveCollateralSupply",
            a.withdraw_reserve_collateral_supply,
        )
        .account(
            "withdrawReserveLiquiditySupply",
            a.withdraw_reserve_liquidity_supply,
        )
        .account(
            "withdrawReserveFeeReceiver",
            a.withdraw_reserve_fee_receiver,
        )
        .account("userSourceLiquidity", a.user_source_liquidity)
        .account("userDestinationCollateral", a.user_destination_collateral)
        .account("userDestinationLiquidity", a.user_destination_liquidity)
        .group("farmAccounts", farm_accounts)
        .instruction()?;
    Ok(instruction)
}
// #endregion kamino-liquidate

// #region signed-quote
/// The quote a maker signs off chain, as `signed-quote-settlement.ts` reads it.
pub struct Quote {
    /// Quote-token base units per 1,000,000 base-token base units.
    pub price: u64,
    /// The most base-token base units the maker delivers in one settlement. The quote can settle
    /// again until it expires, so this bounds each settlement, not the total.
    pub max_amount: u64,
    /// The last Unix timestamp at which the quote can settle.
    pub expiry: i64,
    /// The one wallet that can take the quote.
    pub taker: Pubkey,
    /// The mint the maker delivers, and the mint the taker pays in.
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
}

impl Quote {
    /// The 128 bytes the maker signs: the tag `BLSTQT01`, then the fields, integers
    /// little-endian.
    pub fn message(&self) -> Vec<u8> {
        let mut message = Vec::with_capacity(128);
        message.extend_from_slice(b"BLSTQT01");
        message.extend_from_slice(&self.price.to_le_bytes());
        message.extend_from_slice(&self.max_amount.to_le_bytes());
        message.extend_from_slice(&self.expiry.to_le_bytes());
        for key in [self.taker, self.base_mint, self.quote_mint] {
            message.extend_from_slice(key.as_ref());
        }
        message
    }
}

/// The Ed25519 precompile instruction that verifies `signature`, `signer`'s over `message`. It
/// holds one signature, with the key, the signature and the message in its own data, laid out as
/// `new_ed25519_instruction_with_signature` lays them out.
pub fn ed25519_instruction(signer: &Pubkey, signature: &[u8; 64], message: &[u8]) -> Instruction {
    // `u16::MAX` as an instruction index: the bytes are in this instruction's own data.
    const OWN_DATA: u16 = u16::MAX;
    let key_offset: u16 = 2 + 14;
    let signature_offset = key_offset + 32;
    let message_offset = signature_offset + 64;
    let mut data = vec![1, 0]; // one signature, then a padding byte
    for field in [
        signature_offset,
        OWN_DATA,
        key_offset,
        OWN_DATA,
        message_offset,
        message.len() as u16,
        OWN_DATA,
    ] {
        data.extend_from_slice(&field.to_le_bytes());
    }
    data.extend_from_slice(signer.as_ref());
    data.extend_from_slice(signature);
    data.extend_from_slice(message);
    Instruction {
        program_id: ED25519_PROGRAM_ID,
        accounts: vec![],
        data,
    }
}

pub struct SignedQuoteAccounts {
    pub taker: Pubkey,
    pub maker: Pubkey,
    /// Pays, in the quote mint.
    pub taker_quote_account: Pubkey,
    /// Is paid, in the quote mint: the maker's own account.
    pub maker_quote_account: Pubkey,
    /// Delivers, in the base mint.
    pub maker_base_account: Pubkey,
    /// Receives, in the base mint.
    pub taker_base_account: Pubkey,
}

/// The transaction's two instructions, in order: the Ed25519 instruction carrying `signature`,
/// the maker's over `quote`, and the run, which reads the signature from the instruction directly
/// before it. The taker and the maker both sign the transaction. `amount` is in base-token base
/// units, up to the quote's `max_amount`.
pub fn run_signed_quote(
    template: Pubkey,
    a: &SignedQuoteAccounts,
    quote: &Quote,
    signature: &[u8; 64],
    amount: u64,
) -> Result<[Instruction; 2], Box<dyn Error>> {
    let verify = ed25519_instruction(&a.maker, signature, &quote.message());
    let run = templates::signed_quote_settlement()
        .compile()?
        .run(template)
        .input("amount", amount)
        .account("instructions", INSTRUCTIONS_SYSVAR_ID)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("taker", a.taker)
        .account("maker", a.maker)
        .account("takerQuoteAccount", a.taker_quote_account)
        .account("makerQuoteAccount", a.maker_quote_account)
        .account("makerBaseAccount", a.maker_base_account)
        .account("takerBaseAccount", a.taker_base_account)
        .instruction()?;
    Ok([verify, run])
}
// #endregion signed-quote

// #region pump-accounts
/// pump.fun's `Global`, `["global"]`.
pub const PUMP_GLOBAL: Pubkey = pubkey!("4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf");
/// Anchor's event authority, `["__event_authority"]`: pump.fun logs each trade by calling itself.
pub const PUMP_EVENT_AUTHORITY: Pubkey = pubkey!("Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1");
/// `["global_volume_accumulator"]`.
pub const PUMP_GLOBAL_VOLUME_ACCUMULATOR: Pubkey =
    pubkey!("Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y");
/// Pump Fees' `["fee_config", pump.fun]`.
pub const PUMP_FEE_CONFIG: Pubkey = pubkey!("8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt");
/// `Global.fee_recipient`, which an ordinary coin's trades may pay. `Global.fee_recipients` lists
/// seven more.
pub const PUMP_FEE_RECIPIENT: Pubkey = pubkey!("62qc2CNXwrYqQScmEdiZFFAnJR262PxWEuNQtxfafNgV");
/// `Global.reserved_fee_recipient`, which a mayhem-mode coin's trades may pay instead.
pub const PUMP_RESERVED_FEE_RECIPIENT: Pubkey =
    pubkey!("GesfTA3X2arioaHp8bbKdjG9vJtskViWACZoYvxp4twS");
/// One of `Global.buyback_fee_recipients`, which every trade pays.
pub const PUMP_BUYBACK_FEE_RECIPIENT: Pubkey =
    pubkey!("9M4giFFMxmFGXtc3feFzRai56WbBqehoSeRE5GK7gf7");

/// A pump.fun coin, as its bonding curve describes it.
pub struct PumpCoin {
    pub mint: Pubkey,
    /// `BondingCurve.creator`: the 32 bytes at offset 49 of the curve's data.
    pub creator: Pubkey,
    /// `BondingCurve.is_mayhem_mode`: the byte at offset 81.
    pub mayhem: bool,
}

fn pump_pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &PUMP_FUN).0
}

/// `owner`'s associated token account for the Token-2022 mint `mint`.
pub fn token_2022_ata(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            owner.as_ref(),
            TOKEN_2022_PROGRAM_ID.as_ref(),
            mint.as_ref(),
        ],
        &ASSOCIATED_TOKEN_PROGRAM_ID,
    )
    .0
}

/// `["user_volume_accumulator", user]`. pump.fun creates it on the user's first buy.
pub fn pump_user_volume_accumulator(user: &Pubkey) -> Pubkey {
    pump_pda(&[b"user_volume_accumulator", user.as_ref()])
}

impl PumpCoin {
    /// `["bonding-curve", mint]`.
    pub fn bonding_curve(&self) -> Pubkey {
        pump_pda(&[b"bonding-curve", self.mint.as_ref()])
    }

    /// The curve's own token account, which coins are bought from and sold into.
    pub fn curve_token_account(&self) -> Pubkey {
        token_2022_ata(&self.bonding_curve(), &self.mint)
    }

    /// `["creator-vault", creator]`: the creator's fees accrue here.
    pub fn creator_vault(&self) -> Pubkey {
        pump_pda(&[b"creator-vault", self.creator.as_ref()])
    }

    /// `["bonding-curve-v2", mint]`, which `buy` and `sell` take since pump.fun's April 2026
    /// upgrade, whether or not it exists.
    pub fn bonding_curve_v2(&self) -> Pubkey {
        pump_pda(&[b"bonding-curve-v2", self.mint.as_ref()])
    }

    /// A fee recipient of the kind the coin's mode needs.
    pub fn fee_recipient(&self) -> Pubkey {
        if self.mayhem {
            PUMP_RESERVED_FEE_RECIPIENT
        } else {
            PUMP_FEE_RECIPIENT
        }
    }
}
// #endregion pump-accounts

// #region pump-buy-basket
/// One coin of the basket.
pub struct PumpBuy {
    pub coin: PumpCoin,
    /// Base units to buy.
    pub amount: u64,
    /// The most this buy may cost, in lamports, fees included.
    pub max_sol_cost: u64,
}

/// 1 to 7 rows, one per coin, bought into the buyer's own associated token accounts, which must
/// exist. `budget` caps what the whole basket takes from the buyer. Put a compute-budget
/// instruction first: a buy costs pump.fun about 75,000 compute units. Without a lookup table, two
/// coins fit a transaction; see the page for more.
pub fn run_pump_buy_basket(
    template: Pubkey,
    buyer: Pubkey,
    buys: &[PumpBuy],
    budget: u64,
) -> Result<Instruction, Box<dyn Error>> {
    let instruction = templates::pump_fun_buy_basket()
        .compile()?
        .run(template)
        .input("budget", budget)
        .account("pumpProgram", PUMP_FUN)
        .account("global", PUMP_GLOBAL)
        .account("buyer", buyer)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("tokenProgram", TOKEN_2022_PROGRAM_ID)
        .account("eventAuthority", PUMP_EVENT_AUTHORITY)
        .account("globalVolumeAccumulator", PUMP_GLOBAL_VOLUME_ACCUMULATOR)
        .account(
            "userVolumeAccumulator",
            pump_user_volume_accumulator(&buyer),
        )
        .account("feeConfig", PUMP_FEE_CONFIG)
        .account("feeProgram", PUMP_FEES)
        .account("buybackFeeRecipient", PUMP_BUYBACK_FEE_RECIPIENT)
        .rows(buys.iter().map(|buy| {
            let coin = &buy.coin;
            Row::new()
                .account("mint", coin.mint)
                .account("bondingCurve", coin.bonding_curve())
                .account("curveTokenAccount", coin.curve_token_account())
                .account("buyerTokenAccount", token_2022_ata(&buyer, &coin.mint))
                .account("creatorVault", coin.creator_vault())
                .account("bondingCurveV2", coin.bonding_curve_v2())
                .account("feeRecipient", coin.fee_recipient())
                .input("amount", buy.amount)
                .input("maxSolCost", buy.max_sol_cost)
        }))
        .instruction()?;
    Ok(instruction)
}
// #endregion pump-buy-basket

// #region pump-sell-all
/// Sells everything in the seller's associated token account for `coin`, and fails unless the
/// seller receives at least `min_sol_out` lamports after fees.
pub fn run_pump_sell_all(
    template: Pubkey,
    seller: Pubkey,
    coin: &PumpCoin,
    min_sol_out: u64,
) -> Result<Instruction, Box<dyn Error>> {
    let instruction = templates::pump_fun_sell_all()
        .compile()?
        .run(template)
        .input("minSolOut", min_sol_out)
        .account("pumpProgram", PUMP_FUN)
        .account("global", PUMP_GLOBAL)
        .account("feeRecipient", coin.fee_recipient())
        .account("mint", coin.mint)
        .account("bondingCurve", coin.bonding_curve())
        .account("curveTokenAccount", coin.curve_token_account())
        .account("sellerTokenAccount", token_2022_ata(&seller, &coin.mint))
        .account("seller", seller)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("creatorVault", coin.creator_vault())
        .account("tokenProgram", TOKEN_2022_PROGRAM_ID)
        .account("eventAuthority", PUMP_EVENT_AUTHORITY)
        .account("feeConfig", PUMP_FEE_CONFIG)
        .account("feeProgram", PUMP_FEES)
        .account("bondingCurveV2", coin.bonding_curve_v2())
        .account("buybackFeeRecipient", PUMP_BUYBACK_FEE_RECIPIENT)
        .instruction()?;
    Ok(instruction)
}
// #endregion pump-sell-all

// ------------------------------------------------------------- sample runs

/// A stand-in template address for the sample runs.
pub const TEMPLATE: Pubkey = pubkey!("Temp1ate11111111111111111111111111111111111");

/// SOL/USD's Pyth feed id.
const SOL_USD_FEED_ID: [u8; 32] = [
    0xef, 0x0d, 0x8b, 0x6f, 0xda, 0x2c, 0xeb, 0xa4, 0x1d, 0xa1, 0x5d, 0x40, 0x95, 0xd1, 0xda, 0x39,
    0x2a, 0x0d, 0x2f, 0x8e, 0xd0, 0xc6, 0xc7, 0xbc, 0x0f, 0x4c, 0xfa, 0xc8, 0xc2, 0x80, 0xb5, 0x6d,
];

fn key(seed: u8) -> Pubkey {
    Pubkey::new_from_array([seed; 32])
}

fn group(count: u8) -> Vec<AccountMeta> {
    (0..count)
        .map(|index| AccountMeta::new(key(200 + index), false))
        .collect()
}

/// Jupiter `route` data with a 60-byte plan, quoted at 1,000,000 in for 990,000 out.
fn route_data() -> Vec<u8> {
    let mut data = anchor_discriminator("route").to_vec();
    data.extend_from_slice(&[7; 60]);
    data.extend_from_slice(&1_000_000u64.to_le_bytes());
    data.extend_from_slice(&990_000u64.to_le_bytes());
    data.extend_from_slice(&50u16.to_le_bytes());
    data.push(0);
    data
}

/// A sample transaction for every template, under the name `fixtures/protocol-examples.json`
/// records it by: its instructions, the run last.
pub type SampleRun = (&'static str, fn() -> Vec<Instruction>);

pub const RUNS: [SampleRun; 12] = [
    ("jupiterDailyCapSwap", || {
        let data = route_data();
        let route = RouteQuote::split(&data);
        let a = DailyCapAccounts {
            actor: key(1),
            source_ata: key(2),
        };
        vec![run_jupiter_daily_cap(TEMPLATE, &a, &route, group(20)).unwrap()]
    }),
    ("jupiterDepositExactOutput", || {
        let a = JupiterDepositAccounts {
            owner: key(1),
            source_ata: key(2),
            destination_ata: key(3),
            obligation: key(4),
            lending_market: key(5),
            lending_market_authority: key(6),
            reserve: key(7),
            reserve_liquidity_mint: key(8),
            reserve_liquidity_supply: key(9),
            reserve_collateral_mint: key(10),
            reserve_destination_deposit_collateral: key(11),
            collateral_farm: Some((key(12), key(13))),
        };
        let mut transaction = kamino_refreshes(
            a.lending_market,
            a.obligation,
            &[(a.reserve, key(14))],
            &[],
            &[],
        );
        let data = route_data();
        let route = RouteQuote::split(&data);
        transaction.push(run_jupiter_deposit(TEMPLATE, &a, &route, 1_000_000, group(20)).unwrap());
        transaction
    }),
    ("jupiterOracleCheckedSwap", || {
        let a = OracleSwapAccounts {
            price_update: key(1),
            trader: key(2),
            source_ata: key(3),
            destination_ata: key(4),
        };
        let data = route_data();
        let route = RouteQuote::split(&data);
        vec![run_jupiter_oracle_swap(TEMPLATE, &a, &route, group(20)).unwrap()]
    }),
    ("kaminoLiquidateWithProof", || {
        let a = KaminoLiquidateAccounts {
            liquidator: key(1),
            obligation: key(2),
            lending_market: key(3),
            lending_market_authority: key(4),
            repay_reserve: key(5),
            repay_reserve_liquidity_mint: key(6),
            repay_reserve_liquidity_supply: key(7),
            withdraw_reserve: key(8),
            withdraw_reserve_liquidity_mint: key(9),
            withdraw_reserve_collateral_mint: key(10),
            withdraw_reserve_collateral_supply: key(11),
            withdraw_reserve_liquidity_supply: key(12),
            withdraw_reserve_fee_receiver: key(13),
            user_source_liquidity: key(14),
            user_destination_collateral: key(15),
            user_destination_liquidity: key(16),
            collateral_farm: Some((key(17), key(18))),
            debt_farm: None,
        };
        // The obligation holds the collateral and the debt.
        let held = [(a.withdraw_reserve, key(19)), (a.repay_reserve, key(19))];
        let mut transaction = kamino_refreshes(a.lending_market, a.obligation, &held, &[], &[]);
        transaction
            .push(run_kamino_liquidate(TEMPLATE, &a, 5_000_000, 4_000_000, 100_000).unwrap());
        transaction
    }),
    ("kaminoRepaySwapOutput", || {
        let a = KaminoRepayAccounts {
            borrower: key(1),
            collateral_ata: key(2),
            borrowed_asset_ata: key(3),
            obligation: key(4),
            lending_market: key(5),
            lending_market_authority: key(6),
            repay_reserve: key(7),
            reserve_liquidity_mint: key(8),
            reserve_liquidity_supply: key(9),
            debt_farm: None,
        };
        let held = [(key(10), key(11)), (a.repay_reserve, key(11))];
        let mut transaction = kamino_refreshes(a.lending_market, a.obligation, &held, &[], &[]);
        let data = route_data();
        let route = RouteQuote::split(&data);
        transaction.push(run_kamino_repay(TEMPLATE, &a, &route, 1_000_000, group(20)).unwrap());
        transaction
    }),
    ("orcaCompoundFees", || {
        let a = OrcaCompoundAccounts {
            position_authority: key(1),
            whirlpool: key(2),
            position: key(3),
            position_token_account: key(4),
            token_mint_a: key(5),
            token_mint_b: key(6),
            token_owner_account_a: key(7),
            token_owner_account_b: key(8),
            token_vault_a: key(9),
            token_vault_b: key(10),
            tick_array_lower: key(11),
            tick_array_upper: key(12),
        };
        let sqrt_price_bounds = (6_400_000_000_000_000_000, 6_550_000_000_000_000_000);
        vec![run_orca_compound(TEMPLATE, &a, 5_000, sqrt_price_bounds).unwrap()]
    }),
    ("orcaHarvestManyPositions", || {
        let a = OrcaHarvestAccounts {
            position_authority: key(1),
            whirlpool: key(2),
            token_owner_account_a: key(3),
            token_owner_account_b: key(4),
            token_vault_a: key(5),
            token_vault_b: key(6),
        };
        // Positions in one pool often share tick arrays.
        let rows: Vec<_> = (0..5)
            .map(|row| OrcaHarvestRow {
                position: key(100 + row),
                position_token_account: key(150 + row),
                tick_array_lower: key(7),
                tick_array_upper: key(8),
            })
            .collect();
        vec![run_orca_harvest(TEMPLATE, &a, &rows, 10_000).unwrap()]
    }),
    ("pumpFunBuyBasket", || {
        // Two ordinary coins and one in mayhem mode.
        let buys: Vec<_> = (0..3)
            .map(|row| PumpBuy {
                coin: PumpCoin {
                    mint: key(30 + row),
                    creator: key(40 + row),
                    mayhem: row == 2,
                },
                amount: 1_000_000_000_000,
                max_sol_cost: 100_000_000,
            })
            .collect();
        vec![run_pump_buy_basket(TEMPLATE, key(1), &buys, 250_000_000).unwrap()]
    }),
    ("pumpFunSellAll", || {
        let coin = PumpCoin {
            mint: key(30),
            creator: key(40),
            mayhem: false,
        };
        vec![run_pump_sell_all(TEMPLATE, key(1), &coin, 50_000_000).unwrap()]
    }),
    ("pythFreshPriceGate", || {
        let gate = PriceGate {
            price_update: key(1),
            feed_id: SOL_USD_FEED_ID,
            exponent: -8,
            actor: key(2),
        };
        // A minute old at most, confident to $0.10, between $100 and $150.
        let band = (100_00000000, 150_00000000);
        let data = route_data();
        let route = RouteQuote::split(&data);
        vec![run_pyth_gate(TEMPLATE, &gate, 60, 10_000_000, band, &route, group(14)).unwrap()]
    }),
    ("signedQuoteSettlement", || {
        let a = SignedQuoteAccounts {
            taker: key(1),
            maker: key(2),
            taker_quote_account: key(3),
            maker_quote_account: key(4),
            maker_base_account: key(5),
            taker_base_account: key(6),
        };
        // 2.5 quote units per base unit, at most 4 base units, for a minute.
        let quote = Quote {
            price: 2_500_000,
            max_amount: 4_000_000,
            expiry: 1_800_000_060,
            taker: a.taker,
            base_mint: key(7),
            quote_mint: key(8),
        };
        // A stand-in for the signature the maker sends with the quote.
        let signature = [9; 64];
        run_signed_quote(TEMPLATE, &a, &quote, &signature, 3_000_000)
            .unwrap()
            .to_vec()
    }),
    ("tokenSweepIntoSwap", || {
        let a = TokenSweepAccounts {
            seller: key(1),
            source_ata: key(2),
            destination_ata: key(3),
        };
        let data = route_data();
        let quote = RouteQuote::split(&data);
        vec![run_token_sweep(TEMPLATE, &a, &quote, 1_000, group(20)).unwrap()]
    }),
];

#[allow(dead_code)]
fn main() {
    for (name, transaction) in RUNS {
        let transaction = transaction();
        let run = transaction.last().expect("the run is last");
        println!(
            "{name:<30} {:>3} accounts  {:>4} data bytes  after {} other instructions",
            run.accounts.len(),
            run.data.len(),
            transaction.len() - 1,
        );
    }
}
