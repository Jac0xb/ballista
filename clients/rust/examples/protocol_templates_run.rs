//! Build a run of each live-protocol template from Rust.
//!
//! A run needs the template's address, one account per declared account in declaration order,
//! and the inputs in declaration order. A template that declares an account group takes its
//! length first, and its members after the declared accounts. `tests/protocol_templates.rs`
//! checks each run below against the template it runs.
//!
//! ```bash
//! cargo run -p ballista-sdk --example protocol_templates_run
//! ```

use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey,
    pubkey::Pubkey,
};

const JUPITER_V6: Pubkey = pubkey!("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");
const KAMINO_LEND: Pubkey = pubkey!("KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD");
const ORCA_WHIRLPOOL: Pubkey = pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");
const MARGINFI_V2: Pubkey = pubkey!("MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA");
const DRIFT_V2: Pubkey = pubkey!("dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH");

fn program(address: Pubkey) -> AccountMeta {
    AccountMeta::new_readonly(address, false)
}

// #region jupiter-deposit
pub struct JupiterDepositAccounts {
    pub owner: Pubkey,
    pub source_ata: Pubkey,
    pub destination_ata: Pubkey,
    pub obligation: Pubkey,
    pub lending_market: Pubkey,
    pub lending_market_authority: Pubkey,
    pub reserve: Pubkey,
    pub reserve_liquidity_supply: Pubkey,
    pub reserve_collateral_mint: Pubkey,
    pub reserve_destination_deposit_collateral: Pubkey,
}

/// `route_args` is the Swap API's `route` data after its 8-byte discriminator, and
/// `route_accounts` its account list from the fifth account on: the template passes the first
/// four (token program, owner, source, destination) itself.
pub fn run_jupiter_deposit(
    template: Pubkey,
    a: &JupiterDepositAccounts,
    route_args: &[u8],
    minimum_out: u64,
    route_accounts: Vec<AccountMeta>,
) -> Instruction {
    let inputs = RunInputs::new()
        .groups(&[route_accounts.len() as u8]) // routeAccounts
        .bytes(route_args)
        .u64(minimum_out)
        .finish();
    let mut accounts = vec![
        program(JUPITER_V6),
        program(KAMINO_LEND),
        program(TOKEN_PROGRAM_ID),
        AccountMeta::new(a.owner, true),
        AccountMeta::new(a.source_ata, false),
        AccountMeta::new(a.destination_ata, false),
        AccountMeta::new(a.obligation, false),
        AccountMeta::new_readonly(a.lending_market, false),
        AccountMeta::new_readonly(a.lending_market_authority, false),
        AccountMeta::new(a.reserve, false),
        AccountMeta::new(a.reserve_liquidity_supply, false),
        AccountMeta::new(a.reserve_collateral_mint, false),
        AccountMeta::new(a.reserve_destination_deposit_collateral, false),
    ];
    accounts.extend(route_accounts);
    run_instruction(template, accounts, &inputs)
}
// #endregion jupiter-deposit

// #region jupiter-oracle-swap
pub struct OracleSwapAccounts {
    pub price_update: Pubkey,
    pub trader: Pubkey,
    pub source_ata: Pubkey,
    pub destination_ata: Pubkey,
}

/// `scale_divisor` is `10^(source decimals − destination decimals − price_exponent)`.
pub fn run_jupiter_oracle_swap(
    template: Pubkey,
    a: &OracleSwapAccounts,
    route_args: &[u8],
    price_exponent: i64,
    scale_divisor: u128,
    tolerance_bps: u64,
    route_accounts: Vec<AccountMeta>,
) -> Instruction {
    let inputs = RunInputs::new()
        .groups(&[route_accounts.len() as u8]) // routeAccounts
        .bytes(route_args)
        .i64(price_exponent)
        .u128(scale_divisor)
        .u64(tolerance_bps)
        .finish();
    let mut accounts = vec![
        program(JUPITER_V6),
        program(TOKEN_PROGRAM_ID),
        AccountMeta::new_readonly(a.price_update, false),
        AccountMeta::new(a.trader, true),
        AccountMeta::new(a.source_ata, false),
        AccountMeta::new(a.destination_ata, false),
    ];
    accounts.extend(route_accounts);
    run_instruction(template, accounts, &inputs)
}
// #endregion jupiter-oracle-swap

// #region token-sweep
pub struct TokenSweepAccounts {
    pub seller: Pubkey,
    pub source_ata: Pubkey,
    pub destination_ata: Pubkey,
}

/// The quote fields come from the Swap API's `route` data; the template sells the whole balance
/// and rescales `quoted_out_amount` to it.
pub struct RouteQuote<'a> {
    /// `route_plan`: the bytes between the discriminator and `in_amount`.
    pub route_plan: &'a [u8],
    pub in_amount: u64,
    pub quoted_out_amount: u64,
    pub slippage_bps: u64,
    pub platform_fee_bps: u64,
}

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
        .u64(quote.slippage_bps)
        .u64(quote.platform_fee_bps)
        .u64(dust_floor)
        .finish();
    let mut accounts = vec![
        program(JUPITER_V6),
        program(TOKEN_PROGRAM_ID),
        AccountMeta::new(a.seller, true),
        AccountMeta::new(a.source_ata, false),
        AccountMeta::new(a.destination_ata, false),
    ];
    accounts.extend(route_accounts);
    run_instruction(template, accounts, &inputs)
}
// #endregion token-sweep

// #region jito-tip
/// `jito_tip` is one of Jito's eight tip accounts; `strategy_accounts` is the route's account
/// list from the third account on (the template passes the token program and the searcher).
pub fn run_jito_tip(
    template: Pubkey,
    searcher: Pubkey,
    jito_tip: Pubkey,
    strategy_data: &[u8],
    tip_lamports: u64,
    minimum_edge: u64,
    strategy_accounts: Vec<AccountMeta>,
) -> Instruction {
    let inputs = RunInputs::new()
        .groups(&[strategy_accounts.len() as u8]) // strategyAccounts
        .bytes(strategy_data)
        .u64(tip_lamports)
        .u64(minimum_edge)
        .finish();
    let mut accounts = vec![
        program(SYSTEM_PROGRAM_ID),
        program(JUPITER_V6),
        program(TOKEN_PROGRAM_ID),
        AccountMeta::new(searcher, true),
        AccountMeta::new(jito_tip, false),
    ];
    accounts.extend(strategy_accounts);
    run_instruction(template, accounts, &inputs)
}
// #endregion jito-tip

// #region pyth-gate
/// `action_accounts` is the route's account list from the third account on: the template passes
/// the token program and the actor itself.
pub fn run_pyth_gate(
    template: Pubkey,
    price_update: Pubkey,
    actor: Pubkey,
    maximum_age: i64,
    maximum_confidence: u64,
    (floor_price, ceiling_price): (i64, i64),
    action_data: &[u8],
    action_accounts: Vec<AccountMeta>,
) -> Instruction {
    let inputs = RunInputs::new()
        .groups(&[action_accounts.len() as u8]) // actionAccounts
        .i64(maximum_age)
        .u64(maximum_confidence)
        .i64(floor_price)
        .i64(ceiling_price)
        .bytes(action_data)
        .finish();
    let mut accounts = vec![
        AccountMeta::new_readonly(price_update, false),
        program(JUPITER_V6),
        program(TOKEN_PROGRAM_ID),
        AccountMeta::new(actor, true),
    ];
    accounts.extend(action_accounts);
    run_instruction(template, accounts, &inputs)
}
// #endregion pyth-gate

// #region orca-compound
pub struct OrcaCompoundAccounts {
    pub position_authority: Pubkey,
    pub whirlpool: Pubkey,
    pub position: Pubkey,
    pub position_token_account: Pubkey,
    pub token_owner_account_a: Pubkey,
    pub token_owner_account_b: Pubkey,
    pub token_vault_a: Pubkey,
    pub token_vault_b: Pubkey,
    pub tick_array_lower: Pubkey,
    pub tick_array_upper: Pubkey,
}

pub fn run_orca_compound(
    template: Pubkey,
    a: &OrcaCompoundAccounts,
    liquidity_amount: u128,
    dust_floor: u64,
) -> Instruction {
    let inputs = RunInputs::new().u128(liquidity_amount).u64(dust_floor).finish();
    let accounts = vec![
        program(ORCA_WHIRLPOOL),
        program(TOKEN_PROGRAM_ID),
        AccountMeta::new_readonly(a.position_authority, true),
        AccountMeta::new(a.whirlpool, false),
        AccountMeta::new(a.position, false),
        AccountMeta::new_readonly(a.position_token_account, false),
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
    pub token_owner_account_a: Pubkey,
    pub token_owner_account_b: Pubkey,
    pub token_vault_a: Pubkey,
    pub token_vault_b: Pubkey,
}

/// One row per position: `(position, position_token_account)`. The template takes 1 to 12, and
/// the row count comes from the account list, so there is no count to pass.
pub fn run_orca_harvest(
    template: Pubkey,
    a: &OrcaHarvestAccounts,
    positions: &[(Pubkey, Pubkey)],
    dust_floor: u64,
) -> Instruction {
    assert!((1..=12).contains(&positions.len()), "the template takes 1 to 12 positions");
    let inputs = RunInputs::new().u64(dust_floor).finish();
    let mut accounts = vec![
        program(ORCA_WHIRLPOOL),
        program(TOKEN_PROGRAM_ID),
        AccountMeta::new_readonly(a.position_authority, true),
        AccountMeta::new(a.whirlpool, false),
        AccountMeta::new(a.token_owner_account_a, false),
        AccountMeta::new(a.token_owner_account_b, false),
        AccountMeta::new(a.token_vault_a, false),
        AccountMeta::new(a.token_vault_b, false),
    ];
    for (position, position_token_account) in positions {
        accounts.push(AccountMeta::new(*position, false));
        accounts.push(AccountMeta::new_readonly(*position_token_account, false));
    }
    run_instruction(template, accounts, &inputs)
}
// #endregion orca-harvest

// #region kamino-repay
pub struct KaminoRepayAccounts {
    pub borrower: Pubkey,
    pub collateral_ata: Pubkey,
    pub borrowed_asset_ata: Pubkey,
    pub obligation: Pubkey,
    pub lending_market: Pubkey,
    pub repay_reserve: Pubkey,
    pub reserve_liquidity_supply: Pubkey,
    pub reserve_price_feed: Pubkey,
}

/// `route_accounts` is the route's account list from the fifth account on.
pub fn run_kamino_repay(
    template: Pubkey,
    a: &KaminoRepayAccounts,
    route_args: &[u8],
    minimum_repayment: u64,
    route_accounts: Vec<AccountMeta>,
) -> Instruction {
    let inputs = RunInputs::new()
        .groups(&[route_accounts.len() as u8]) // routeAccounts
        .bytes(route_args)
        .u64(minimum_repayment)
        .finish();
    let mut accounts = vec![
        program(JUPITER_V6),
        program(KAMINO_LEND),
        program(TOKEN_PROGRAM_ID),
        AccountMeta::new(a.borrower, true),
        AccountMeta::new(a.collateral_ata, false),
        AccountMeta::new(a.borrowed_asset_ata, false),
        AccountMeta::new(a.obligation, false),
        AccountMeta::new_readonly(a.lending_market, false),
        AccountMeta::new(a.repay_reserve, false),
        AccountMeta::new(a.reserve_liquidity_supply, false),
        AccountMeta::new_readonly(a.reserve_price_feed, false),
    ];
    accounts.extend(route_accounts);
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
    pub repay_reserve_liquidity_supply: Pubkey,
    pub withdraw_reserve: Pubkey,
    pub withdraw_reserve_collateral_mint: Pubkey,
    pub withdraw_reserve_liquidity_supply: Pubkey,
    pub reserve_price_feed: Pubkey,
    pub user_source_liquidity: Pubkey,
    pub user_destination_collateral: Pubkey,
}

pub fn run_kamino_liquidate(
    template: Pubkey,
    a: &KaminoLiquidateAccounts,
    liquidity_amount: u64,
    min_acceptable_received: u64,
    minimum_bounty: u64,
) -> Instruction {
    let inputs = RunInputs::new()
        .u64(liquidity_amount)
        .u64(min_acceptable_received)
        .u64(minimum_bounty)
        .finish();
    let accounts = vec![
        program(KAMINO_LEND),
        program(TOKEN_PROGRAM_ID),
        AccountMeta::new(a.liquidator, true),
        AccountMeta::new(a.obligation, false),
        AccountMeta::new_readonly(a.lending_market, false),
        AccountMeta::new_readonly(a.lending_market_authority, false),
        AccountMeta::new(a.repay_reserve, false),
        AccountMeta::new(a.repay_reserve_liquidity_supply, false),
        AccountMeta::new(a.withdraw_reserve, false),
        AccountMeta::new(a.withdraw_reserve_collateral_mint, false),
        AccountMeta::new(a.withdraw_reserve_liquidity_supply, false),
        AccountMeta::new_readonly(a.reserve_price_feed, false),
        AccountMeta::new(a.user_source_liquidity, false),
        AccountMeta::new(a.user_destination_collateral, false),
    ];
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
    pub bank_liquidity_vault_authority: Pubkey,
    pub destination_ata: Pubkey,
    pub treasury_ata: Pubkey,
}

pub fn run_marginfi_withdraw(
    template: Pubkey,
    a: &MarginfiWithdrawAccounts,
    minimum_withdrawn: u64,
) -> Instruction {
    let inputs = RunInputs::new().u64(minimum_withdrawn).finish();
    let accounts = vec![
        program(MARGINFI_V2),
        program(TOKEN_PROGRAM_ID),
        AccountMeta::new_readonly(a.marginfi_group, false),
        AccountMeta::new(a.marginfi_account, false),
        AccountMeta::new_readonly(a.authority, true),
        AccountMeta::new(a.bank, false),
        AccountMeta::new(a.bank_liquidity_vault, false),
        AccountMeta::new(a.bank_liquidity_vault_authority, false),
        AccountMeta::new(a.destination_ata, false),
        AccountMeta::new(a.treasury_ata, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion marginfi-withdraw

// #region drift-rebalance
pub struct DriftRebalanceAccounts {
    pub owner: Pubkey,
    pub wallet_ata: Pubkey,
    pub marginfi_group: Pubkey,
    pub marginfi_account: Pubkey,
    pub marginfi_bank: Pubkey,
    pub marginfi_vault: Pubkey,
    pub marginfi_vault_authority: Pubkey,
    pub drift_state: Pubkey,
    pub drift_user: Pubkey,
    pub drift_user_stats: Pubkey,
    pub drift_spot_market_vault: Pubkey,
}

pub fn run_drift_rebalance(
    template: Pubkey,
    a: &DriftRebalanceAccounts,
    minimum_moved: u64,
) -> Instruction {
    let inputs = RunInputs::new().u64(minimum_moved).finish();
    let accounts = vec![
        program(MARGINFI_V2),
        program(DRIFT_V2),
        program(TOKEN_PROGRAM_ID),
        AccountMeta::new(a.owner, true),
        AccountMeta::new(a.wallet_ata, false),
        AccountMeta::new_readonly(a.marginfi_group, false),
        AccountMeta::new(a.marginfi_account, false),
        AccountMeta::new(a.marginfi_bank, false),
        AccountMeta::new(a.marginfi_vault, false),
        AccountMeta::new(a.marginfi_vault_authority, false),
        AccountMeta::new_readonly(a.drift_state, false),
        AccountMeta::new(a.drift_user, false),
        AccountMeta::new(a.drift_user_stats, false),
        AccountMeta::new(a.drift_spot_market_vault, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion drift-rebalance

// #region drift-settle
pub struct DriftSettleAccounts {
    pub owner: Pubkey,
    pub drift_state: Pubkey,
    pub drift_user: Pubkey,
    pub drift_user_stats: Pubkey,
    pub drift_spot_market_vault: Pubkey,
    pub drift_signer: Pubkey,
    pub perp_market: Pubkey,
    pub spot_market: Pubkey,
    pub destination_ata: Pubkey,
}

pub fn run_drift_settle(
    template: Pubkey,
    a: &DriftSettleAccounts,
    minimum_settled: u64,
) -> Instruction {
    let inputs = RunInputs::new().u64(minimum_settled).finish();
    let accounts = vec![
        program(DRIFT_V2),
        program(TOKEN_PROGRAM_ID),
        AccountMeta::new(a.owner, true),
        AccountMeta::new_readonly(a.drift_state, false),
        AccountMeta::new(a.drift_user, false),
        AccountMeta::new(a.drift_user_stats, false),
        AccountMeta::new(a.drift_spot_market_vault, false),
        AccountMeta::new_readonly(a.drift_signer, false),
        AccountMeta::new(a.perp_market, false),
        AccountMeta::new(a.spot_market, false),
        AccountMeta::new(a.destination_ata, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion drift-settle

// ------------------------------------------------------------- sample runs

/// A stand-in template address for the sample runs.
pub const TEMPLATE: Pubkey = pubkey!("Temp1ate11111111111111111111111111111111111");

fn key(seed: u8) -> Pubkey {
    Pubkey::new_from_array([seed; 32])
}

fn group(count: u8) -> Vec<AccountMeta> {
    (0..count).map(|index| AccountMeta::new(key(200 + index), false)).collect()
}

/// A sample run of every template, under the name `fixtures/protocol-examples.json` records it by.
pub const RUNS: [(&str, fn() -> Instruction); 12] = [
    ("driftRebalanceExact", || {
        let a = DriftRebalanceAccounts {
            owner: key(1),
            wallet_ata: key(2),
            marginfi_group: key(3),
            marginfi_account: key(4),
            marginfi_bank: key(5),
            marginfi_vault: key(6),
            marginfi_vault_authority: key(7),
            drift_state: key(8),
            drift_user: key(9),
            drift_user_stats: key(10),
            drift_spot_market_vault: key(11),
        };
        run_drift_rebalance(TEMPLATE, &a, 1_000_000)
    }),
    ("driftSettleWhenProfitable", || {
        let a = DriftSettleAccounts {
            owner: key(1),
            drift_state: key(2),
            drift_user: key(3),
            drift_user_stats: key(4),
            drift_spot_market_vault: key(5),
            drift_signer: key(6),
            perp_market: key(7),
            spot_market: key(8),
            destination_ata: key(9),
        };
        run_drift_settle(TEMPLATE, &a, 1_000_000)
    }),
    ("jitoProfitGuardedTip", || {
        run_jito_tip(TEMPLATE, key(1), key(2), &[7; 40], 10_000, 50_000, group(9))
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
            reserve_liquidity_supply: key(8),
            reserve_collateral_mint: key(9),
            reserve_destination_deposit_collateral: key(10),
        };
        run_jupiter_deposit(TEMPLATE, &a, &[7; 96], 1_000_000, group(12))
    }),
    ("jupiterOracleCheckedSwap", || {
        let a = OracleSwapAccounts {
            price_update: key(1),
            trader: key(2),
            source_ata: key(3),
            destination_ata: key(4),
        };
        run_jupiter_oracle_swap(TEMPLATE, &a, &[7; 96], -8, 10u128.pow(11), 50, group(12))
    }),
    ("kaminoLiquidateWithProof", || {
        let a = KaminoLiquidateAccounts {
            liquidator: key(1),
            obligation: key(2),
            lending_market: key(3),
            lending_market_authority: key(4),
            repay_reserve: key(5),
            repay_reserve_liquidity_supply: key(6),
            withdraw_reserve: key(7),
            withdraw_reserve_collateral_mint: key(8),
            withdraw_reserve_liquidity_supply: key(9),
            reserve_price_feed: key(10),
            user_source_liquidity: key(11),
            user_destination_collateral: key(12),
        };
        run_kamino_liquidate(TEMPLATE, &a, 5_000_000, 4_000_000, 100_000)
    }),
    ("kaminoRepaySwapOutput", || {
        let a = KaminoRepayAccounts {
            borrower: key(1),
            collateral_ata: key(2),
            borrowed_asset_ata: key(3),
            obligation: key(4),
            lending_market: key(5),
            repay_reserve: key(6),
            reserve_liquidity_supply: key(7),
            reserve_price_feed: key(8),
        };
        run_kamino_repay(TEMPLATE, &a, &[7; 96], 1_000_000, group(12))
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
        run_marginfi_withdraw(TEMPLATE, &a, 1_000_000)
    }),
    ("orcaCompoundFees", || {
        let a = OrcaCompoundAccounts {
            position_authority: key(1),
            whirlpool: key(2),
            position: key(3),
            position_token_account: key(4),
            token_owner_account_a: key(5),
            token_owner_account_b: key(6),
            token_vault_a: key(7),
            token_vault_b: key(8),
            tick_array_lower: key(9),
            tick_array_upper: key(10),
        };
        run_orca_compound(TEMPLATE, &a, 1_000_000_000, 10_000)
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
        let positions: Vec<_> = (0..5).map(|row| (key(100 + row), key(150 + row))).collect();
        run_orca_harvest(TEMPLATE, &a, &positions, 10_000)
    }),
    ("pythFreshPriceGate", || {
        run_pyth_gate(
            TEMPLATE,
            key(1),
            key(2),
            60,
            1_000_000,
            (90_00000000, 250_00000000),
            &[7; 96],
            group(14),
        )
    }),
    ("tokenSweepIntoSwap", || {
        let a = TokenSweepAccounts { seller: key(1), source_ata: key(2), destination_ata: key(3) };
        let quote = RouteQuote {
            route_plan: &[7; 60],
            in_amount: 1_000_000,
            quoted_out_amount: 990_000,
            slippage_bps: 50,
            platform_fee_bps: 0,
        };
        run_token_sweep(TEMPLATE, &a, &quote, 1_000, group(12))
    }),
];

#[allow(dead_code)]
fn main() {
    for (name, run) in RUNS {
        let instruction = run();
        println!(
            "{name:<30} {:>3} accounts  {:>4} data bytes",
            instruction.accounts.len(),
            instruction.data.len()
        );
    }
}
