//! Build the `Run` instruction for each template from the guide and example pages.
//!
//! Each function compiles its template from `docs_templates.rs` and names every input and
//! account, as the TypeScript runs do. The compiled template puts them in the order it declares
//! them and gives each account the signer and writable flags it declares.
//!
//! `tests/docs_examples.rs` checks each function below against the run the TypeScript client
//! builds for the same example: the same run data and the same signer and writable flags.
//!
//! ```bash
//! cargo run -p ballista-sdk --example docs_runs
//! ```
//!
//! The pages include each function by its `#region` name.

#![allow(dead_code)]

#[path = "docs_templates.rs"]
pub mod templates;

use std::error::Error;

use ballista_sdk::template::Row;
use ballista_sdk::{
    find_registry_entry_address, ASSOCIATED_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID,
};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    pubkey,
    pubkey::Pubkey,
};

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

type RunResult = Result<Instruction, Box<dyn Error>>;

/// An example's name and a function that builds its run, with the example inputs, for a given
/// number of batch rows.
pub type ExampleRun = (&'static str, fn(usize) -> Instruction);

pub const ALL: &[ExampleRun] = &[
    ("sweep-above-a-reserve", |_| {
        sweep_above_a_reserve(key(200), key(1), key(2), 2_000_000).unwrap()
    }),
    ("forward-the-whole-token-balance", |_| {
        forward_the_whole_token_balance(key(200), key(1), key(2), key(3)).unwrap()
    }),
    ("split-what-arrived", |_| {
        split_what_arrived(key(200), key(1), key(2), key(3), 2_000_000, 3_000).unwrap()
    }),
    ("claim-only-when-there-is-something", |_| {
        claim_only_when_there_is_something(key(200), key(1), key(2), key(3)).unwrap()
    }),
    ("liquidate-only-when-unhealthy", |_| {
        liquidate_only_when_unhealthy(key(200), key(1), key(2), key(3), 2).unwrap()
    }),
    ("top-up-to-a-target", |_| {
        top_up_to_a_target(key(200), key(1), key(2), 2_000_000_000).unwrap()
    }),
    ("initialize-only-if-missing", |_| {
        initialize_only_if_missing(key(200), key(1), key(2)).unwrap()
    }),
    ("waterfall-until-the-money-runs-out", |rows| {
        let creditors: Vec<(Pubkey, u64)> =
            keys(10, rows).into_iter().map(|k| (k, 1_000)).collect();
        waterfall_until_the_money_runs_out(key(200), key(1), 2_000_000, &creditors).unwrap()
    }),
    ("consolidate-only-the-funded-accounts", |rows| {
        consolidate_only_the_funded_accounts(key(200), key(1), key(2), &keys(10, rows)).unwrap()
    }),
    ("crank-only-the-ripe-entries", |rows| {
        crank_only_the_ripe_entries(key(200), key(1), &keys(10, rows)).unwrap()
    }),
    ("crank-once-per-waiting-entry", |_| {
        crank_once_per_waiting_entry(key(200), key(1), key(2)).unwrap()
    }),
    ("distribute-a-runtime-pot-pro-rata", |rows| {
        let holders: Vec<(Pubkey, u64)> = keys(10, rows).into_iter().map(|k| (k, 100)).collect();
        distribute_a_runtime_pot_pro_rata(key(200), key(1), 2_000_000, &holders).unwrap()
    }),
    ("bounded-sol-payroll", |rows| {
        bounded_sol_payroll(key(200), key(1), &keys(10, rows), 10_000).unwrap()
    }),
    ("basis-point-revenue-split", |_| {
        basis_point_revenue_split(key(200), key(1), key(2), key(3), 1_000_000, 250).unwrap()
    }),
    ("index-weighted-rewards", |rows| {
        index_weighted_rewards(key(200), key(1), &keys(10, rows), 1_000).unwrap()
    }),
    ("deadline-refund", |_| {
        deadline_refund(key(200), key(1), key(2), 10_000, 9_000_000_000).unwrap()
    }),
    ("reserve-preserving-sweep", |_| {
        reserve_preserving_sweep(key(200), key(1), key(2), 1_000_000_000, 50_000).unwrap()
    }),
    ("payment-agent", |_| {
        payment_agent(key(200), key(1), key(2), 1_000).unwrap()
    }),
    ("daily-limit-per-caller", |_| {
        daily_limit_per_caller(key(200), key(1), key(2), 1_000).unwrap()
    }),
    ("listed-callers-only", |_| {
        listed_callers_only(key(200), key(1), key(2), Some((key(3), true))).unwrap()
    }),
    ("assert-recipient-ata", |_| {
        assert_recipient_ata(key(200), key(1), key(2), TOKEN_PROGRAM_ID).unwrap()
    }),
    ("assert-create-then-transfer", |rows| {
        let recipients = keys(10, rows);
        let atas = keys(100, rows);
        let rows: Vec<(Pubkey, Pubkey)> = recipients.into_iter().zip(atas).collect();
        assert_create_then_transfer(key(200), key(1), key(2), key(3), key(4), &rows, 1_000).unwrap()
    }),
    ("existing-account-token-payroll", |rows| {
        existing_account_token_payroll(key(200), key(1), key(2), &keys(10, rows), 1_000).unwrap()
    }),
    ("conditional-ata-setup", |_| {
        conditional_ata_setup(key(200), key(1), key(2), key(3), key(4)).unwrap()
    }),
    ("close-empty-token-accounts", |rows| {
        close_empty_token_accounts(key(200), key(1), key(2), &keys(10, rows)).unwrap()
    }),
    ("exact-token-debit", |_| {
        exact_token_debit(key(200), key(1), key(2), key(3), 1_000).unwrap()
    }),
    ("deadline-and-minimum-output", |_| {
        deadline_and_minimum_output(
            key(200),
            key(1),
            key(2),
            (9_000_000_000, 1_000, 900),
            &stand_in_data(),
        )
        .unwrap()
    }),
    ("pinned-program-and-owner", |_| {
        pinned_program_and_owner(key(200), key(1), key(2), 10_000).unwrap()
    }),
    ("oracle-price-band", |_| {
        oracle_price_band(key(200), key(1), key(2), key(3), (1, 1_000_000)).unwrap()
    }),
    ("maximum-lamport-spend", |_| {
        maximum_lamport_spend(key(200), key(1), key(2), 1_000_000).unwrap()
    }),
    ("canonical-position-account", |_| {
        canonical_position_account(key(200), key(1), 7).unwrap()
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
        .unwrap()
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
        .unwrap()
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
        .unwrap()
    }),
    ("time-gated-governance-execution", |_| {
        time_gated_governance_execution(key(200), key(1), key(2), key(3), &stand_in_data()).unwrap()
    }),
    ("bounded-keeper-crank", |rows| {
        let markets = keys(10, rows);
        let queues = keys(100, rows);
        let rows: Vec<(Pubkey, Pubkey)> = markets.into_iter().zip(queues).collect();
        bounded_keeper_crank(key(200), key(1), &rows).unwrap()
    }),
    ("swap-and-return-what-arrived", |_| {
        swap_and_return_what_arrived(key(200), key(1), key(2), key(3), 900, &stand_in_data())
            .unwrap()
    }),
    ("nested-swap-then-deposit", |_| {
        nested_swap_then_deposit(key(200), key(1), key(2), key(3), 900, &stand_in_data()).unwrap()
    }),
    ("row-amounts", |rows| {
        let payees: Vec<(Pubkey, u64)> = keys(10, rows)
            .into_iter()
            .enumerate()
            .map(|(i, k)| (k, 10_000 * (i as u64 + 1)))
            .collect();
        row_amounts(key(200), key(1), &payees).unwrap()
    }),
    ("budgeted-payroll", |rows| {
        budgeted_payroll(key(200), key(1), &keys(10, rows), 10_000, 250_000).unwrap()
    }),
    ("exact-lamport-delta", |_| {
        exact_lamport_delta(key(200), key(1), key(2), 50_000_000).unwrap()
    }),
    ("token-transfer", |_| {
        token_transfer(key(200), key(1), key(2), key(3), 25_000).unwrap()
    }),
    ("generic-cpi", |_| {
        generic_cpi(key(200), key(1), key(2), 25_000, &[9, 9, 9], true).unwrap()
    }),
    ("swap-through-a-checked-route", |_| {
        let pools = keys(20, 3)
            .into_iter()
            .map(|pool| AccountMeta::new(pool, false))
            .collect();
        swap_through_a_checked_route(key(200), key(10), key(1), key(2), vec![1; 40], pools).unwrap()
    }),
    ("rebalance-three-swaps", |_| {
        let legs = [
            SwapLeg {
                source: key(1),
                destination: key(2),
                route: vec![1; 40],
                pools: keys(20, 3)
                    .into_iter()
                    .map(|pool| AccountMeta::new(pool, false))
                    .collect(),
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
                pools: keys(30, 2)
                    .into_iter()
                    .map(|pool| AccountMeta::new(pool, false))
                    .collect(),
                target: 7_000,
                min_out: 6_800,
            },
        ];
        rebalance_three_swaps(key(200), key(10), &legs).unwrap()
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
) -> RunResult {
    let instruction = templates::sweep_above_a_reserve()
        .compile()?
        .run(template)
        .input("reserve", reserve)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("vault", vault) // signs: the template declares it a signer
        .account("destination", destination)
        .instruction()?;
    Ok(instruction)
}
// #endregion sweep-above-a-reserve

// #region forward-the-whole-token-balance
/// No inputs: the amount is whatever `source` holds when the run executes.
pub fn forward_the_whole_token_balance(
    template: Pubkey,
    source: Pubkey,
    destination: Pubkey,
    authority: Pubkey,
) -> RunResult {
    let instruction = templates::forward_the_whole_token_balance()
        .compile()?
        .run(template)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("source", source)
        .account("destination", destination)
        .account("authority", authority)
        .instruction()?;
    Ok(instruction)
}
// #endregion forward-the-whole-token-balance

// #region split-what-arrived
pub fn split_what_arrived(
    template: Pubkey,
    vault: Pubkey,
    partner: Pubkey,
    treasury: Pubkey,
    reserve: u64,
    share_bps: u64,
) -> RunResult {
    let instruction = templates::split_what_arrived()
        .compile()?
        .run(template)
        .input("reserve", reserve)
        .input("shareBps", share_bps)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("vault", vault)
        .account("partner", partner)
        .account("treasury", treasury)
        .instruction()?;
    Ok(instruction)
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
) -> RunResult {
    const REWARDS_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let instruction = templates::claim_only_when_there_is_something()
        .compile()?
        .run(template)
        .account("rewardsProgram", REWARDS_PROGRAM)
        .account("rewards", rewards)
        .account("claimant", claimant)
        .account("destination", destination)
        .instruction()?;
    Ok(instruction)
}
// #endregion claim-only-when-there-is-something

// #region liquidate-only-when-unhealthy
pub fn liquidate_only_when_unhealthy(
    template: Pubkey,
    position: Pubkey,
    liquidator: Pubkey,
    vault: Pubkey,
    threshold: u64,
) -> RunResult {
    const LENDING_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let instruction = templates::liquidate_only_when_unhealthy()
        .compile()?
        .run(template)
        .input("threshold", threshold)
        .account("lendingProgram", LENDING_PROGRAM)
        .account("position", position)
        .account("liquidator", liquidator)
        .account("vault", vault)
        .instruction()?;
    Ok(instruction)
}
// #endregion liquidate-only-when-unhealthy

// #region top-up-to-a-target
pub fn top_up_to_a_target(template: Pubkey, funder: Pubkey, bot: Pubkey, target: u64) -> RunResult {
    let instruction = templates::top_up_to_a_target()
        .compile()?
        .run(template)
        .input("target", target)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("funder", funder)
        .account("bot", bot)
        .instruction()?;
    Ok(instruction)
}
// #endregion top-up-to-a-target

// #region initialize-only-if-missing
/// The same instruction whether or not the position exists yet.
pub fn initialize_only_if_missing(template: Pubkey, payer: Pubkey, position: Pubkey) -> RunResult {
    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let instruction = templates::initialize_only_if_missing()
        .compile()?
        .run(template)
        .account("protocolProgram", PROTOCOL_PROGRAM)
        .account("payer", payer)
        .account("position", position)
        .instruction()?;
    Ok(instruction)
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
) -> RunResult {
    let instruction = templates::waterfall_until_the_money_runs_out()
        .compile()?
        .run(template)
        .input("reserve", reserve)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("treasury", treasury)
        // One row per creditor: its account and what it is owed.
        .rows(creditors.iter().map(|(creditor, owed)| {
            Row::new()
                .account("creditor", *creditor)
                .input("owed", *owed)
        }))
        .instruction()?;
    Ok(instruction)
}
// #endregion waterfall-until-the-money-runs-out

// #region consolidate-only-the-funded-accounts
/// Pass every candidate; the run skips the empty ones.
pub fn consolidate_only_the_funded_accounts(
    template: Pubkey,
    vault: Pubkey,
    authority: Pubkey,
    sources: &[Pubkey],
) -> RunResult {
    let instruction = templates::consolidate_only_the_funded_accounts()
        .compile()?
        .run(template)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("vault", vault)
        .account("authority", authority)
        .rows(
            sources
                .iter()
                .map(|source| Row::new().account("source", *source)),
        )
        .instruction()?;
    Ok(instruction)
}
// #endregion consolidate-only-the-funded-accounts

// #region crank-only-the-ripe-entries
/// Pass the whole queue; the run settles the entries that are due when it executes.
pub fn crank_only_the_ripe_entries(
    template: Pubkey,
    keeper: Pubkey,
    entries: &[Pubkey],
) -> RunResult {
    const QUEUE_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let instruction = templates::crank_only_the_ripe_entries()
        .compile()?
        .run(template)
        .account("queueProgram", QUEUE_PROGRAM)
        .account("keeper", keeper)
        .rows(
            entries
                .iter()
                .map(|entry| Row::new().account("entry", *entry)),
        )
        .instruction()?;
    Ok(instruction)
}
// #endregion crank-only-the-ripe-entries

// #region distribute-a-runtime-pot-pro-rata
/// `holders` with each one's weight in basis points.
pub fn distribute_a_runtime_pot_pro_rata(
    template: Pubkey,
    vault: Pubkey,
    reserve: u64,
    holders: &[(Pubkey, u64)],
) -> RunResult {
    let instruction = templates::distribute_a_runtime_pot_pro_rata()
        .compile()?
        .run(template)
        .input("reserve", reserve)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("vault", vault)
        .rows(holders.iter().map(|(holder, weight_bps)| {
            Row::new()
                .account("holder", *holder)
                .input("weightBps", *weight_bps)
        }))
        .instruction()?;
    Ok(instruction)
}
// #endregion distribute-a-runtime-pot-pro-rata

// #region crank-once-per-waiting-entry
/// The run reads the count itself, so the keeper passes only the accounts.
pub fn crank_once_per_waiting_entry(template: Pubkey, keeper: Pubkey, queue: Pubkey) -> RunResult {
    const QUEUE_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let instruction = templates::crank_once_per_waiting_entry()
        .compile()?
        .run(template)
        .account("queueProgram", QUEUE_PROGRAM)
        .account("keeper", keeper)
        .account("queue", queue)
        .instruction()?;
    Ok(instruction)
}
// #endregion crank-once-per-waiting-entry

// ------------------------------------------------------------------ payments

// #region bounded-sol-payroll
pub fn bounded_sol_payroll(
    template: Pubkey,
    treasury: Pubkey,
    recipients: &[Pubkey],
    amount: u64,
) -> RunResult {
    let instruction = templates::bounded_sol_payroll()
        .compile()?
        .run(template)
        .input("amount", amount)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("treasury", treasury)
        // One row per recipient, after the fixed accounts.
        .rows(
            recipients
                .iter()
                .map(|recipient| Row::new().account("recipient", *recipient)),
        )
        .instruction()?;
    Ok(instruction)
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
) -> RunResult {
    let instruction = templates::basis_point_revenue_split()
        .compile()?
        .run(template)
        .input("total", total)
        .input("partnerBps", partner_bps)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("source", source)
        .account("partner", partner)
        .account("treasury", treasury)
        .instruction()?;
    Ok(instruction)
}
// #endregion basis-point-revenue-split

// #region index-weighted-rewards
/// The order of `recipients` sets each one's multiple of `base`.
pub fn index_weighted_rewards(
    template: Pubkey,
    treasury: Pubkey,
    recipients: &[Pubkey],
    base: u64,
) -> RunResult {
    let instruction = templates::index_weighted_rewards()
        .compile()?
        .run(template)
        .input("base", base)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("treasury", treasury)
        .rows(
            recipients
                .iter()
                .map(|recipient| Row::new().account("recipient", *recipient)),
        )
        .instruction()?;
    Ok(instruction)
}
// #endregion index-weighted-rewards

// #region deadline-refund
pub fn deadline_refund(
    template: Pubkey,
    escrow_authority: Pubkey,
    customer: Pubkey,
    refund_amount: u64,
    deadline: i64,
) -> RunResult {
    let instruction = templates::deadline_refund()
        .compile()?
        .run(template)
        .input("refundAmount", refund_amount)
        .input("deadline", deadline)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("escrowAuthority", escrow_authority)
        .account("customer", customer)
        .instruction()?;
    Ok(instruction)
}
// #endregion deadline-refund

// #region reserve-preserving-sweep
pub fn reserve_preserving_sweep(
    template: Pubkey,
    payer: Pubkey,
    vault: Pubkey,
    reserve: u64,
    cap: u64,
) -> RunResult {
    let instruction = templates::reserve_preserving_sweep()
        .compile()?
        .run(template)
        .input("reserve", reserve)
        .input("cap", cap)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("payer", payer)
        .account("vault", vault)
        .instruction()?;
    Ok(instruction)
}
// #endregion reserve-preserving-sweep

// #region payment-agent
pub fn payment_agent(template: Pubkey, agent: Pubkey, recipient: Pubkey, amount: u64) -> RunResult {
    let compiled = templates::payment_agent().compile()?;
    // The agent's entry in `limits`, keyed by the agent's address.
    let limits = compiled.registry_index("limits").unwrap();
    let (agent_limit, _) = find_registry_entry_address(&template, limits, &agent.to_bytes());
    let instruction = compiled
        .run(template)
        .input("amount", amount)
        .account("agent", agent)
        .account("recipient", recipient)
        .account("agentLimit", agent_limit)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .instruction()?;
    Ok(instruction)
}
// #endregion payment-agent

// ------------------------------------------------------------------ registries

// #region daily-limit-per-caller
pub fn daily_limit_per_caller(
    template: Pubkey,
    caller: Pubkey,
    recipient: Pubkey,
    amount: u64,
) -> RunResult {
    let compiled = templates::daily_limit_per_caller().compile()?;
    // The caller's entry in `limits`, keyed by the caller's address.
    let limits = compiled.registry_index("limits").unwrap();
    let (caller_limit, _) = find_registry_entry_address(&template, limits, &caller.to_bytes());
    let instruction = compiled
        .run(template)
        .input("amount", amount)
        .account("caller", caller)
        .account("recipient", recipient)
        .account("callerLimit", caller_limit)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .instruction()?;
    Ok(instruction)
}
// #endregion daily-limit-per-caller

// #region listed-callers-only
/// `set` is for the author's runs only: the member to add or remove, and the flag to set.
pub fn listed_callers_only(
    template: Pubkey,
    caller: Pubkey,
    pool: Pubkey,
    set: Option<(Pubkey, bool)>,
) -> RunResult {
    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in

    let compiled = templates::listed_callers_only().compile()?;
    // The key the template computes: the member in the author's runs, the caller in everyone else's.
    let (member, allow) = set.unwrap_or((caller, false));
    let allowed = compiled.registry_index("allowed").unwrap();
    let (entry, _) = find_registry_entry_address(&template, allowed, &member.to_bytes());
    let instruction = compiled
        .run(template)
        // Every run passes both inputs. Only the author's runs read them.
        .input("member", member)
        .input("allow", allow)
        .account("caller", caller)
        .account("entry", entry)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("protocolProgram", PROTOCOL_PROGRAM)
        .account("pool", pool)
        .instruction()?;
    Ok(instruction)
}
// #endregion listed-callers-only

// ------------------------------------------------------------------ token accounts

// #region assert-recipient-ata
/// Derive the ATA as the template does, and name each account the template declares.
pub fn assert_recipient_ata(
    template: Pubkey,
    recipient: Pubkey,
    mint: Pubkey,
    token_program: Pubkey,
) -> RunResult {
    let (destination_ata, _bump) = Pubkey::find_program_address(
        &[recipient.as_ref(), token_program.as_ref(), mint.as_ref()],
        &ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    let instruction = templates::assert_recipient_ata()
        .compile()?
        .run(template)
        .account("associatedTokenProgram", ASSOCIATED_TOKEN_PROGRAM_ID)
        .account("tokenProgram", token_program)
        .account("recipient", recipient)
        .account("mint", mint)
        .account("destinationAta", destination_ata)
        .instruction()?;
    Ok(instruction)
}
// #endregion assert-recipient-ata

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
) -> RunResult {
    let instruction = templates::assert_create_then_transfer()
        .compile()?
        .run(template)
        .input("amount", amount)
        .account("associatedTokenProgram", ASSOCIATED_TOKEN_PROGRAM_ID)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("mint", mint)
        .account("payer", payer)
        .account("authority", authority)
        .account("source", source)
        .rows(rows.iter().map(|(recipient, destination_ata)| {
            Row::new()
                .account("recipient", *recipient)
                .account("destinationAta", *destination_ata)
        }))
        .instruction()?;
    Ok(instruction)
}
// #endregion assert-create-then-transfer

// #region existing-account-token-payroll
pub fn existing_account_token_payroll(
    template: Pubkey,
    source: Pubkey,
    authority: Pubkey,
    destinations: &[Pubkey],
    amount: u64,
) -> RunResult {
    let instruction = templates::existing_account_token_payroll()
        .compile()?
        .run(template)
        .input("amount", amount)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("source", source)
        .account("authority", authority)
        .rows(
            destinations
                .iter()
                .map(|destination| Row::new().account("destination", *destination)),
        )
        .instruction()?;
    Ok(instruction)
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
) -> RunResult {
    let instruction = templates::conditional_ata_setup()
        .compile()?
        .run(template)
        .account("associatedTokenProgram", ASSOCIATED_TOKEN_PROGRAM_ID)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("mint", mint)
        .account("payer", payer)
        .account("wallet", wallet)
        .account("ata", ata)
        .instruction()?;
    Ok(instruction)
}
// #endregion conditional-ata-setup

// #region close-empty-token-accounts
/// Pass every candidate; the run closes only those holding zero tokens.
pub fn close_empty_token_accounts(
    template: Pubkey,
    rent_destination: Pubkey,
    authority: Pubkey,
    token_accounts: &[Pubkey],
) -> RunResult {
    let instruction = templates::close_empty_token_accounts()
        .compile()?
        .run(template)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("rentDestination", rent_destination)
        .account("authority", authority)
        .rows(
            token_accounts
                .iter()
                .map(|account| Row::new().account("tokenAccount", *account)),
        )
        .instruction()?;
    Ok(instruction)
}
// #endregion close-empty-token-accounts

// #region exact-token-debit
pub fn exact_token_debit(
    template: Pubkey,
    source: Pubkey,
    destination: Pubkey,
    authority: Pubkey,
    amount: u64,
) -> RunResult {
    let instruction = templates::exact_token_debit()
        .compile()?
        .run(template)
        .input("amount", amount)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("source", source)
        .account("destination", destination)
        .account("authority", authority)
        .instruction()?;
    Ok(instruction)
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
) -> RunResult {
    const SWAP_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let (deadline, quoted_out, minimum_out) = quote;
    let instruction = templates::deadline_and_minimum_output()
        .compile()?
        .run(template)
        .input("deadline", deadline)
        .input("quotedOut", quoted_out)
        .input("minimumOut", minimum_out)
        .input("routeData", route_data) // a u16 length, then the bytes
        .account("swapProgram", SWAP_PROGRAM)
        .account("payer", payer)
        .account("pool", pool)
        .instruction()?;
    Ok(instruction)
}
// #endregion deadline-and-minimum-output

// #region pinned-program-and-owner
/// A different program, or a position with another owner, fails the run before its first step.
pub fn pinned_program_and_owner(
    template: Pubkey,
    payer: Pubkey,
    position: Pubkey,
    amount: u64,
) -> RunResult {
    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let instruction = templates::pinned_program_and_owner()
        .compile()?
        .run(template)
        .input("amount", amount)
        .account("protocolProgram", PROTOCOL_PROGRAM)
        .account("payer", payer)
        .account("position", position)
        .instruction()?;
    Ok(instruction)
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
) -> RunResult {
    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let instruction = templates::oracle_price_band()
        .compile()?
        .run(template)
        .input("minimumPrice", band.0)
        .input("maximumPrice", band.1)
        .account("oracle", oracle)
        .account("protocolProgram", PROTOCOL_PROGRAM)
        .account("payer", payer)
        .account("pool", pool)
        .instruction()?;
    Ok(instruction)
}
// #endregion oracle-price-band

// #region maximum-lamport-spend
pub fn maximum_lamport_spend(
    template: Pubkey,
    payer: Pubkey,
    pool: Pubkey,
    maximum_spend: u64,
) -> RunResult {
    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let instruction = templates::maximum_lamport_spend()
        .compile()?
        .run(template)
        .input("maximumSpend", maximum_spend)
        .account("protocolProgram", PROTOCOL_PROGRAM)
        .account("payer", payer)
        .account("pool", pool)
        .instruction()?;
    Ok(instruction)
}
// #endregion maximum-lamport-spend

// #region canonical-position-account
/// Derives the position the same way the template checks it.
pub fn canonical_position_account(template: Pubkey, owner: Pubkey, position_id: u64) -> RunResult {
    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let position_id = position_id.to_le_bytes();
    let (position, _bump) = Pubkey::find_program_address(
        &[b"position", owner.as_ref(), &position_id],
        &PROTOCOL_PROGRAM,
    );
    let instruction = templates::canonical_position_account()
        .compile()?
        .run(template)
        .input("positionId", position_id) // a bytes input
        .account("protocolProgram", PROTOCOL_PROGRAM)
        .account("owner", owner)
        .account("position", position)
        .instruction()?;
    Ok(instruction)
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
) -> RunResult {
    // The same stand-ins as the template.
    const SWAP_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const VAULT_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;

    let instruction = templates::swap_then_deposit()
        .compile()?
        .run(template)
        .input("minimumOut", minimum_out)
        .input("swapData", swap_data)
        .input("depositData", deposit_data)
        .account("swapProgram", SWAP_PROGRAM)
        .account("vaultProgram", VAULT_PROGRAM)
        .account("payer", payer)
        .account("pool", pool)
        .account("receivedTokens", received_tokens)
        .instruction()?;
    Ok(instruction)
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
) -> RunResult {
    const REWARDS_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let instruction = templates::claim_then_distribute()
        .compile()?
        .run(template)
        .input("amountPerRecipient", amount_per_recipient)
        .account("rewardsProgram", REWARDS_PROGRAM)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("claimer", claimer)
        .account("pool", pool)
        .account("treasuryTokens", treasury_tokens)
        .account("authority", authority)
        .rows(
            recipient_tokens
                .iter()
                .map(|account| Row::new().account("recipientTokens", *account)),
        )
        .instruction()?;
    Ok(instruction)
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
) -> RunResult {
    // The same stand-ins as the template.
    const PRIMARY_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const FALLBACK_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;

    let instruction = templates::primary_or_fallback_route()
        .compile()?
        .run(template)
        .input("usePrimary", use_primary)
        .input("primaryData", primary_data)
        .input("fallbackData", fallback_data)
        .account("primaryProgram", PRIMARY_PROGRAM)
        .account("fallbackProgram", FALLBACK_PROGRAM)
        .account("payer", payer)
        .account("pool", pool)
        .instruction()?;
    Ok(instruction)
}
// #endregion primary-or-fallback-route

// #region time-gated-governance-execution
pub fn time_gated_governance_execution(
    template: Pubkey,
    proposal: Pubkey,
    payer: Pubkey,
    target: Pubkey,
    execute_data: &[u8],
) -> RunResult {
    const GOVERNANCE_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let instruction = templates::time_gated_governance_execution()
        .compile()?
        .run(template)
        .input("executeData", execute_data)
        .account("governanceProgram", GOVERNANCE_PROGRAM)
        .account("proposal", proposal)
        .account("payer", payer)
        .account("target", target)
        .instruction()?;
    Ok(instruction)
}
// #endregion time-gated-governance-execution

// #region bounded-keeper-crank
/// `rows` pairs each market with its queue.
pub fn bounded_keeper_crank(
    template: Pubkey,
    keeper: Pubkey,
    rows: &[(Pubkey, Pubkey)],
) -> RunResult {
    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let instruction = templates::bounded_keeper_crank()
        .compile()?
        .run(template)
        .account("protocolProgram", PROTOCOL_PROGRAM)
        .account("keeper", keeper)
        .rows(rows.iter().map(|(market, queue)| {
            Row::new()
                .account("market", *market)
                .account("queue", *queue)
        }))
        .instruction()?;
    Ok(instruction)
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
) -> RunResult {
    const SWAP_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in as the template

    let instruction = templates::swap_and_return_what_arrived()
        .compile()?
        .run(template)
        .input("minimumOut", minimum_out)
        .input("swapData", swap_data)
        .account("swapProgram", SWAP_PROGRAM)
        .account("payer", payer)
        .account("pool", pool)
        .account("receivedTokens", received_tokens)
        .instruction()?;
    Ok(instruction)
}
// #endregion swap-and-return-what-arrived

// #region nested-swap-then-deposit
/// `innerRun` is what the inner template's own run would carry: its inputs, encoded.
pub fn nested_swap_then_deposit(
    template: Pubkey,
    payer: Pubkey,
    pool: Pubkey,
    received_tokens: Pubkey,
    minimum_out: u64,
    swap_data: &[u8],
) -> RunResult {
    // The same stand-ins as the template.
    const INNER_TEMPLATE: Pubkey = Pubkey::new_from_array([7; 32]);
    const SWAP_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;
    const VAULT_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID;

    let inner_run = templates::swap_and_return_what_arrived()
        .compile()?
        .run_inputs()
        .input("minimumOut", minimum_out)
        .input("swapData", swap_data)
        .encode_inputs()?;
    let instruction = templates::nested_swap_then_deposit()
        .compile()?
        .run(template)
        .input("innerRun", inner_run)
        .account("ballista", ballista_sdk::ID)
        .account("innerTemplate", INNER_TEMPLATE)
        .account("swapProgram", SWAP_PROGRAM)
        .account("vaultProgram", VAULT_PROGRAM)
        .account("payer", payer)
        .account("pool", pool)
        .account("receivedTokens", received_tokens)
        .instruction()?;
    Ok(instruction)
}
// #endregion nested-swap-then-deposit

// ------------------------------------------------------------------ guide pages

// #region row-amounts
/// `payees` with each one's amount in lamports.
pub fn row_amounts(template: Pubkey, treasury: Pubkey, payees: &[(Pubkey, u64)]) -> RunResult {
    let instruction = templates::row_amounts()
        .compile()?
        .run(template)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("treasury", treasury)
        .rows(payees.iter().map(|(payee, lamports)| {
            Row::new()
                .account("recipient", *payee)
                .input("amount", *lamports)
        }))
        .instruction()?;
    Ok(instruction)
}
// #endregion row-amounts

// #region budgeted-payroll
pub fn budgeted_payroll(
    template: Pubkey,
    treasury: Pubkey,
    recipients: &[Pubkey],
    amount: u64,
    budget: u64,
) -> RunResult {
    let instruction = templates::budgeted_payroll()
        .compile()?
        .run(template)
        .input("amount", amount)
        .input("budget", budget)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("treasury", treasury)
        .rows(
            recipients
                .iter()
                .map(|recipient| Row::new().account("recipient", *recipient)),
        )
        .instruction()?;
    Ok(instruction)
}
// #endregion budgeted-payroll

// #region exact-lamport-delta
/// If the balance does not fall by exactly `amount`, the run fails and the transfer is undone.
pub fn exact_lamport_delta(
    template: Pubkey,
    sender: Pubkey,
    recipient: Pubkey,
    amount: u64,
) -> RunResult {
    let instruction = templates::exact_lamport_delta()
        .compile()?
        .run(template)
        .input("amount", amount)
        .account("systemProgram", SYSTEM_PROGRAM_ID)
        .account("sender", sender)
        .account("recipient", recipient)
        .instruction()?;
    Ok(instruction)
}
// #endregion exact-lamport-delta

// #region token-transfer
pub fn token_transfer(
    template: Pubkey,
    authority: Pubkey,
    source: Pubkey,
    destination: Pubkey,
    amount: u64,
) -> RunResult {
    let instruction = templates::token_transfer_template()
        .compile()?
        .run(template)
        .input("amount", amount)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("authority", authority)
        .account("source", source)
        .account("destination", destination)
        .instruction()?;
    Ok(instruction)
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
) -> RunResult {
    const MY_PROGRAM: Pubkey = Pubkey::new_from_array([7; 32]); // the template's placeholder

    let instruction = templates::generic_cpi()
        .compile()?
        .run(template)
        .input("amount", amount)
        .input("clientPayload", client_payload)
        .input("enabled", enabled)
        .account("program", MY_PROGRAM)
        .account("vault", vault)
        .account("authority", authority)
        .instruction()?;
    Ok(instruction)
}
// #endregion generic-cpi

// #region swap-through-a-checked-route
/// `route` and `pools` are the quote's route data and pool accounts, each pool writable or not as
/// the quote lists it.
pub fn swap_through_a_checked_route(
    template: Pubkey,
    user: Pubkey,
    source: Pubkey,
    destination: Pubkey,
    route: Vec<u8>,
    pools: Vec<AccountMeta>,
) -> RunResult {
    const JUPITER_V6: Pubkey = pubkey!("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");

    let instruction = templates::swap_through_a_checked_route()
        .compile()?
        .run(template)
        .account("jupiter", JUPITER_V6)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("user", user)
        .account("source", source)
        .account("destination", destination)
        .input("route", route)
        .group("amm", pools)
        .instruction()?;
    Ok(instruction)
}
// #endregion swap-through-a-checked-route

// #region rebalance-three-swaps
/// One swap: its token accounts, its quote's route data and pool accounts, and its limits. A
/// swap that is not needed passes empty route data and no pools.
pub struct SwapLeg {
    pub source: Pubkey,
    pub destination: Pubkey,
    pub route: Vec<u8>,
    /// Writable or not as the quote lists each one. Members never sign.
    pub pools: Vec<AccountMeta>,
    pub target: u64,
    pub min_out: u64,
}

pub fn rebalance_three_swaps(template: Pubkey, user: Pubkey, legs: &[SwapLeg; 3]) -> RunResult {
    const JUPITER_V6: Pubkey = pubkey!("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");

    let [a, b, c] = legs;
    let instruction = templates::rebalance_three_swaps()
        .compile()?
        .run(template)
        .account("jupiter", JUPITER_V6)
        .account("tokenProgram", TOKEN_PROGRAM_ID)
        .account("user", user)
        .account("sourceA", a.source)
        .account("destinationA", a.destination)
        .account("sourceB", b.source)
        .account("destinationB", b.destination)
        .account("sourceC", c.source)
        .account("destinationC", c.destination)
        .input("routeA", &a.route)
        .input("routeB", &b.route)
        .input("routeC", &c.route)
        .input("targetA", a.target)
        .input("targetB", b.target)
        .input("targetC", c.target)
        .input("minOutA", a.min_out)
        .input("minOutB", b.min_out)
        .input("minOutC", c.min_out)
        .group("ammA", a.pools.clone())
        .group("ammB", b.pools.clone())
        .group("ammC", c.pools.clone())
        .instruction()?;
    Ok(instruction)
}
// #endregion rebalance-three-swaps
