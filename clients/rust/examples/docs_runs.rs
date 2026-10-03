//! Build the `Run` instruction for each template from the guide and example pages.
//!
//! A runner never sees the bytecode. It passes the template address, one `AccountMeta` per
//! declared account in the order the template declares them (fixed accounts first, then each
//! row's accounts, then any account-group members), and the inputs encoded with `RunInputs` in
//! declaration order (group lengths first, then fixed inputs, then each row's inputs).
//!
//! `tests/docs_examples.rs` checks each function below against the run the TypeScript client
//! builds for the same example: the same run data and the same signer and writable flags.
//!
//! ```bash
//! cargo run -p ballista-sdk --example docs_runs
//! ```

#![allow(dead_code)]

use solana_program::{instruction::Instruction, pubkey::Pubkey};

fn main() {
    for (name, run) in ALL {
        let instruction = run(2);
        println!(
            "{name:40} {:3} accounts  {:4} data bytes",
            instruction.accounts.len(),
            instruction.data.len()
        );
    }
}

/// A distinct placeholder key per position, standing in for real addresses.
fn key(index: u8) -> Pubkey {
    Pubkey::new_from_array([index + 1; 32])
}

fn keys(start: u8, count: usize) -> Vec<Pubkey> {
    (0..count as u8).map(|index| key(start + index)).collect()
}

/// A System transfer's data padded to 32 bytes, the protocol-call stand-in the examples measure.
fn stand_in_data() -> Vec<u8> {
    let mut data = vec![2, 0, 0, 0];
    data.extend_from_slice(&10_000u64.to_le_bytes());
    data.resize(32, 0);
    data
}

/// Every run in this file, called with the example inputs and a given number of batch rows.
/// An example's name and a function that builds its run for a given number of batch rows.
pub type ExampleRun = (&'static str, fn(usize) -> Instruction);

pub const ALL: &[ExampleRun] = &[
    ("sweep-above-a-reserve", |_| {
        sweep_above_a_reserve(key(200), key(1), key(2), 2_000_000)
    }),
    ("forward-the-whole-token-balance", |_| {
        forward_the_whole_token_balance(key(200), key(1), key(2), key(3))
    }),
    ("repay-exactly-what-is-owed", |_| {
        repay_exactly_what_is_owed(key(200), key(1), key(2), key(3))
    }),
    ("split-what-arrived", |_| {
        split_what_arrived(key(200), key(1), key(2), key(3), 2_000_000, 3_000)
    }),
    ("claim-only-when-there-is-something", |_| {
        claim_only_when_there_is_something(key(200), key(1), key(2), key(3))
    }),
    ("liquidate-only-when-unhealthy", |_| {
        liquidate_only_when_unhealthy(key(200), key(1), key(2), key(3), 2)
    }),
    ("top-up-only-when-low", |_| {
        top_up_only_when_low(key(200), key(1), key(2), 2_000_000_000, 10_000)
    }),
    ("initialize-only-if-missing", |_| {
        initialize_only_if_missing(key(200), key(1), key(2))
    }),
    ("waterfall-until-the-money-runs-out", |rows| {
        let creditors: Vec<(Pubkey, u64)> =
            keys(10, rows).into_iter().map(|k| (k, 1_000)).collect();
        waterfall_until_the_money_runs_out(key(200), key(1), 2_000_000, &creditors)
    }),
    ("consolidate-only-the-funded-accounts", |rows| {
        consolidate_only_the_funded_accounts(key(200), key(1), key(2), &keys(10, rows))
    }),
    ("crank-only-the-ripe-entries", |rows| {
        crank_only_the_ripe_entries(key(200), key(1), &keys(10, rows))
    }),
    ("crank-once-per-waiting-entry", |_| {
        crank_once_per_waiting_entry(key(200), key(1), key(2))
    }),
    ("distribute-a-runtime-pot-pro-rata", |rows| {
        let holders: Vec<(Pubkey, u64)> = keys(10, rows).into_iter().map(|k| (k, 100)).collect();
        distribute_a_runtime_pot_pro_rata(key(200), key(1), 2_000_000, &holders)
    }),
    ("bounded-sol-payroll", |rows| {
        bounded_sol_payroll(key(200), key(1), &keys(10, rows), 10_000)
    }),
    ("basis-point-revenue-split", |_| {
        basis_point_revenue_split(key(200), key(1), key(2), key(3), 1_000_000, 250)
    }),
    ("index-weighted-rewards", |rows| {
        index_weighted_rewards(key(200), key(1), &keys(10, rows), 1_000)
    }),
    ("deadline-refund", |_| {
        deadline_refund(key(200), key(1), key(2), 10_000, 9_000_000_000)
    }),
    ("reserve-preserving-sweep", |_| {
        reserve_preserving_sweep(key(200), key(1), key(2), 1_000_000_000, 50_000)
    }),
    ("assert-create-then-transfer", |rows| {
        let recipients = keys(10, rows);
        let atas = keys(100, rows);
        let rows: Vec<(Pubkey, Pubkey)> = recipients.into_iter().zip(atas).collect();
        assert_create_then_transfer(key(200), key(1), key(2), key(3), key(4), &rows, 1_000)
    }),
    ("existing-account-token-payroll", |rows| {
        existing_account_token_payroll(key(200), key(1), key(2), &keys(10, rows), 1_000)
    }),
    ("conditional-ata-setup", |_| {
        conditional_ata_setup(key(200), key(1), key(2), key(3), key(4))
    }),
    ("close-empty-token-accounts", |rows| {
        close_empty_token_accounts(key(200), key(1), key(2), &keys(10, rows))
    }),
    ("exact-token-debit", |_| {
        exact_token_debit(key(200), key(1), key(2), key(3), 1_000)
    }),
    ("deadline-and-minimum-output", |_| {
        deadline_and_minimum_output(
            key(200),
            key(1),
            key(2),
            (9_000_000_000, 1_000, 900),
            &stand_in_data(),
        )
    }),
    ("pinned-program-and-owner", |_| {
        pinned_program_and_owner(key(200), key(1), key(2), 10_000)
    }),
    ("oracle-price-band", |_| {
        oracle_price_band(key(200), key(1), key(2), key(3), (1, 1_000_000))
    }),
    ("maximum-lamport-spend", |_| {
        maximum_lamport_spend(key(200), key(1), key(2), 1_000_000)
    }),
    ("canonical-position-account", |_| {
        canonical_position_account(key(200), key(1), 7).expect("the example key is off curve")
    }),
    ("swap-then-deposit", |_| {
        swap_then_deposit(
            key(200),
            key(1),
            key(2),
            key(3),
            0,
            &stand_in_data(),
            &stand_in_data(),
        )
    }),
    ("claim-then-distribute", |rows| {
        claim_then_distribute(
            key(200),
            key(1),
            key(2),
            key(3),
            key(4),
            &keys(10, rows),
            1_000,
        )
    }),
    ("primary-or-fallback-route", |_| {
        primary_or_fallback_route(
            key(200),
            key(1),
            key(2),
            true,
            &stand_in_data(),
            &stand_in_data(),
        )
    }),
    ("time-gated-governance-execution", |_| {
        time_gated_governance_execution(key(200), key(1), key(2), key(3), &stand_in_data())
    }),
    ("bounded-keeper-crank", |rows| {
        let markets = keys(10, rows);
        let queues = keys(100, rows);
        let rows: Vec<(Pubkey, Pubkey)> = markets.into_iter().zip(queues).collect();
        bounded_keeper_crank(key(200), key(1), &rows)
    }),
    ("swap-and-return-what-arrived", |_| {
        swap_and_return_what_arrived(key(200), key(1), key(2), key(3), 900, &stand_in_data())
    }),
    ("nested-swap-then-deposit", |_| {
        nested_swap_then_deposit(key(200), key(1), key(2), key(3), 900, &stand_in_data())
    }),
    ("row-amounts", |rows| {
        let payees: Vec<(Pubkey, u64)> = keys(10, rows)
            .into_iter()
            .enumerate()
            .map(|(i, k)| (k, 10_000 * (i as u64 + 1)))
            .collect();
        row_amounts(key(200), key(1), &payees)
    }),
    ("budgeted-payroll", |rows| {
        budgeted_payroll(key(200), key(1), &keys(10, rows), 10_000, 250_000)
    }),
    ("exact-lamport-delta", |_| {
        exact_lamport_delta(key(200), key(1), key(2), 50_000_000)
    }),
    ("token-transfer", |_| {
        token_transfer(key(200), key(1), key(2), key(3), 25_000)
    }),
    ("generic-cpi", |_| {
        generic_cpi(key(200), key(1), key(2), 25_000, &[9, 9, 9], true)
    }),
    ("rebalance-three-swaps", |_| {
        let legs = [
            SwapLeg {
                source: key(1),
                destination: key(2),
                route: vec![1; 40],
                pools: keys(20, 3),
                target: 5_000,
                min_out: 4_900,
            },
            SwapLeg {
                source: key(3),
                destination: key(4),
                route: vec![],
                pools: vec![],
                target: 0,
                min_out: 0,
            },
            SwapLeg {
                source: key(5),
                destination: key(6),
                route: vec![3; 24],
                pools: keys(30, 2),
                target: 7_000,
                min_out: 6_800,
            },
        ];
        rebalance_three_swaps(key(200), key(10), &legs)
    }),
];

// ------------------------------------------------------------------ amounts read at run time

// #region sweep-above-a-reserve
/// The caller names the reserve, never the amount.
pub fn sweep_above_a_reserve(
    template: Pubkey,
    vault: Pubkey,
    destination: Pubkey,
    reserve: u64,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    let inputs = RunInputs::new().u64(reserve).finish();
    let accounts = vec![
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false), // systemProgram
        AccountMeta::new(vault, true),                       // vault: signs, writable
        AccountMeta::new(destination, false),                // destination: writable
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion sweep-above-a-reserve

// #region forward-the-whole-token-balance
/// No inputs: the amount is whatever `source` holds when the run executes.
pub fn forward_the_whole_token_balance(
    template: Pubkey,
    source: Pubkey,
    destination: Pubkey,
    authority: Pubkey,
) -> Instruction {
    use ballista_sdk::{run_instruction, TOKEN_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    let accounts = vec![
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        AccountMeta::new(source, false),
        AccountMeta::new(destination, false),
        AccountMeta::new_readonly(authority, true),
    ];
    run_instruction(template, accounts, &[])
}
// #endregion forward-the-whole-token-balance

// #region repay-exactly-what-is-owed
/// No inputs: the template reads the debt and the borrower's balance itself.
pub fn repay_exactly_what_is_owed(
    template: Pubkey,
    loan: Pubkey,
    borrower: Pubkey,
    pool: Pubkey,
) -> Instruction {
    use ballista_sdk::{run_instruction, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    const LENDING_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let accounts = vec![
        AccountMeta::new_readonly(LENDING_PROGRAM, false),
        AccountMeta::new_readonly(loan, false),
        AccountMeta::new(borrower, true),
        AccountMeta::new(pool, false),
    ];
    run_instruction(template, accounts, &[])
}
// #endregion repay-exactly-what-is-owed

// #region split-what-arrived
pub fn split_what_arrived(
    template: Pubkey,
    vault: Pubkey,
    partner: Pubkey,
    treasury: Pubkey,
    reserve: u64,
    share_bps: u64,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    // Inputs in declaration order: reserve, then shareBps.
    let inputs = RunInputs::new().u64(reserve).u64(share_bps).finish();
    let accounts = vec![
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new(vault, true),
        AccountMeta::new(partner, false),
        AccountMeta::new(treasury, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion split-what-arrived

// ------------------------------------------------------------------ conditional calls

// #region claim-only-when-there-is-something
/// Safe to send on a schedule: with nothing pending, the run succeeds without calling claim.
pub fn claim_only_when_there_is_something(
    template: Pubkey,
    rewards: Pubkey,
    claimant: Pubkey,
    destination: Pubkey,
) -> Instruction {
    use ballista_sdk::{run_instruction, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    const REWARDS_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let accounts = vec![
        AccountMeta::new_readonly(REWARDS_PROGRAM, false),
        AccountMeta::new_readonly(rewards, false),
        AccountMeta::new(claimant, true),
        AccountMeta::new(destination, false),
    ];
    run_instruction(template, accounts, &[])
}
// #endregion claim-only-when-there-is-something

// #region liquidate-only-when-unhealthy
pub fn liquidate_only_when_unhealthy(
    template: Pubkey,
    position: Pubkey,
    liquidator: Pubkey,
    vault: Pubkey,
    threshold: u64,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    const LENDING_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let inputs = RunInputs::new().u64(threshold).finish();
    let accounts = vec![
        AccountMeta::new_readonly(LENDING_PROGRAM, false),
        AccountMeta::new_readonly(position, false),
        AccountMeta::new(liquidator, true),
        AccountMeta::new(vault, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion liquidate-only-when-unhealthy

// #region top-up-only-when-low
pub fn top_up_only_when_low(
    template: Pubkey,
    funder: Pubkey,
    bot: Pubkey,
    floor: u64,
    top_up: u64,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    let inputs = RunInputs::new().u64(floor).u64(top_up).finish();
    let accounts = vec![
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new(funder, true),
        AccountMeta::new(bot, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion top-up-only-when-low

// #region initialize-only-if-missing
/// The same instruction whether or not the position exists yet.
pub fn initialize_only_if_missing(
    template: Pubkey,
    payer: Pubkey,
    position: Pubkey,
) -> Instruction {
    use ballista_sdk::{run_instruction, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let accounts = vec![
        AccountMeta::new_readonly(PROTOCOL_PROGRAM, false),
        AccountMeta::new(payer, true),
        AccountMeta::new(position, false),
    ];
    run_instruction(template, accounts, &[])
}
// #endregion initialize-only-if-missing

// ------------------------------------------------------------------ loops

// #region waterfall-until-the-money-runs-out
/// `creditors` in priority order, each with what it is owed.
pub fn waterfall_until_the_money_runs_out(
    template: Pubkey,
    treasury: Pubkey,
    reserve: u64,
    creditors: &[(Pubkey, u64)],
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    // The fixed input first, then one `owed` value per row, in row order.
    let mut inputs = RunInputs::new().u64(reserve);
    for (_, owed) in creditors {
        inputs = inputs.u64(*owed);
    }
    let mut accounts = vec![
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new(treasury, true),
    ];
    // One row per creditor: its account, after the fixed accounts.
    accounts.extend(
        creditors
            .iter()
            .map(|(creditor, _)| AccountMeta::new(*creditor, false)),
    );
    run_instruction(template, accounts, &inputs.finish())
}
// #endregion waterfall-until-the-money-runs-out

// #region consolidate-only-the-funded-accounts
/// Pass every candidate; the run skips the empty ones.
pub fn consolidate_only_the_funded_accounts(
    template: Pubkey,
    vault: Pubkey,
    authority: Pubkey,
    sources: &[Pubkey],
) -> Instruction {
    use ballista_sdk::{run_instruction, TOKEN_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    let mut accounts = vec![
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        AccountMeta::new(vault, false),
        AccountMeta::new_readonly(authority, true),
    ];
    accounts.extend(
        sources
            .iter()
            .map(|source| AccountMeta::new(*source, false)),
    );
    run_instruction(template, accounts, &[])
}
// #endregion consolidate-only-the-funded-accounts

// #region crank-only-the-ripe-entries
/// Pass the whole queue; the run settles the entries that are due when it executes.
pub fn crank_only_the_ripe_entries(
    template: Pubkey,
    keeper: Pubkey,
    entries: &[Pubkey],
) -> Instruction {
    use ballista_sdk::{run_instruction, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    const QUEUE_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let mut accounts = vec![
        AccountMeta::new_readonly(QUEUE_PROGRAM, false),
        AccountMeta::new(keeper, true),
    ];
    accounts.extend(entries.iter().map(|entry| AccountMeta::new(*entry, false)));
    run_instruction(template, accounts, &[])
}
// #endregion crank-only-the-ripe-entries

// #region distribute-a-runtime-pot-pro-rata
/// `holders` with each one's weight in basis points.
pub fn distribute_a_runtime_pot_pro_rata(
    template: Pubkey,
    vault: Pubkey,
    reserve: u64,
    holders: &[(Pubkey, u64)],
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    let mut inputs = RunInputs::new().u64(reserve);
    for (_, weight_bps) in holders {
        inputs = inputs.u64(*weight_bps);
    }
    let mut accounts = vec![
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new(vault, true),
    ];
    accounts.extend(
        holders
            .iter()
            .map(|(holder, _)| AccountMeta::new(*holder, false)),
    );
    run_instruction(template, accounts, &inputs.finish())
}
// #endregion distribute-a-runtime-pot-pro-rata

// #region crank-once-per-waiting-entry
/// The run reads the count itself, so the keeper passes only the accounts.
pub fn crank_once_per_waiting_entry(
    template: Pubkey,
    keeper: Pubkey,
    queue: Pubkey,
) -> Instruction {
    use ballista_sdk::{run_instruction, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    const QUEUE_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let accounts = vec![
        AccountMeta::new_readonly(QUEUE_PROGRAM, false),
        AccountMeta::new(keeper, true),
        AccountMeta::new(queue, false),
    ];
    run_instruction(template, accounts, &[])
}
// #endregion crank-once-per-waiting-entry

// ------------------------------------------------------------------ payments

// #region bounded-sol-payroll
pub fn bounded_sol_payroll(
    template: Pubkey,
    treasury: Pubkey,
    recipients: &[Pubkey],
    amount: u64,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    let inputs = RunInputs::new().u64(amount).finish();
    let mut accounts = vec![
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new(treasury, true),
    ];
    // One row per recipient, after the fixed accounts.
    accounts.extend(
        recipients
            .iter()
            .map(|recipient| AccountMeta::new(*recipient, false)),
    );
    run_instruction(template, accounts, &inputs)
}
// #endregion bounded-sol-payroll

// #region basis-point-revenue-split
pub fn basis_point_revenue_split(
    template: Pubkey,
    source: Pubkey,
    partner: Pubkey,
    treasury: Pubkey,
    total: u64,
    partner_bps: u64,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    let inputs = RunInputs::new().u64(total).u64(partner_bps).finish();
    let accounts = vec![
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new(source, true),
        AccountMeta::new(partner, false),
        AccountMeta::new(treasury, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion basis-point-revenue-split

// #region index-weighted-rewards
/// The order of `recipients` sets each one's multiple of `base`.
pub fn index_weighted_rewards(
    template: Pubkey,
    treasury: Pubkey,
    recipients: &[Pubkey],
    base: u64,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    let inputs = RunInputs::new().u64(base).finish();
    let mut accounts = vec![
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new(treasury, true),
    ];
    accounts.extend(
        recipients
            .iter()
            .map(|recipient| AccountMeta::new(*recipient, false)),
    );
    run_instruction(template, accounts, &inputs)
}
// #endregion index-weighted-rewards

// #region deadline-refund
pub fn deadline_refund(
    template: Pubkey,
    escrow_authority: Pubkey,
    customer: Pubkey,
    refund_amount: u64,
    deadline: i64,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    let inputs = RunInputs::new().u64(refund_amount).i64(deadline).finish();
    let accounts = vec![
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new(escrow_authority, true),
        AccountMeta::new(customer, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion deadline-refund

// #region reserve-preserving-sweep
pub fn reserve_preserving_sweep(
    template: Pubkey,
    payer: Pubkey,
    vault: Pubkey,
    reserve: u64,
    cap: u64,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    let inputs = RunInputs::new().u64(reserve).u64(cap).finish();
    let accounts = vec![
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new(payer, true),
        AccountMeta::new(vault, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion reserve-preserving-sweep

// ------------------------------------------------------------------ token accounts

// #region assert-create-then-transfer
/// `rows` pairs each recipient's wallet with its ATA for `mint`.
pub fn assert_create_then_transfer(
    template: Pubkey,
    mint: Pubkey,
    payer: Pubkey,
    authority: Pubkey,
    source: Pubkey,
    rows: &[(Pubkey, Pubkey)],
    amount: u64,
) -> Instruction {
    use ballista_sdk::{
        run_instruction, RunInputs, ASSOCIATED_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID,
        TOKEN_PROGRAM_ID,
    };
    use solana_program::instruction::AccountMeta;

    let inputs = RunInputs::new().u64(amount).finish();
    let mut accounts = vec![
        AccountMeta::new_readonly(ASSOCIATED_TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new_readonly(mint, false),
        AccountMeta::new(payer, true),
        AccountMeta::new_readonly(authority, true),
        AccountMeta::new(source, false),
    ];
    // Each row is two accounts, in the order the row declares them.
    for (recipient, destination_ata) in rows {
        accounts.push(AccountMeta::new_readonly(*recipient, false));
        accounts.push(AccountMeta::new(*destination_ata, false));
    }
    run_instruction(template, accounts, &inputs)
}
// #endregion assert-create-then-transfer

// #region existing-account-token-payroll
pub fn existing_account_token_payroll(
    template: Pubkey,
    source: Pubkey,
    authority: Pubkey,
    destinations: &[Pubkey],
    amount: u64,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, TOKEN_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    let inputs = RunInputs::new().u64(amount).finish();
    let mut accounts = vec![
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        AccountMeta::new(source, false),
        AccountMeta::new_readonly(authority, true),
    ];
    accounts.extend(
        destinations
            .iter()
            .map(|destination| AccountMeta::new(*destination, false)),
    );
    run_instruction(template, accounts, &inputs)
}
// #endregion existing-account-token-payroll

// #region conditional-ata-setup
/// Re-sending this after the ATA exists skips `Create` instead of failing.
pub fn conditional_ata_setup(
    template: Pubkey,
    mint: Pubkey,
    payer: Pubkey,
    wallet: Pubkey,
    ata: Pubkey,
) -> Instruction {
    use ballista_sdk::{
        run_instruction, ASSOCIATED_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID,
    };
    use solana_program::instruction::AccountMeta;

    let accounts = vec![
        AccountMeta::new_readonly(ASSOCIATED_TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new_readonly(mint, false),
        AccountMeta::new(payer, true),
        AccountMeta::new_readonly(wallet, false),
        AccountMeta::new(ata, false),
    ];
    run_instruction(template, accounts, &[])
}
// #endregion conditional-ata-setup

// #region close-empty-token-accounts
/// Pass every candidate; the run closes only those holding zero tokens.
pub fn close_empty_token_accounts(
    template: Pubkey,
    rent_destination: Pubkey,
    authority: Pubkey,
    token_accounts: &[Pubkey],
) -> Instruction {
    use ballista_sdk::{run_instruction, TOKEN_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    let mut accounts = vec![
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        AccountMeta::new(rent_destination, false),
        AccountMeta::new_readonly(authority, true),
    ];
    accounts.extend(
        token_accounts
            .iter()
            .map(|account| AccountMeta::new(*account, false)),
    );
    run_instruction(template, accounts, &[])
}
// #endregion close-empty-token-accounts

// #region exact-token-debit
pub fn exact_token_debit(
    template: Pubkey,
    source: Pubkey,
    destination: Pubkey,
    authority: Pubkey,
    amount: u64,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, TOKEN_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    let inputs = RunInputs::new().u64(amount).finish();
    let accounts = vec![
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        AccountMeta::new(source, false),
        AccountMeta::new(destination, false),
        AccountMeta::new_readonly(authority, true),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion exact-token-debit

// ------------------------------------------------------------------ guardrails

// #region deadline-and-minimum-output
/// `quote` is `(deadline, quoted_out, minimum_out)`; `route_data` is the swap instruction's data.
pub fn deadline_and_minimum_output(
    template: Pubkey,
    payer: Pubkey,
    pool: Pubkey,
    quote: (i64, u64, u64),
    route_data: &[u8],
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    const SWAP_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let (deadline, quoted_out, minimum_out) = quote;
    let inputs = RunInputs::new()
        .i64(deadline)
        .u64(quoted_out)
        .u64(minimum_out)
        .bytes(route_data) // a u16 length, then the bytes
        .finish();
    let accounts = vec![
        AccountMeta::new_readonly(SWAP_PROGRAM, false),
        AccountMeta::new(payer, true),
        AccountMeta::new(pool, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion deadline-and-minimum-output

// #region pinned-program-and-owner
/// A different program, or a position with another owner, fails the run before its first step.
pub fn pinned_program_and_owner(
    template: Pubkey,
    payer: Pubkey,
    position: Pubkey,
    amount: u64,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let inputs = RunInputs::new().u64(amount).finish();
    let accounts = vec![
        AccountMeta::new_readonly(PROTOCOL_PROGRAM, false),
        AccountMeta::new(payer, true),
        AccountMeta::new(position, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion pinned-program-and-owner

// #region oracle-price-band
/// `band` is `(minimum_price, maximum_price)` in the oracle's own units.
pub fn oracle_price_band(
    template: Pubkey,
    oracle: Pubkey,
    payer: Pubkey,
    pool: Pubkey,
    band: (i64, i64),
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let inputs = RunInputs::new().i64(band.0).i64(band.1).finish();
    let accounts = vec![
        AccountMeta::new_readonly(oracle, false),
        AccountMeta::new_readonly(PROTOCOL_PROGRAM, false),
        AccountMeta::new(payer, true),
        AccountMeta::new(pool, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion oracle-price-band

// #region maximum-lamport-spend
pub fn maximum_lamport_spend(
    template: Pubkey,
    payer: Pubkey,
    pool: Pubkey,
    maximum_spend: u64,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let inputs = RunInputs::new().u64(maximum_spend).finish();
    let accounts = vec![
        AccountMeta::new_readonly(PROTOCOL_PROGRAM, false),
        AccountMeta::new(payer, true),
        AccountMeta::new(pool, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion maximum-lamport-spend

// #region canonical-position-account
/// Derives the position the same way the template checks it. `None` if the seeds have no PDA.
pub fn canonical_position_account(
    template: Pubkey,
    owner: Pubkey,
    position_id: u64,
) -> Option<Instruction> {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let position_id = position_id.to_le_bytes();
    let (position, _bump) = Pubkey::try_find_program_address(
        &[b"position", owner.as_ref(), &position_id],
        &PROTOCOL_PROGRAM,
    )?;
    let inputs = RunInputs::new().bytes(&position_id).finish(); // `positionId` is a bytes input
    let accounts = vec![
        AccountMeta::new_readonly(PROTOCOL_PROGRAM, false),
        AccountMeta::new(owner, true),
        AccountMeta::new(position, false),
    ];
    Some(run_instruction(template, accounts, &inputs))
}
// #endregion canonical-position-account

// ------------------------------------------------------------------ composition

// #region swap-then-deposit
/// `swap_data` and `deposit_data` are the two instructions' data, built by your client.
pub fn swap_then_deposit(
    template: Pubkey,
    payer: Pubkey,
    pool: Pubkey,
    received_tokens: Pubkey,
    minimum_out: u64,
    swap_data: &[u8],
    deposit_data: &[u8],
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    // The same stand-ins as the template.
    const SWAP_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const VAULT_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;

    let inputs = RunInputs::new()
        .u64(minimum_out)
        .bytes(swap_data)
        .bytes(deposit_data)
        .finish();
    let accounts = vec![
        AccountMeta::new_readonly(SWAP_PROGRAM, false),
        AccountMeta::new_readonly(VAULT_PROGRAM, false),
        AccountMeta::new(payer, true),
        AccountMeta::new(pool, false),
        AccountMeta::new(received_tokens, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion swap-then-deposit

// #region claim-then-distribute
pub fn claim_then_distribute(
    template: Pubkey,
    claimer: Pubkey,
    pool: Pubkey,
    treasury_tokens: Pubkey,
    authority: Pubkey,
    recipient_tokens: &[Pubkey],
    amount_per_recipient: u64,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    const REWARDS_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let inputs = RunInputs::new().u64(amount_per_recipient).finish();
    let mut accounts = vec![
        AccountMeta::new_readonly(REWARDS_PROGRAM, false),
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        AccountMeta::new(claimer, true),
        AccountMeta::new(pool, false),
        AccountMeta::new(treasury_tokens, false),
        AccountMeta::new_readonly(authority, true),
    ];
    accounts.extend(
        recipient_tokens
            .iter()
            .map(|account| AccountMeta::new(*account, false)),
    );
    run_instruction(template, accounts, &inputs)
}
// #endregion claim-then-distribute

// #region primary-or-fallback-route
/// Both routes' data travel in every run; `use_primary` picks which call happens.
pub fn primary_or_fallback_route(
    template: Pubkey,
    payer: Pubkey,
    pool: Pubkey,
    use_primary: bool,
    primary_data: &[u8],
    fallback_data: &[u8],
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    // The same stand-ins as the template.
    const PRIMARY_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const FALLBACK_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;

    let inputs = RunInputs::new()
        .bool(use_primary)
        .bytes(primary_data)
        .bytes(fallback_data)
        .finish();
    let accounts = vec![
        AccountMeta::new_readonly(PRIMARY_PROGRAM, false),
        AccountMeta::new_readonly(FALLBACK_PROGRAM, false),
        AccountMeta::new(payer, true),
        AccountMeta::new(pool, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion primary-or-fallback-route

// #region time-gated-governance-execution
pub fn time_gated_governance_execution(
    template: Pubkey,
    proposal: Pubkey,
    payer: Pubkey,
    target: Pubkey,
    execute_data: &[u8],
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    const GOVERNANCE_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let inputs = RunInputs::new().bytes(execute_data).finish();
    let accounts = vec![
        AccountMeta::new_readonly(GOVERNANCE_PROGRAM, false),
        AccountMeta::new_readonly(proposal, false),
        AccountMeta::new(payer, true),
        AccountMeta::new(target, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion time-gated-governance-execution

// #region bounded-keeper-crank
/// `rows` pairs each market with its queue.
pub fn bounded_keeper_crank(
    template: Pubkey,
    keeper: Pubkey,
    rows: &[(Pubkey, Pubkey)],
) -> Instruction {
    use ballista_sdk::{run_instruction, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let mut accounts = vec![
        AccountMeta::new_readonly(PROTOCOL_PROGRAM, false),
        AccountMeta::new(keeper, true),
    ];
    for (market, queue) in rows {
        accounts.push(AccountMeta::new(*market, false));
        accounts.push(AccountMeta::new(*queue, false));
    }
    run_instruction(template, accounts, &[])
}
// #endregion bounded-keeper-crank

// #region swap-and-return-what-arrived
/// Run on its own, the template returns what arrived to whoever reads the transaction's return
/// data.
pub fn swap_and_return_what_arrived(
    template: Pubkey,
    payer: Pubkey,
    pool: Pubkey,
    received_tokens: Pubkey,
    minimum_out: u64,
    swap_data: &[u8],
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    const SWAP_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let inputs = RunInputs::new().u64(minimum_out).bytes(swap_data).finish();
    let accounts = vec![
        AccountMeta::new_readonly(SWAP_PROGRAM, false),
        AccountMeta::new(payer, true),
        AccountMeta::new(pool, false),
        AccountMeta::new(received_tokens, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion swap-and-return-what-arrived

// #region nested-swap-then-deposit
/// `inner_run` is what the inner template's own run would carry: its inputs, encoded.
pub fn nested_swap_then_deposit(
    template: Pubkey,
    payer: Pubkey,
    pool: Pubkey,
    received_tokens: Pubkey,
    minimum_out: u64,
    swap_data: &[u8],
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, ID, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    // The same stand-ins as the template.
    const INNER_TEMPLATE: Pubkey = Pubkey::new_from_array([7; 32]);
    const SWAP_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const VAULT_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;

    let inner_run = RunInputs::new().u64(minimum_out).bytes(swap_data).finish();
    let inputs = RunInputs::new().bytes(&inner_run).finish();
    let accounts = vec![
        AccountMeta::new_readonly(ID, false),
        AccountMeta::new_readonly(INNER_TEMPLATE, false),
        AccountMeta::new_readonly(SWAP_PROGRAM, false),
        AccountMeta::new_readonly(VAULT_PROGRAM, false),
        AccountMeta::new(payer, true),
        AccountMeta::new(pool, false),
        AccountMeta::new(received_tokens, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion nested-swap-then-deposit

// ------------------------------------------------------------------ guide pages

// #region row-amounts
/// `payees` with each one's amount in lamports.
pub fn row_amounts(template: Pubkey, treasury: Pubkey, payees: &[(Pubkey, u64)]) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    // No fixed inputs here, so the data is one row of values per payee, in row order.
    let mut inputs = RunInputs::new();
    for (_, lamports) in payees {
        inputs = inputs.u64(*lamports);
    }
    let mut accounts = vec![
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new(treasury, true),
    ];
    accounts.extend(
        payees
            .iter()
            .map(|(payee, _)| AccountMeta::new(*payee, false)),
    );
    run_instruction(template, accounts, &inputs.finish())
}
// #endregion row-amounts

// #region budgeted-payroll
pub fn budgeted_payroll(
    template: Pubkey,
    treasury: Pubkey,
    recipients: &[Pubkey],
    amount: u64,
    budget: u64,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    let inputs = RunInputs::new().u64(amount).u64(budget).finish();
    let mut accounts = vec![
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new(treasury, true),
    ];
    accounts.extend(
        recipients
            .iter()
            .map(|recipient| AccountMeta::new(*recipient, false)),
    );
    run_instruction(template, accounts, &inputs)
}
// #endregion budgeted-payroll

// #region exact-lamport-delta
/// If the balance does not fall by exactly `amount`, the run fails and the transfer is undone.
pub fn exact_lamport_delta(
    template: Pubkey,
    sender: Pubkey,
    recipient: Pubkey,
    amount: u64,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, SYSTEM_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    let inputs = RunInputs::new().u64(amount).finish();
    let accounts = vec![
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new(sender, true),
        AccountMeta::new(recipient, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion exact-lamport-delta

// #region token-transfer
pub fn token_transfer(
    template: Pubkey,
    authority: Pubkey,
    source: Pubkey,
    destination: Pubkey,
    amount: u64,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, TOKEN_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    let inputs = RunInputs::new().u64(amount).finish();
    let accounts = vec![
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(authority, true),
        AccountMeta::new(source, false),
        AccountMeta::new(destination, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion token-transfer

// #region generic-cpi
pub fn generic_cpi(
    template: Pubkey,
    vault: Pubkey,
    authority: Pubkey,
    amount: u64,
    client_payload: &[u8],
    enabled: bool,
) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs};
    use solana_program::instruction::AccountMeta;

    const MY_PROGRAM: Pubkey = Pubkey::new_from_array([7; 32]); // the template's placeholder

    let inputs = RunInputs::new()
        .u64(amount)
        .bytes(client_payload)
        .bool(enabled)
        .finish();
    let accounts = vec![
        AccountMeta::new_readonly(MY_PROGRAM, false),
        AccountMeta::new(vault, false),
        AccountMeta::new_readonly(authority, true),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion generic-cpi

// #region rebalance-three-swaps
/// One swap: its token accounts, its quote's route data and pool accounts, and its limits. A
/// swap that is not needed passes empty route data and no pools.
pub struct SwapLeg {
    pub source: Pubkey,
    pub destination: Pubkey,
    pub route: Vec<u8>,
    pub pools: Vec<Pubkey>,
    pub target: u64,
    pub min_out: u64,
}

pub fn rebalance_three_swaps(template: Pubkey, user: Pubkey, legs: &[SwapLeg; 3]) -> Instruction {
    use ballista_sdk::{run_instruction, RunInputs, TOKEN_PROGRAM_ID};
    use solana_program::{instruction::AccountMeta, pubkey};

    const JUPITER_V6: Pubkey = pubkey!("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");

    // Group lengths come first, one byte per group, then the inputs in declaration order:
    // the three routes, the three targets, the three minimum outputs.
    let lengths: Vec<u8> = legs.iter().map(|leg| leg.pools.len() as u8).collect();
    let mut inputs = RunInputs::new().groups(&lengths);
    for leg in legs {
        inputs = inputs.bytes(&leg.route);
    }
    for leg in legs {
        inputs = inputs.u64(leg.target);
    }
    for leg in legs {
        inputs = inputs.u64(leg.min_out);
    }

    let mut accounts = vec![
        AccountMeta::new_readonly(JUPITER_V6, false),
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        AccountMeta::new(user, true),
    ];
    for leg in legs {
        accounts.push(AccountMeta::new(leg.source, false));
        accounts.push(AccountMeta::new(leg.destination, false));
    }
    // Group members follow the declared accounts, group by group. They never sign; mark each
    // writable or not as the quote says (all writable here).
    for leg in legs {
        accounts.extend(leg.pools.iter().map(|pool| AccountMeta::new(*pool, false)));
    }
    run_instruction(template, accounts, &inputs.finish())
}
// #endregion rebalance-three-swaps
