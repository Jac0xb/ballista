//! Run the live-protocol templates from Rust.
//!
//! The templates are authored once in TypeScript (`clients/js/examples/protocols`) and uploaded
//! once. Everything after that is a service building one instruction, which is the half that
//! usually lives in Rust: liquidation bots, crank keepers, rebalancers.
//!
//! A runner never sees the bytecode. It needs three things from the template author: the order
//! of the fixed accounts, the order of the inputs, and — for a batched template — the row shape.
//! There are only three run shapes across all twelve examples, and they are all below.
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

// #region plain
/// Shape one: fixed accounts and fixed inputs, in the order the template declares them.
///
/// This is `pyth-fresh-price-gate`. Its inputs are `maximumAge`, `maximumConfidence`,
/// `floorPrice`, `ceilingPrice`, `actionData`; its accounts are the price update, the action
/// program, and the actor.
pub struct PriceGate {
    pub price_update: Pubkey,
    pub action_program: Pubkey,
    pub actor: Pubkey,
}

pub fn run_price_gate(
    template: Pubkey,
    gate: &PriceGate,
    maximum_age: i64,
    maximum_confidence: u64,
    band: (i64, i64),
    action_data: &[u8],
    action_accounts: Vec<AccountMeta>,
) -> Instruction {
    // Group lengths come first, before any value, one byte per declared group.
    let inputs = RunInputs::new()
        .groups(&[action_accounts.len() as u8])
        .i64(maximum_age)
        .u64(maximum_confidence)
        .i64(band.0)
        .i64(band.1)
        .bytes(action_data)
        .finish();

    let mut accounts = vec![
        AccountMeta::new_readonly(gate.price_update, false),
        AccountMeta::new_readonly(gate.action_program, false),
        AccountMeta::new(gate.actor, true),
    ];
    // Group members follow the declared accounts. They never sign, whatever flags they carry.
    accounts.extend(action_accounts);
    run_instruction(template, accounts, &inputs)
}
// #endregion plain

// #region group
/// Shape two: an account group, for a callee whose account list is not a fixed length.
///
/// This is `jupiter-deposit-exact-output`. The route's accounts arrive as a group, so one
/// template serves every route the aggregator returns.
pub fn run_jupiter_deposit(
    template: Pubkey,
    owner: Pubkey,
    destination_ata: Pubkey,
    kamino_accounts: [Pubkey; 8],
    route_data: &[u8],
    route_accounts: Vec<AccountMeta>,
    minimum_out: u64,
) -> Instruction {
    let inputs = RunInputs::new()
        .groups(&[route_accounts.len() as u8])
        .bytes(route_data)
        .u64(minimum_out)
        .finish();

    let mut accounts = vec![
        AccountMeta::new_readonly(pubkey!("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4"), false),
        AccountMeta::new_readonly(KAMINO_LEND, false),
        AccountMeta::new_readonly(TOKEN_PROGRAM, false),
        AccountMeta::new(owner, true),
        AccountMeta::new(destination_ata, false),
    ];
    accounts.extend(kamino_accounts.iter().enumerate().map(|(index, key)| {
        // The obligation and reserve accounts are written; the market and its authority are not.
        if matches!(index, 1 | 2) {
            AccountMeta::new_readonly(*key, false)
        } else {
            AccountMeta::new(*key, false)
        }
    }));
    accounts.extend(route_accounts);
    run_instruction(template, accounts, &inputs)
}
// #endregion group

// #region rows
/// Shape three: batch rows. The iteration count comes from how many rows are passed, so there is
/// no count in the instruction data to get wrong.
///
/// This is `orca-harvest-many-positions`, whose row is `(position, position_token_account)`.
pub fn run_orca_harvest(
    template: Pubkey,
    authority: Pubkey,
    whirlpool: Pubkey,
    owner_accounts: (Pubkey, Pubkey),
    vaults: (Pubkey, Pubkey),
    positions: &[(Pubkey, Pubkey)],
    dust_floor: u64,
) -> Instruction {
    assert!(!positions.is_empty(), "the template declares minIterations 1");
    assert!(positions.len() <= 12, "the template declares maxIterations 12");

    let inputs = RunInputs::new().u64(dust_floor).finish();
    let mut accounts = vec![
        AccountMeta::new_readonly(ORCA_WHIRLPOOL, false),
        AccountMeta::new_readonly(TOKEN_PROGRAM, false),
        AccountMeta::new_readonly(authority, true),
        AccountMeta::new(whirlpool, false),
        AccountMeta::new(owner_accounts.0, false),
        AccountMeta::new(owner_accounts.1, false),
        AccountMeta::new(vaults.0, false),
        AccountMeta::new(vaults.1, false),
    ];
    // One row after another, each in the order the row schema declares.
    for (position, position_token_account) in positions {
        accounts.push(AccountMeta::new(*position, false));
        accounts.push(AccountMeta::new_readonly(*position_token_account, false));
    }
    run_instruction(template, accounts, &inputs)
}
// #endregion rows

fn main() {
    let creator = Pubkey::new_unique();
    let (template, _) = find_template_pda(&creator, 1);
    let key = Pubkey::new_unique;

    let gate = run_price_gate(
        template,
        &PriceGate { price_update: key(), action_program: SYSTEM_PROGRAM_ID, actor: key() },
        60,
        1_000_000,
        (90_00000000, 250_00000000),
        &[1, 2, 3, 4],
        vec![AccountMeta::new(key(), false); 6],
    );
    println!("price gate        {} accounts, {} data bytes", gate.accounts.len(), gate.data.len());

    let deposit = run_jupiter_deposit(
        template,
        key(),
        key(),
        [key(), key(), key(), key(), key(), key(), key(), key()],
        &[0xc1; 96],
        vec![AccountMeta::new(key(), false); 24],
        1_000_000,
    );
    println!("jupiter deposit   {} accounts, {} data bytes", deposit.accounts.len(), deposit.data.len());

    let positions: Vec<(Pubkey, Pubkey)> = (0..5).map(|_| (key(), key())).collect();
    let harvest = run_orca_harvest(
        template,
        key(),
        key(),
        (key(), key()),
        (key(), key()),
        &positions,
        10_000,
    );
    println!("orca harvest      {} accounts, {} rows", harvest.accounts.len(), positions.len());

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
