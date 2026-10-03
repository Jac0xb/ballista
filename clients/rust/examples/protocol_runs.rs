//! Run the live-protocol templates from Rust.
//!
//! The templates are authored once in TypeScript (`clients/js/examples/protocols`) and uploaded
//! once. Everything after that is a service building one instruction, which is the half that
//! usually lives in Rust: liquidation bots, crank keepers, rebalancers.
//!
//! A runner never sees the bytecode. It needs three things from the template author: the order
//! of the fixed accounts, the order of the inputs, and — for a batched template — the row shape.
//! There are only three run shapes across all eleven examples, and they are all below.
//!
//! ```bash
//! cargo run -p ballista-sdk --example protocol_runs
//! ```

use ballista_sdk::{
    decode_ballista_error, find_template_pda, run_instruction, RunInputs, SYSTEM_PROGRAM_ID,
};
use solana_program::{instruction::AccountMeta, instruction::Instruction, pubkey, pubkey::Pubkey};

const TOKEN_PROGRAM: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const ORCA_WHIRLPOOL: Pubkey = pubkey!("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");
const KAMINO_LEND: Pubkey = pubkey!("KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD");
const PYTH_RECEIVER: Pubkey = pubkey!("rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ");
const JUPITER_V6: Pubkey = pubkey!("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");
const KAMINO_FARMS: Pubkey = pubkey!("FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr");
const INSTRUCTIONS_SYSVAR: Pubkey = pubkey!("Sysvar1nstructions1111111111111111111111111");

/// An Anchor instruction discriminator: the first eight bytes of `sha256("global:<handler>")`.
fn anchor_discriminator(handler: &str) -> Vec<u8> {
    solana_sha256_hasher::hash(format!("global:{handler}").as_bytes()).to_bytes()[..8].to_vec()
}

/// Appends Jupiter `route` data, as the Swap API returns it, in the parts every Jupiter template
/// takes: the plan (the bytes between the discriminator and `in_amount`), then `in_amount`,
/// `quoted_out_amount`, `slippage_bps` and `platform_fee_bps`, each as a u64. The templates take
/// the route in parts so that they can cap its platform fee.
fn route_inputs(inputs: RunInputs, route_data: &[u8]) -> RunInputs {
    const TAIL: usize = 8 + 8 + 2 + 1;
    assert!(
        route_data.len() >= 8 + 4 + TAIL && route_data[..8] == anchor_discriminator("route")[..],
        "not Jupiter `route` data; ask the Swap API for useSharedAccounts: false"
    );
    let (plan, tail) = route_data[8..].split_at(route_data.len() - 8 - TAIL);
    inputs
        .bytes(plan)
        .u64(u64::from_le_bytes(tail[0..8].try_into().unwrap()))
        .u64(u64::from_le_bytes(tail[8..16].try_into().unwrap()))
        .u64(u16::from_le_bytes([tail[16], tail[17]]).into())
        .u64(tail[18].into())
}

// #region plain
/// Shape one: fixed accounts and fixed inputs, in the order the template declares them.
///
/// This is `pyth-fresh-price-gate`. Its inputs are `feedId`, `exponent`, `maximumAge`,
/// `maximumConfidence`, `floorPrice`, `ceilingPrice`, then the route's parts (see
/// [`route_inputs`]); its accounts are the price
/// update, the action program, the token program, and the actor. `feed_id` is the Pyth feed the
/// price update must carry, as 32 bytes, and `exponent` the exponent it must have: the confidence
/// limit and the band are raw integers at it (SOL/USD's is −8). The action is a Jupiter `route`,
/// whose list starts with the token program and the actor, so `action_accounts` is its list from
/// the third account on.
pub struct PriceGate {
    pub price_update: Pubkey,
    pub feed_id: [u8; 32],
    pub exponent: i32,
    pub action_program: Pubkey,
    pub actor: Pubkey,
}

pub fn run_price_gate(
    template: Pubkey,
    gate: &PriceGate,
    maximum_age: i64,
    maximum_confidence: u64,
    band: (i64, i64),
    route_data: &[u8],
    action_accounts: Vec<AccountMeta>,
) -> Instruction {
    // Group lengths come first, before any value, one byte per declared group.
    let inputs = RunInputs::new()
        .groups(&[action_accounts.len() as u8])
        .pubkey(&Pubkey::new_from_array(gate.feed_id))
        // Pyth stores the exponent as an i32; the template takes it as an i64.
        .i64(gate.exponent.into())
        .i64(maximum_age)
        .u64(maximum_confidence)
        .i64(band.0)
        .i64(band.1);
    let inputs = route_inputs(inputs, route_data).finish();

    let mut accounts = vec![
        AccountMeta::new_readonly(gate.price_update, false),
        AccountMeta::new_readonly(gate.action_program, false),
        AccountMeta::new_readonly(TOKEN_PROGRAM, false),
        AccountMeta::new(gate.actor, true),
    ];
    // Group members follow the declared accounts. They never sign, whatever flags they carry.
    accounts.extend(action_accounts);
    run_instruction(template, accounts, &inputs)
}
// #endregion plain

// #region group
/// Kamino's reserve-side accounts for the deposit, and the reserve's collateral farm if it has one.
pub struct KaminoDeposit {
    pub obligation: Pubkey,
    pub lending_market: Pubkey,
    pub lending_market_authority: Pubkey,
    pub reserve: Pubkey,
    pub reserve_liquidity_mint: Pubkey,
    pub reserve_liquidity_supply: Pubkey,
    pub reserve_collateral_mint: Pubkey,
    pub reserve_collateral_supply: Pubkey,
    /// `(obligation farm user state, reserve farm state)` when the reserve has a collateral farm.
    pub farm: Option<(Pubkey, Pubkey)>,
}

/// Shape two: account groups, for callees whose account lists are not a fixed length.
///
/// This is `jupiter-deposit-exact-output`.
/// - Jupiter's `route` starts with the token program, the signing owner, and the owner's source
///   and destination token accounts. The template passes those four itself, so `route_accounts`
///   is the Swap API's list from the fifth account on, and one template serves every route.
///   `route_data` is the Swap API's instruction data, which [`route_inputs`] splits.
/// - Kamino's deposit ends in two farm accounts, writable when the reserve has a collateral farm
///   and the Kamino program ID when it does not, then the Farms program. They travel as a second
///   group, which keeps each account's own writable flag.
///
/// Send the run after [`kamino_refreshes`], in the same transaction.
pub fn run_jupiter_deposit(
    template: Pubkey,
    owner: Pubkey,
    token_accounts: (Pubkey, Pubkey),
    kamino: &KaminoDeposit,
    route_data: &[u8],
    route_accounts: Vec<AccountMeta>,
    minimum_out: u64,
) -> Instruction {
    let mut farm_accounts = match kamino.farm {
        Some((user_state, farm_state)) => vec![
            AccountMeta::new(user_state, false),
            AccountMeta::new(farm_state, false),
        ],
        None => vec![AccountMeta::new_readonly(KAMINO_LEND, false); 2],
    };
    farm_accounts.push(AccountMeta::new_readonly(KAMINO_FARMS, false));
    // Group lengths come first, before any value, one byte per declared group.
    let inputs = RunInputs::new().groups(&[route_accounts.len() as u8, farm_accounts.len() as u8]);
    let inputs = route_inputs(inputs, route_data).u64(minimum_out).finish();

    let mut accounts = vec![
        AccountMeta::new_readonly(JUPITER_V6, false),
        AccountMeta::new_readonly(KAMINO_LEND, false),
        AccountMeta::new_readonly(TOKEN_PROGRAM, false),
        AccountMeta::new_readonly(INSTRUCTIONS_SYSVAR, false),
        AccountMeta::new(owner, true),
        AccountMeta::new(token_accounts.0, false),
        AccountMeta::new(token_accounts.1, false),
        AccountMeta::new(kamino.obligation, false),
        AccountMeta::new_readonly(kamino.lending_market, false),
        AccountMeta::new_readonly(kamino.lending_market_authority, false),
        AccountMeta::new(kamino.reserve, false),
        AccountMeta::new_readonly(kamino.reserve_liquidity_mint, false),
        AccountMeta::new(kamino.reserve_liquidity_supply, false),
        AccountMeta::new(kamino.reserve_collateral_mint, false),
        AccountMeta::new(kamino.reserve_collateral_supply, false),
    ];
    // The groups follow the declared accounts, in declaration order.
    accounts.extend(route_accounts);
    accounts.extend(farm_accounts);
    run_instruction(template, accounts, &inputs)
}
// #endregion group

// #region refresh
/// Kamino's refreshes. A run that deposits into, repays or liquidates an obligation needs them
/// earlier in the same transaction. Kamino's v2 instructions check only that the reserves they
/// price and the obligation were refreshed in the current slot, not where.
///
/// - `held` is every reserve the obligation holds: deposits in its deposit order, then borrows in
///   its borrow order.
/// - `touched` adds any other reserve the run needs fresh, such as a first borrow's. A first
///   deposit's needs none: Kamino's deposit refreshes its own reserve.
/// - Each reserve comes with the Scope price account its config names. The main market prices by
///   Scope alone, so the Pyth and Switchboard slots take the Kamino program ID, which it reads as
///   "none".
/// - `referrer_token_states` is empty unless the obligation has a referrer. Then Kamino expects
///   one per borrow after the reserves, or it fails with `InvalidAccountInput`: the PDA
///   `["referrer_acc", referrer, borrow reserve]`. It reads them in borrow order, but only for
///   reserves that pay referral fees, so those go first. klend-interface's `refresh_obligation`
///   helper builds this list.
pub fn kamino_refreshes(
    lending_market: Pubkey,
    obligation: Pubkey,
    held: &[(Pubkey, Pubkey)],
    touched: &[(Pubkey, Pubkey)],
    referrer_token_states: &[Pubkey],
) -> Vec<Instruction> {
    let refresh_reserve = |&(reserve, scope_prices): &(Pubkey, Pubkey)| Instruction {
        program_id: KAMINO_LEND,
        accounts: vec![
            AccountMeta::new(reserve, false),
            AccountMeta::new_readonly(lending_market, false),
            AccountMeta::new_readonly(KAMINO_LEND, false), // Pyth
            AccountMeta::new_readonly(KAMINO_LEND, false), // Switchboard price
            AccountMeta::new_readonly(KAMINO_LEND, false), // Switchboard TWAP
            AccountMeta::new_readonly(scope_prices, false),
        ],
        data: anchor_discriminator("refresh_reserve"),
    };
    let mut refreshed: Vec<Pubkey> = Vec::new();
    let mut instructions = Vec::new();
    for entry in held.iter().chain(touched) {
        if !refreshed.contains(&entry.0) {
            refreshed.push(entry.0);
            instructions.push(refresh_reserve(entry));
        }
    }
    let mut accounts = vec![
        AccountMeta::new_readonly(lending_market, false),
        AccountMeta::new(obligation, false),
    ];
    // Every reserve the obligation holds, writable, in its own order; then any referrer token
    // states, writable.
    accounts.extend(held.iter().map(|&(reserve, _)| AccountMeta::new(reserve, false)));
    accounts.extend(referrer_token_states.iter().map(|&state| AccountMeta::new(state, false)));
    instructions.push(Instruction {
        program_id: KAMINO_LEND,
        accounts,
        data: anchor_discriminator("refresh_obligation"),
    });
    instructions
}
// #endregion refresh

// #region rows
/// This is `orca-harvest-many-positions`. Its row is a position, the token account holding the
/// position's NFT, and the tick arrays holding the position's lower and upper ticks.
pub struct HarvestRow {
    pub position: Pubkey,
    pub position_token_account: Pubkey,
    pub tick_array_lower: Pubkey,
    pub tick_array_upper: Pubkey,
}

/// Shape three: batch rows. The iteration count comes from how many rows are passed, so there is
/// no count in the instruction data to get wrong.
///
/// Every row must belong to the same holder: `owner_accounts` are fixed for the whole batch, and
/// the fees a row earns reach them only when that row's own position NFT is held by the same
/// owner.
pub fn run_orca_harvest(
    template: Pubkey,
    authority: Pubkey,
    whirlpool: Pubkey,
    owner_accounts: (Pubkey, Pubkey),
    vaults: (Pubkey, Pubkey),
    rows: &[HarvestRow],
    dust_floor: u64,
) -> Instruction {
    assert!(!rows.is_empty(), "the template declares minIterations 1");
    assert!(rows.len() <= 12, "the template declares maxIterations 12");

    let inputs = RunInputs::new().u64(dust_floor).finish();
    let mut accounts = vec![
        AccountMeta::new_readonly(ORCA_WHIRLPOOL, false),
        AccountMeta::new_readonly(TOKEN_PROGRAM, false),
        AccountMeta::new_readonly(authority, true),
        // Each row's update_fees_and_rewards writes the pool.
        AccountMeta::new(whirlpool, false),
        AccountMeta::new(owner_accounts.0, false),
        AccountMeta::new(owner_accounts.1, false),
        AccountMeta::new(vaults.0, false),
        AccountMeta::new(vaults.1, false),
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
// #endregion rows

fn main() {
    let creator = Pubkey::new_unique();
    let (template, _) = find_template_pda(&creator, 1);
    let key = Pubkey::new_unique;
    // `route` data with an 80-byte plan, quoted at 1,000,000 in for 990,000 out, with no
    // platform fee.
    let mut route_data = anchor_discriminator("route");
    route_data.extend_from_slice(&[0xc1; 80]);
    route_data.extend_from_slice(&1_000_000u64.to_le_bytes());
    route_data.extend_from_slice(&990_000u64.to_le_bytes());
    route_data.extend_from_slice(&50u16.to_le_bytes());
    route_data.push(0);

    // SOL/USD's feed id, `ef0d8b6f…c280b56d`.
    let sol_usd = [
        0xef, 0x0d, 0x8b, 0x6f, 0xda, 0x2c, 0xeb, 0xa4, 0x1d, 0xa1, 0x5d, 0x40, 0x95, 0xd1, 0xda,
        0x39, 0x2a, 0x0d, 0x2f, 0x8e, 0xd0, 0xc6, 0xc7, 0xbc, 0x0f, 0x4c, 0xfa, 0xc8, 0xc2, 0x80,
        0xb5, 0x6d,
    ];
    let gate = run_price_gate(
        template,
        &PriceGate {
            price_update: key(),
            feed_id: sol_usd,
            exponent: -8,
            action_program: SYSTEM_PROGRAM_ID,
            actor: key(),
        },
        60,
        1_000_000,
        (90_00000000, 250_00000000),
        &route_data,
        vec![AccountMeta::new(key(), false); 6],
    );
    println!("price gate        {} accounts, {} data bytes", gate.accounts.len(), gate.data.len());

    let kamino = KaminoDeposit {
        obligation: key(),
        lending_market: key(),
        lending_market_authority: key(),
        reserve: key(),
        reserve_liquidity_mint: key(),
        reserve_liquidity_supply: key(),
        reserve_collateral_mint: key(),
        reserve_collateral_supply: key(),
        farm: Some((key(), key())),
    };
    let deposit = run_jupiter_deposit(
        template,
        key(),
        (key(), key()),
        &kamino,
        &route_data,
        vec![AccountMeta::new(key(), false); 24],
        1_000_000,
    );
    println!("jupiter deposit   {} accounts, {} data bytes", deposit.accounts.len(), deposit.data.len());
    let refreshes = kamino_refreshes(key(), key(), &[(key(), key())], &[], &[]);
    println!("kamino refreshes  {} instructions before the run", refreshes.len());

    let rows: Vec<HarvestRow> = (0..5)
        .map(|_| HarvestRow {
            position: key(),
            position_token_account: key(),
            tick_array_lower: key(),
            tick_array_upper: key(),
        })
        .collect();
    let harvest = run_orca_harvest(
        template,
        key(),
        key(),
        (key(), key()),
        (key(), key()),
        &rows,
        10_000,
    );
    println!("orca harvest      {} accounts, {} rows", harvest.accounts.len(), rows.len());

    // A guard that rejects is a `RequirementFailed` whose high half is the program counter, so a
    // runner can name the step that refused without knowing the bytecode.
    let _ = PYTH_RECEIVER;
    if let Some(decoded) = decode_ballista_error((4u32 << 16) | 6015) {
        println!(
            "rejected run      {} at instruction {}",
            decoded.name, decoded.context
        );
    }
}
