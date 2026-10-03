//! Build a run of each live-protocol template from Rust.
//!
//! A run needs the template's address, one account per declared account in declaration order,
//! and the inputs in declaration order. A template that declares account groups takes one length
//! byte per group first, and each group's members after the declared accounts, group after group.
//! `tests/protocol_templates.rs` checks each run below against the template it runs.
//!
//! No template refreshes Kamino: a run that deposits into, repays or liquidates an obligation goes
//! behind [`kamino_refreshes`], in the same transaction.
//!
//! ```bash
//! cargo run -p ballista-sdk --example protocol_templates_run
//! ```

use ballista_sdk::{
    find_registry_entry_address, run_instruction, RunInputs, ED25519_PROGRAM_ID, SYSTEM_PROGRAM_ID,
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
const MARGINFI_V2: Pubkey = pubkey!("MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA");
const SYSVAR_INSTRUCTIONS: Pubkey = pubkey!("Sysvar1nstructions1111111111111111111111111");
const MEMO_PROGRAM: Pubkey = pubkey!("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
const WRAPPED_SOL_MINT: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
const USDC_MINT: Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

/// `in_amount`, `quoted_out_amount`, `slippage_bps` and `platform_fee_bps`: `route`'s data after
/// its plan.
const ROUTE_TAIL_LEN: usize = 8 + 8 + 2 + 1;

/// An account the template pins, such as a program: read-only, at exactly this address.
fn pinned(address: Pubkey) -> AccountMeta {
    AccountMeta::new_readonly(address, false)
}

/// An Anchor instruction discriminator: the first eight bytes of `sha256("global:<handler>")`.
fn anchor(handler: &str) -> Vec<u8> {
    solana_sha256_hasher::hash(format!("global:{handler}").as_bytes()).to_bytes()[..8].to_vec()
}

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
            route_data.len() >= 8 + 4 + ROUTE_TAIL_LEN && route_data[..8] == anchor("route")[..],
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
                pinned(KAMINO_LEND), // Pyth
                pinned(KAMINO_LEND), // Switchboard price
                pinned(KAMINO_LEND), // Switchboard TWAP
                AccountMeta::new_readonly(scope_prices, false),
            ],
            data: anchor("refresh_reserve"),
        });
    }
    let mut accounts = vec![
        AccountMeta::new_readonly(lending_market, false),
        AccountMeta::new(obligation, false),
    ];
    accounts.extend(held.iter().map(|&(reserve, _)| AccountMeta::new(reserve, false)));
    accounts.extend(referrer_token_states.iter().map(|&state| AccountMeta::new(state, false)));
    instructions.push(Instruction {
        program_id: KAMINO_LEND,
        accounts,
        data: anchor("refresh_obligation"),
    });
    instructions
}

/// One farm in a Kamino v2 account tail: `(the obligation's user state in the farm, the farm)`,
/// both writable. When the reserve has no such farm, Kamino reads its own program ID, twice and
/// read-only, as "none".
pub fn kamino_farm_pair(farm: Option<(Pubkey, Pubkey)>) -> [AccountMeta; 2] {
    match farm {
        Some((user_state, farm_state)) => {
            [AccountMeta::new(user_state, false), AccountMeta::new(farm_state, false)]
        }
        None => [pinned(KAMINO_LEND), pinned(KAMINO_LEND)],
    }
}
// #endregion kamino-refreshes

// #region marginfi-health
/// marginfi's `healthAccounts`: for every balance the account still holds after the withdrawal,
/// `(bank, oracle)`, passed read-only, bank then oracle, by bank address from highest to lowest.
/// Empty when the withdrawn balance was the only one. A bank priced by more than one account
/// (staked, Kamino) takes more than this passes.
pub fn marginfi_health_accounts(remaining_balances: &[(Pubkey, Pubkey)]) -> Vec<AccountMeta> {
    let mut balances = remaining_balances.to_vec();
    balances.sort_by(|left, right| right.0.cmp(&left.0));
    balances
        .iter()
        .flat_map(|&(bank, oracle)| {
            [AccountMeta::new_readonly(bank, false), AccountMeta::new_readonly(oracle, false)]
        })
        .collect()
}
// #endregion marginfi-health

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
) -> Instruction {
    // Kamino's v2 deposit ends in the farm pair and the Farms program.
    let mut farm_accounts = kamino_farm_pair(a.collateral_farm).to_vec();
    farm_accounts.push(pinned(KAMINO_FARMS));
    let inputs = RunInputs::new()
        // One length per group, in declaration order: routeAccounts, farmAccounts.
        .groups(&[route_accounts.len() as u8, farm_accounts.len() as u8])
        .bytes(route.route_plan)
        .u64(route.in_amount)
        .u64(route.quoted_out_amount)
        .u64(route.slippage_bps.into())
        .u64(route.platform_fee_bps.into())
        .u64(minimum_out)
        .finish();
    let mut accounts = vec![
        pinned(JUPITER_V6),
        pinned(KAMINO_LEND),
        pinned(TOKEN_PROGRAM_ID),
        pinned(SYSVAR_INSTRUCTIONS),
        AccountMeta::new(a.owner, true),
        AccountMeta::new(a.source_ata, false),
        AccountMeta::new(a.destination_ata, false),
        AccountMeta::new(a.obligation, false),
        AccountMeta::new_readonly(a.lending_market, false),
        AccountMeta::new_readonly(a.lending_market_authority, false),
        AccountMeta::new(a.reserve, false),
        AccountMeta::new_readonly(a.reserve_liquidity_mint, false),
        AccountMeta::new(a.reserve_liquidity_supply, false),
        AccountMeta::new(a.reserve_collateral_mint, false),
        AccountMeta::new(a.reserve_destination_deposit_collateral, false),
    ];
    accounts.extend(route_accounts);
    accounts.extend(farm_accounts);
    run_instruction(template, accounts, &inputs)
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
) -> Instruction {
    let inputs = RunInputs::new()
        .groups(&[route_accounts.len() as u8]) // routeAccounts
        .bytes(route.route_plan)
        .u64(route.in_amount)
        .u64(route.quoted_out_amount)
        .u64(route.slippage_bps.into())
        .u64(route.platform_fee_bps.into())
        .finish();
    let mut accounts = vec![
        pinned(JUPITER_V6),
        pinned(TOKEN_PROGRAM_ID),
        AccountMeta::new_readonly(a.price_update, false),
        AccountMeta::new(a.trader, true),
        AccountMeta::new(a.source_ata, false),
        AccountMeta::new(a.destination_ata, false),
        pinned(WRAPPED_SOL_MINT),
        pinned(USDC_MINT),
    ];
    accounts.extend(route_accounts);
    run_instruction(template, accounts, &inputs)
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
) -> Instruction {
    let inputs = RunInputs::new()
        .groups(&[route_accounts.len() as u8]) // routeAccounts
        .bytes(quote.route_plan)
        .u64(quote.in_amount)
        .u64(quote.quoted_out_amount)
        .u64(quote.slippage_bps.into())
        .u64(quote.platform_fee_bps.into())
        .u64(dust_floor)
        .finish();
    let mut accounts = vec![
        pinned(JUPITER_V6),
        pinned(TOKEN_PROGRAM_ID),
        AccountMeta::new(a.seller, true),
        AccountMeta::new(a.source_ata, false),
        AccountMeta::new(a.destination_ata, false),
    ];
    accounts.extend(route_accounts);
    run_instruction(template, accounts, &inputs)
}
// #endregion token-sweep

// #region jito-tip
pub struct JitoTipAccounts {
    pub searcher: Pubkey,
    /// The searcher's wrapped-SOL token account: the round trip's source and its destination.
    pub wsol_account: Pubkey,
    /// One of Jito's eight tip accounts.
    pub jito_tip: Pubkey,
}

/// The strategy is one Jupiter `route` from the searcher's wrapped SOL back to it. The Swap API
/// quotes the two legs separately, so they are joined: the second leg's step goes after the
/// first's with its token indices moved up by one, and the joined list is the second leg's fixed
/// accounts, then both legs' step accounts. `round_trip` in `tests/protocols/tests/jito_tip.rs`
/// joins single-step legs, as `joinRoundTrip` does in TypeScript.
///
/// `route` is the joined data split by [`RouteQuote::split`], and `strategy_accounts` its account
/// list from the fifth account on: the template passes the token program, the searcher, and the
/// wrapped-SOL account twice.
pub fn run_jito_tip(
    template: Pubkey,
    a: &JitoTipAccounts,
    route: &RouteQuote,
    tip_lamports: u64,
    minimum_edge: u64,
    strategy_accounts: Vec<AccountMeta>,
) -> Instruction {
    let inputs = RunInputs::new()
        .groups(&[strategy_accounts.len() as u8]) // strategyAccounts
        .bytes(route.route_plan)
        .u64(route.in_amount)
        .u64(route.quoted_out_amount)
        .u64(route.slippage_bps.into())
        .u64(route.platform_fee_bps.into())
        .u64(tip_lamports)
        .u64(minimum_edge)
        .finish();
    let mut accounts = vec![
        pinned(SYSTEM_PROGRAM_ID),
        pinned(JUPITER_V6),
        pinned(TOKEN_PROGRAM_ID),
        AccountMeta::new(a.searcher, true),
        AccountMeta::new(a.wsol_account, false),
        AccountMeta::new(a.jito_tip, false),
    ];
    accounts.extend(strategy_accounts);
    run_instruction(template, accounts, &inputs)
}
// #endregion jito-tip

// #region pyth-gate
pub struct PriceGate {
    /// The Pyth `PriceUpdateV2` account.
    pub price_update: Pubkey,
    /// The feed the price must be, as 32 bytes: SOL/USD's is `ef0d8b6f…c280b56d`.
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
) -> Instruction {
    let inputs = RunInputs::new()
        .groups(&[action_accounts.len() as u8]) // actionAccounts
        .pubkey(&Pubkey::new_from_array(gate.feed_id))
        // Pyth stores the exponent as an i32; the template takes it as an i64.
        .i64(gate.exponent.into())
        .i64(maximum_age)
        .u64(maximum_confidence)
        .i64(floor_price)
        .i64(ceiling_price)
        .bytes(route.route_plan)
        .u64(route.in_amount)
        .u64(route.quoted_out_amount)
        .u64(route.slippage_bps.into())
        .u64(route.platform_fee_bps.into())
        .finish();
    let mut accounts = vec![
        AccountMeta::new_readonly(gate.price_update, false),
        pinned(JUPITER_V6),
        pinned(TOKEN_PROGRAM_ID),
        AccountMeta::new(gate.actor, true),
    ];
    accounts.extend(action_accounts);
    run_instruction(template, accounts, &inputs)
}
// #endregion pyth-gate

// #region jupiter-daily-cap
/// `jupiterDailyCapSwap`'s one registry, `dailySpend`: the first it declares.
const DAILY_SPEND: u8 = 0;

pub struct DailyCapAccounts {
    /// Signs, keys the entry, and pays its rent on the first run.
    pub actor: Pubkey,
    /// The actor's wrapped-SOL token account, which the route sells from.
    pub source_ata: Pubkey,
}

/// `route` is the Swap API's `route` data split by [`RouteQuote::split`], and `action_accounts`
/// its account list from the fourth account on: the template passes the token program, the actor
/// and the source itself. The actor's entry is at its registry address for the actor's own key;
/// the actor's first run creates it, and pays its rent.
pub fn run_jupiter_daily_cap(
    template: Pubkey,
    a: &DailyCapAccounts,
    route: &RouteQuote,
    action_accounts: Vec<AccountMeta>,
) -> Instruction {
    let inputs = RunInputs::new()
        .groups(&[action_accounts.len() as u8]) // actionAccounts
        .bytes(route.route_plan)
        .u64(route.in_amount)
        .u64(route.quoted_out_amount)
        .u64(route.slippage_bps.into())
        .u64(route.platform_fee_bps.into())
        .finish();
    let (spend, _) = find_registry_entry_address(&template, DAILY_SPEND, &a.actor.to_bytes());
    let mut accounts = vec![
        pinned(JUPITER_V6),
        pinned(TOKEN_PROGRAM_ID),
        AccountMeta::new(a.actor, true),
        AccountMeta::new(a.source_ata, false),
        AccountMeta::new(spend, false),
        pinned(SYSTEM_PROGRAM_ID),
    ];
    accounts.extend(action_accounts);
    run_instruction(template, accounts, &inputs)
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
) -> Instruction {
    let inputs = RunInputs::new()
        .u64(dust_floor)
        .u128(min_sqrt_price)
        .u128(max_sqrt_price)
        .finish();
    let accounts = vec![
        pinned(ORCA_WHIRLPOOL),
        pinned(TOKEN_PROGRAM_ID),
        pinned(MEMO_PROGRAM),
        AccountMeta::new_readonly(a.position_authority, true),
        AccountMeta::new(a.whirlpool, false),
        AccountMeta::new(a.position, false),
        AccountMeta::new_readonly(a.position_token_account, false),
        AccountMeta::new_readonly(a.token_mint_a, false),
        AccountMeta::new_readonly(a.token_mint_b, false),
        AccountMeta::new(a.token_owner_account_a, false),
        AccountMeta::new(a.token_owner_account_b, false),
        AccountMeta::new(a.token_vault_a, false),
        AccountMeta::new(a.token_vault_b, false),
        AccountMeta::new(a.tick_array_lower, false),
        AccountMeta::new(a.tick_array_upper, false),
    ];
    run_instruction(template, accounts, &inputs)
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
) -> Instruction {
    assert!((1..=12).contains(&rows.len()), "the template takes 1 to 12 positions");
    let inputs = RunInputs::new().u64(dust_floor).finish();
    let mut accounts = vec![
        pinned(ORCA_WHIRLPOOL),
        pinned(TOKEN_PROGRAM_ID),
        AccountMeta::new_readonly(a.position_authority, true),
        AccountMeta::new(a.whirlpool, false),
        AccountMeta::new(a.token_owner_account_a, false),
        AccountMeta::new(a.token_owner_account_b, false),
        AccountMeta::new(a.token_vault_a, false),
        AccountMeta::new(a.token_vault_b, false),
    ];
    // One row after another, each in the order the row schema declares.
    for row in rows {
        accounts.push(AccountMeta::new(row.position, false));
        accounts.push(AccountMeta::new_readonly(row.position_token_account, false));
        accounts.push(AccountMeta::new_readonly(row.tick_array_lower, false));
        accounts.push(AccountMeta::new_readonly(row.tick_array_upper, false));
    }
    run_instruction(template, accounts, &inputs)
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
) -> Instruction {
    // Kamino's v2 repayment ends in the farm pair, the lending market authority and Farms.
    let mut farm_accounts = kamino_farm_pair(a.debt_farm).to_vec();
    farm_accounts.push(AccountMeta::new_readonly(a.lending_market_authority, false));
    farm_accounts.push(pinned(KAMINO_FARMS));
    let inputs = RunInputs::new()
        // One length per group, in declaration order: routeAccounts, farmAccounts.
        .groups(&[route_accounts.len() as u8, farm_accounts.len() as u8])
        .bytes(route.route_plan)
        .u64(route.in_amount)
        .u64(route.quoted_out_amount)
        .u64(route.slippage_bps.into())
        .u64(route.platform_fee_bps.into())
        .u64(minimum_repayment)
        .finish();
    let mut accounts = vec![
        pinned(JUPITER_V6),
        pinned(KAMINO_LEND),
        pinned(TOKEN_PROGRAM_ID),
        pinned(SYSVAR_INSTRUCTIONS),
        // Kamino declares the borrower a bare signer, so it is read-only.
        AccountMeta::new_readonly(a.borrower, true),
        AccountMeta::new(a.collateral_ata, false),
        AccountMeta::new(a.borrowed_asset_ata, false),
        AccountMeta::new(a.obligation, false),
        AccountMeta::new_readonly(a.lending_market, false),
        AccountMeta::new(a.repay_reserve, false),
        AccountMeta::new_readonly(a.reserve_liquidity_mint, false),
        AccountMeta::new(a.reserve_liquidity_supply, false),
    ];
    accounts.extend(route_accounts);
    accounts.extend(farm_accounts);
    run_instruction(template, accounts, &inputs)
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

/// `minimum_bounty` is in the seized collateral's own units (lamports for SOL collateral).
///
/// Send it behind [`kamino_refreshes`], in the same transaction.
pub fn run_kamino_liquidate(
    template: Pubkey,
    a: &KaminoLiquidateAccounts,
    liquidity_amount: u64,
    min_acceptable_received: u64,
    minimum_bounty: u64,
) -> Instruction {
    // Kamino's v2 liquidation ends in both farm pairs and Farms.
    let mut farm_accounts = kamino_farm_pair(a.collateral_farm).to_vec();
    farm_accounts.extend(kamino_farm_pair(a.debt_farm));
    farm_accounts.push(pinned(KAMINO_FARMS));
    let inputs = RunInputs::new()
        .groups(&[farm_accounts.len() as u8]) // farmAccounts
        .u64(liquidity_amount)
        .u64(min_acceptable_received)
        .u64(minimum_bounty)
        .finish();
    let mut accounts = vec![
        pinned(KAMINO_LEND),
        pinned(TOKEN_PROGRAM_ID),
        pinned(SYSVAR_INSTRUCTIONS),
        // Kamino declares the liquidator a bare signer, so it is read-only.
        AccountMeta::new_readonly(a.liquidator, true),
        AccountMeta::new(a.obligation, false),
        AccountMeta::new_readonly(a.lending_market, false),
        AccountMeta::new_readonly(a.lending_market_authority, false),
        AccountMeta::new(a.repay_reserve, false),
        AccountMeta::new_readonly(a.repay_reserve_liquidity_mint, false),
        AccountMeta::new(a.repay_reserve_liquidity_supply, false),
        AccountMeta::new(a.withdraw_reserve, false),
        AccountMeta::new_readonly(a.withdraw_reserve_liquidity_mint, false),
        AccountMeta::new(a.withdraw_reserve_collateral_mint, false),
        AccountMeta::new(a.withdraw_reserve_collateral_supply, false),
        AccountMeta::new(a.withdraw_reserve_liquidity_supply, false),
        AccountMeta::new(a.withdraw_reserve_fee_receiver, false),
        AccountMeta::new(a.user_source_liquidity, false),
        AccountMeta::new(a.user_destination_collateral, false),
        AccountMeta::new(a.user_destination_liquidity, false),
    ];
    accounts.extend(farm_accounts);
    run_instruction(template, accounts, &inputs)
}
// #endregion kamino-liquidate

// #region marginfi-withdraw
pub struct MarginfiWithdrawAccounts {
    pub marginfi_group: Pubkey,
    pub marginfi_account: Pubkey,
    pub authority: Pubkey,
    pub bank: Pubkey,
    pub bank_liquidity_vault: Pubkey,
    /// A PDA marginfi signs for, so it is passed read-only.
    pub bank_liquidity_vault_authority: Pubkey,
    /// Where marginfi pays the withdrawal, and the treasury it is swept to: both the authority's
    /// own token accounts.
    pub destination_ata: Pubkey,
    pub treasury_ata: Pubkey,
}

/// `remaining_balances` is `(bank, oracle)` for every balance the marginfi account still holds
/// after this one is emptied; see [`marginfi_health_accounts`].
pub fn run_marginfi_withdraw(
    template: Pubkey,
    a: &MarginfiWithdrawAccounts,
    minimum_withdrawn: u64,
    remaining_balances: &[(Pubkey, Pubkey)],
) -> Instruction {
    let health_accounts = marginfi_health_accounts(remaining_balances);
    let inputs = RunInputs::new()
        .groups(&[health_accounts.len() as u8]) // healthAccounts
        .u64(minimum_withdrawn)
        .finish();
    let mut accounts = vec![
        pinned(MARGINFI_V2),
        pinned(TOKEN_PROGRAM_ID),
        AccountMeta::new_readonly(a.marginfi_group, false),
        AccountMeta::new(a.marginfi_account, false),
        AccountMeta::new_readonly(a.authority, true),
        AccountMeta::new(a.bank, false),
        AccountMeta::new(a.bank_liquidity_vault, false),
        AccountMeta::new_readonly(a.bank_liquidity_vault_authority, false),
        AccountMeta::new(a.destination_ata, false),
        AccountMeta::new(a.treasury_ata, false),
    ];
    accounts.extend(health_accounts);
    run_instruction(template, accounts, &inputs)
}
// #endregion marginfi-withdraw

// #region marginfi-to-kamino
pub struct MarginfiToKaminoAccounts {
    pub owner: Pubkey,
    /// The owner's token account the assets pass through.
    pub wallet_ata: Pubkey,
    pub marginfi_group: Pubkey,
    pub marginfi_account: Pubkey,
    pub marginfi_bank: Pubkey,
    pub marginfi_vault: Pubkey,
    pub marginfi_vault_authority: Pubkey,
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

/// `remaining_balances` is `(bank, oracle)` for every balance the marginfi account still holds
/// after this one is emptied; see [`marginfi_health_accounts`].
///
/// Send it behind [`kamino_refreshes`], in the same transaction.
pub fn run_marginfi_to_kamino(
    template: Pubkey,
    a: &MarginfiToKaminoAccounts,
    minimum_moved: u64,
    remaining_balances: &[(Pubkey, Pubkey)],
) -> Instruction {
    let health_accounts = marginfi_health_accounts(remaining_balances);
    let mut farm_accounts = kamino_farm_pair(a.collateral_farm).to_vec();
    farm_accounts.push(pinned(KAMINO_FARMS));
    let inputs = RunInputs::new()
        // One length per group, in declaration order: healthAccounts, farmAccounts.
        .groups(&[health_accounts.len() as u8, farm_accounts.len() as u8])
        .u64(minimum_moved)
        .finish();
    let mut accounts = vec![
        pinned(MARGINFI_V2),
        pinned(KAMINO_LEND),
        pinned(TOKEN_PROGRAM_ID),
        pinned(SYSVAR_INSTRUCTIONS),
        AccountMeta::new(a.owner, true),
        AccountMeta::new(a.wallet_ata, false),
        AccountMeta::new_readonly(a.marginfi_group, false),
        AccountMeta::new(a.marginfi_account, false),
        AccountMeta::new(a.marginfi_bank, false),
        AccountMeta::new(a.marginfi_vault, false),
        AccountMeta::new_readonly(a.marginfi_vault_authority, false),
        AccountMeta::new(a.obligation, false),
        AccountMeta::new_readonly(a.lending_market, false),
        AccountMeta::new_readonly(a.lending_market_authority, false),
        AccountMeta::new(a.reserve, false),
        AccountMeta::new_readonly(a.reserve_liquidity_mint, false),
        AccountMeta::new(a.reserve_liquidity_supply, false),
        AccountMeta::new(a.reserve_collateral_mint, false),
        AccountMeta::new(a.reserve_destination_deposit_collateral, false),
    ];
    accounts.extend(health_accounts);
    accounts.extend(farm_accounts);
    run_instruction(template, accounts, &inputs)
}
// #endregion marginfi-to-kamino

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
) -> [Instruction; 2] {
    let verify = ed25519_instruction(&a.maker, signature, &quote.message());
    let inputs = RunInputs::new().u64(amount).finish();
    let accounts = vec![
        pinned(SYSVAR_INSTRUCTIONS),
        pinned(TOKEN_PROGRAM_ID),
        AccountMeta::new_readonly(a.taker, true),
        AccountMeta::new_readonly(a.maker, true),
        AccountMeta::new(a.taker_quote_account, false),
        AccountMeta::new(a.maker_quote_account, false),
        AccountMeta::new(a.maker_base_account, false),
        AccountMeta::new(a.taker_base_account, false),
    ];
    [verify, run_instruction(template, accounts, &inputs)]
}
// #endregion signed-quote

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
    (0..count).map(|index| AccountMeta::new(key(200 + index), false)).collect()
}

/// Jupiter `route` data with a 60-byte plan, quoted at 1,000,000 in for 990,000 out.
fn route_data() -> Vec<u8> {
    let mut data = anchor("route");
    data.extend_from_slice(&[7; 60]);
    data.extend_from_slice(&1_000_000u64.to_le_bytes());
    data.extend_from_slice(&990_000u64.to_le_bytes());
    data.extend_from_slice(&50u16.to_le_bytes());
    data.push(0);
    data
}

/// A sample transaction for every template, under the name `fixtures/protocol-examples.json`
/// records it by: its instructions, the run last.
pub const RUNS: [(&str, fn() -> Vec<Instruction>); 13] = [
    ("jitoProfitGuardedTip", || {
        let data = route_data();
        let route = RouteQuote::split(&data);
        let a = JitoTipAccounts { searcher: key(1), wsol_account: key(2), jito_tip: key(3) };
        vec![run_jito_tip(TEMPLATE, &a, &route, 10_000, 100_000, group(40))]
    }),
    ("jupiterDailyCapSwap", || {
        let data = route_data();
        let route = RouteQuote::split(&data);
        let a = DailyCapAccounts { actor: key(1), source_ata: key(2) };
        vec![run_jupiter_daily_cap(TEMPLATE, &a, &route, group(20))]
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
        let mut transaction =
            kamino_refreshes(a.lending_market, a.obligation, &[(a.reserve, key(14))], &[], &[]);
        let data = route_data();
        let route = RouteQuote::split(&data);
        transaction.push(run_jupiter_deposit(TEMPLATE, &a, &route, 1_000_000, group(20)));
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
        vec![run_jupiter_oracle_swap(TEMPLATE, &a, &route, group(20))]
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
        transaction.push(run_kamino_liquidate(TEMPLATE, &a, 5_000_000, 4_000_000, 100_000));
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
        transaction.push(run_kamino_repay(TEMPLATE, &a, &route, 1_000_000, group(20)));
        transaction
    }),
    ("marginfiToKaminoRebalance", || {
        let a = MarginfiToKaminoAccounts {
            owner: key(1),
            wallet_ata: key(2),
            marginfi_group: key(3),
            marginfi_account: key(4),
            marginfi_bank: key(5),
            marginfi_vault: key(6),
            marginfi_vault_authority: key(7),
            obligation: key(8),
            lending_market: key(9),
            lending_market_authority: key(10),
            reserve: key(11),
            reserve_liquidity_mint: key(12),
            reserve_liquidity_supply: key(13),
            reserve_collateral_mint: key(14),
            reserve_destination_deposit_collateral: key(15),
            collateral_farm: Some((key(16), key(17))),
        };
        let mut transaction =
            kamino_refreshes(a.lending_market, a.obligation, &[(a.reserve, key(18))], &[], &[]);
        transaction.push(run_marginfi_to_kamino(TEMPLATE, &a, 1_000_000, &[(key(19), key(20))]));
        transaction
    }),
    ("marginfiWithdrawAllWithFloor", || {
        let a = MarginfiWithdrawAccounts {
            marginfi_group: key(1),
            marginfi_account: key(2),
            authority: key(3),
            bank: key(4),
            bank_liquidity_vault: key(5),
            bank_liquidity_vault_authority: key(6),
            destination_ata: key(7),
            treasury_ata: key(8),
        };
        let remaining = [(key(9), key(10)), (key(11), key(12))];
        vec![run_marginfi_withdraw(TEMPLATE, &a, 1_000_000, &remaining)]
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
        vec![run_orca_compound(TEMPLATE, &a, 5_000, sqrt_price_bounds)]
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
        vec![run_orca_harvest(TEMPLATE, &a, &rows, 10_000)]
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
        vec![run_pyth_gate(TEMPLATE, &gate, 60, 10_000_000, band, &route, group(14))]
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
        run_signed_quote(TEMPLATE, &a, &quote, &signature, 3_000_000).to_vec()
    }),
    ("tokenSweepIntoSwap", || {
        let a = TokenSweepAccounts { seller: key(1), source_ata: key(2), destination_ata: key(3) };
        let data = route_data();
        let quote = RouteQuote::split(&data);
        vec![run_token_sweep(TEMPLATE, &a, &quote, 1_000, group(20))]
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
