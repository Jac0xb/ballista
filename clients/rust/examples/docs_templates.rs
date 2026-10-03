//! The templates from the guide and example pages, authored in Rust with `ProgramBuilder`.
//!
//! Each function builds the same bytes the TypeScript compiler produces for the example of the
//! same name in `clients/js/examples/docs/`. `tests/docs_examples.rs` checks every one: against
//! `fixtures/benchmarks.json` for the examples the benchmarks measure, and against
//! `tests/fixtures/docs-examples.json` for the rest.
//!
//! ```bash
//! cargo run -p ballista-sdk --example docs_templates
//! ```
//!
//! The builder emits exactly what it is told, in the order it is told. To match the TypeScript
//! compiler, each function follows the compiler's order: accounts and inputs in declaration order,
//! then a load of each input the steps use, then each distinct constant, then the steps. Registers
//! are numbered in the order they are written. The compiler never shares literal CPI data, so two
//! calls with the same discriminator each store their own copy with `blob`.
//!
//! The pages include each function by its `#region` name.

#![allow(dead_code)]

fn main() {
    for (name, build) in ALL {
        let payload = build();
        let program = ballista_sdk::ballista_common::template::ProgramView::parse(&payload)
            .expect("payload parses");
        let stats = program.verify().expect("payload verifies");
        println!(
            "{name:40} {:4} bytes  {:3} instructions  {:2} registers",
            payload.len(),
            stats.instructions,
            stats.registers
        );
    }
}

/// Every template in this file, by the example name the docs and fixtures use.
/// An example's name and the function that builds its template.
pub type Example = (&'static str, fn() -> Vec<u8>);

pub const ALL: &[Example] = &[
    ("sweep-above-a-reserve", sweep_above_a_reserve),
    (
        "forward-the-whole-token-balance",
        forward_the_whole_token_balance,
    ),
    ("repay-exactly-what-is-owed", repay_exactly_what_is_owed),
    ("split-what-arrived", split_what_arrived),
    (
        "claim-only-when-there-is-something",
        claim_only_when_there_is_something,
    ),
    (
        "liquidate-only-when-unhealthy",
        liquidate_only_when_unhealthy,
    ),
    ("top-up-only-when-low", top_up_only_when_low),
    ("initialize-only-if-missing", initialize_only_if_missing),
    (
        "waterfall-until-the-money-runs-out",
        waterfall_until_the_money_runs_out,
    ),
    (
        "consolidate-only-the-funded-accounts",
        consolidate_only_the_funded_accounts,
    ),
    ("crank-only-the-ripe-entries", crank_only_the_ripe_entries),
    ("crank-once-per-waiting-entry", crank_once_per_waiting_entry),
    (
        "distribute-a-runtime-pot-pro-rata",
        distribute_a_runtime_pot_pro_rata,
    ),
    ("bounded-sol-payroll", bounded_sol_payroll),
    ("basis-point-revenue-split", basis_point_revenue_split),
    ("index-weighted-rewards", index_weighted_rewards),
    ("deadline-refund", deadline_refund),
    ("reserve-preserving-sweep", reserve_preserving_sweep),
    ("assert-create-then-transfer", assert_create_then_transfer),
    (
        "existing-account-token-payroll",
        existing_account_token_payroll,
    ),
    ("conditional-ata-setup", conditional_ata_setup),
    ("close-empty-token-accounts", close_empty_token_accounts),
    ("exact-token-debit", exact_token_debit),
    ("deadline-and-minimum-output", deadline_and_minimum_output),
    ("pinned-program-and-owner", pinned_program_and_owner),
    ("oracle-price-band", oracle_price_band),
    ("maximum-lamport-spend", maximum_lamport_spend),
    ("canonical-position-account", canonical_position_account),
    ("swap-then-deposit", swap_then_deposit),
    ("claim-then-distribute", claim_then_distribute),
    ("primary-or-fallback-route", primary_or_fallback_route),
    (
        "time-gated-governance-execution",
        time_gated_governance_execution,
    ),
    ("bounded-keeper-crank", bounded_keeper_crank),
    ("row-amounts", row_amounts),
    ("budgeted-payroll", budgeted_payroll),
    ("exact-lamport-delta", exact_lamport_delta),
    ("token-transfer", token_transfer),
    ("generic-cpi", generic_cpi),
    ("rebalance-three-swaps", rebalance_three_swaps),
];

// ------------------------------------------------------------------ amounts read at run time

// #region sweep-above-a-reserve
/// Move everything above `reserve` from the vault to the destination.
pub fn sweep_above_a_reserve() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    let mut builder = ProgramBuilder::new();
    let system_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let vault = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let destination = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let reserve_input = builder.input(VALUE_U64, 0);

    let reserve = builder.load_input(reserve_input);
    let balance = builder.account_lamports(vault);
    let above_reserve = builder.binary(OP_GT, balance, reserve);
    builder.require(above_reserve);

    let lamports = builder.binary(OP_SUB, balance, reserve);
    let transfer_ix = builder.blob(&[2, 0, 0, 0]); // System Program Transfer
    let transfer = builder.cpi(
        system_program,
        &[
            (vault, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (destination, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(transfer_ix),
            Segment::Register(DATA_REG_U64, lamports),
        ],
    );
    builder.invoke(transfer, None);
    builder.build().expect("template builds")
}
// #endregion sweep-above-a-reserve

// #region forward-the-whole-token-balance
/// Move a token account's entire balance, read during the run, to another token account.
pub fn forward_the_whole_token_balance() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, TOKEN_PROGRAM_ID};

    const TOKEN_ACCOUNT_LEN: u32 = 165;
    const AMOUNT_OFFSET: u64 = 64; // SPL Token account: amount is the u64 at byte 64

    let mut builder = ProgramBuilder::new();
    let token = Some(TOKEN_PROGRAM_ID.to_bytes());
    let token_program = builder.account(ACCOUNT_EXECUTABLE, token, None, 0);
    let source = builder.account(ACCOUNT_WRITABLE, None, token, TOKEN_ACCOUNT_LEN);
    let destination = builder.account(ACCOUNT_WRITABLE, None, token, TOKEN_ACCOUNT_LEN);
    let authority = builder.account(ACCOUNT_SIGNER, None, None, 0);

    let zero = builder.const_u64(0);
    let balance = builder.read(OP_READ_U64, source, AMOUNT_OFFSET);
    let has_balance = builder.binary(OP_GT, balance, zero);
    builder.require(has_balance);

    let transfer_ix = builder.blob(&[3]); // SPL Token Transfer
    let transfer = builder.cpi(
        token_program,
        &[
            (source, ACCOUNT_WRITABLE),
            (destination, ACCOUNT_WRITABLE),
            (authority, ACCOUNT_SIGNER),
        ],
        &[
            Segment::Literal(transfer_ix),
            Segment::Register(DATA_REG_U64, balance),
        ],
    );
    builder.invoke(transfer, None);
    builder.build().expect("template builds")
}
// #endregion forward-the-whole-token-balance

// #region repay-exactly-what-is-owed
/// Repay the debt read from the loan account, capped by the borrower's balance.
pub fn repay_exactly_what_is_owed() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    // Stand-ins so the example runs as written: replace them with your lending program's address,
    // its repay discriminator, and the offset of the debt in its loan account.
    const LENDING_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const REPAY_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const DEBT_OFFSET: u64 = 8;
    const LOAN_ACCOUNT_LEN: u32 = 128;

    let mut builder = ProgramBuilder::new();
    let lending_program = builder.account(ACCOUNT_EXECUTABLE, Some(LENDING_PROGRAM), None, 0);
    let loan = builder.account(0, None, Some(LENDING_PROGRAM), LOAN_ACCOUNT_LEN);
    let borrower = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let pool = builder.account(ACCOUNT_WRITABLE, None, None, 0);

    let owed = builder.read(OP_READ_U64, loan, DEBT_OFFSET);
    let available = builder.account_lamports(borrower);
    let amount = builder.binary(OP_MIN, owed, available);
    let repay_ix = builder.blob(&REPAY_DISCRIMINATOR);
    let repay = builder.cpi(
        lending_program,
        &[
            (borrower, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (pool, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(repay_ix),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    builder.invoke(repay, None);
    builder.build().expect("template builds")
}
// #endregion repay-exactly-what-is-owed

// #region split-what-arrived
/// Pay a partner `shareBps` of the vault's balance above `reserve`, and the treasury the rest.
pub fn split_what_arrived() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    let mut builder = ProgramBuilder::new();
    let system_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let vault = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let partner = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let treasury = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let reserve_input = builder.input(VALUE_U64, 0);
    let share_bps_input = builder.input(VALUE_U64, 0);

    let reserve = builder.load_input(reserve_input);
    let share_bps = builder.load_input(share_bps_input);
    let ten_thousand = builder.const_u64(10_000);

    let balance = builder.account_lamports(vault);
    let distributable = builder.binary(OP_SUB, balance, reserve);
    let weighted = builder.binary(OP_MUL, distributable, share_bps);
    let partner_share = builder.binary(OP_DIV, weighted, ten_thousand);

    let to_partner_ix = builder.blob(&[2, 0, 0, 0]);
    let to_partner = builder.cpi(
        system_program,
        &[
            (vault, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (partner, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(to_partner_ix),
            Segment::Register(DATA_REG_U64, partner_share),
        ],
    );
    builder.invoke(to_partner, None);

    let rest = builder.binary(OP_SUB, distributable, partner_share);
    let to_treasury_ix = builder.blob(&[2, 0, 0, 0]);
    let to_treasury = builder.cpi(
        system_program,
        &[
            (vault, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (treasury, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(to_treasury_ix),
            Segment::Register(DATA_REG_U64, rest),
        ],
    );
    builder.invoke(to_treasury, None);
    builder.build().expect("template builds")
}
// #endregion split-what-arrived

// ------------------------------------------------------------------ conditional calls

// #region claim-only-when-there-is-something
/// Call the claim instruction only when the rewards account shows a pending amount.
pub fn claim_only_when_there_is_something() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    // Stand-ins so the example runs as written: replace them with the rewards program's address,
    // its claim instruction data, and the offset of the pending amount in its rewards account.
    const REWARDS_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const CLAIM_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const CLAIM_ARGUMENT: u64 = 10_000;
    const PENDING_OFFSET: u64 = 8;

    let mut builder = ProgramBuilder::new();
    let rewards_program = builder.account(ACCOUNT_EXECUTABLE, Some(REWARDS_PROGRAM), None, 0);
    let rewards = builder.account(0, None, Some(REWARDS_PROGRAM), 128);
    let claimant = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let destination = builder.account(ACCOUNT_WRITABLE, None, None, 0);

    let claim_argument = builder.const_u64(CLAIM_ARGUMENT);
    let zero = builder.const_u64(0);

    let claim_ix = builder.blob(&CLAIM_DISCRIMINATOR);
    let claim = builder.cpi(
        rewards_program,
        &[
            (claimant, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (destination, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(claim_ix),
            Segment::Register(DATA_REG_U64, claim_argument),
        ],
    );
    let pending = builder.read(OP_READ_U64, rewards, PENDING_OFFSET);
    let has_pending = builder.binary(OP_GT, pending, zero);
    builder.invoke(claim, Some(has_pending));
    builder.build().expect("template builds")
}
// #endregion claim-only-when-there-is-something

// #region liquidate-only-when-unhealthy
/// Liquidate only when the position's health value is below `threshold`.
pub fn liquidate_only_when_unhealthy() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    // Stand-ins so the example runs as written: replace them with the lending program's address,
    // its liquidate instruction data, and the offset of the health value in its position account.
    const LENDING_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const LIQUIDATE_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const LIQUIDATE_ARGUMENT: u64 = 10_000;
    const HEALTH_OFFSET: u64 = 8;

    let mut builder = ProgramBuilder::new();
    let lending_program = builder.account(ACCOUNT_EXECUTABLE, Some(LENDING_PROGRAM), None, 0);
    let position = builder.account(0, None, Some(LENDING_PROGRAM), 128);
    let liquidator = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let vault = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let threshold_input = builder.input(VALUE_U64, 0);

    let threshold = builder.load_input(threshold_input);
    let liquidate_argument = builder.const_u64(LIQUIDATE_ARGUMENT);

    let liquidate_ix = builder.blob(&LIQUIDATE_DISCRIMINATOR);
    let liquidate = builder.cpi(
        lending_program,
        &[
            (liquidator, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (vault, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(liquidate_ix),
            Segment::Register(DATA_REG_U64, liquidate_argument),
        ],
    );
    let health = builder.read(OP_READ_U64, position, HEALTH_OFFSET);
    let unhealthy = builder.binary(OP_LT, health, threshold);
    builder.invoke(liquidate, Some(unhealthy));
    builder.build().expect("template builds")
}
// #endregion liquidate-only-when-unhealthy

// #region top-up-only-when-low
/// Send `topUp` lamports to the bot only while its balance is below `floor`.
pub fn top_up_only_when_low() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    let mut builder = ProgramBuilder::new();
    let system_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let funder = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let bot = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let floor_input = builder.input(VALUE_U64, 0);
    let top_up_input = builder.input(VALUE_U64, 0);

    let floor = builder.load_input(floor_input);
    let top_up = builder.load_input(top_up_input);

    let transfer_ix = builder.blob(&[2, 0, 0, 0]);
    let transfer = builder.cpi(
        system_program,
        &[
            (funder, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (bot, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(transfer_ix),
            Segment::Register(DATA_REG_U64, top_up),
        ],
    );
    let balance = builder.account_lamports(bot);
    let is_low = builder.binary(OP_LT, balance, floor);
    builder.invoke(transfer, Some(is_low));
    builder.build().expect("template builds")
}
// #endregion top-up-only-when-low

// #region initialize-only-if-missing
/// Call the initialize instruction only when the position account holds no data yet.
pub fn initialize_only_if_missing() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    // Stand-ins so the example runs as written: replace them with your program's address and its
    // initialize instruction data.
    const PROTOCOL_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const INITIALIZE_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const INITIALIZE_ARGUMENT: u64 = 10_000;

    let mut builder = ProgramBuilder::new();
    let protocol_program = builder.account(ACCOUNT_EXECUTABLE, Some(PROTOCOL_PROGRAM), None, 0);
    let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let position = builder.account(ACCOUNT_WRITABLE, None, None, 0);

    let argument = builder.const_u64(INITIALIZE_ARGUMENT);
    let initialize_ix = builder.blob(&INITIALIZE_DISCRIMINATOR);
    let initialize = builder.cpi(
        protocol_program,
        &[
            (payer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (position, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(initialize_ix),
            Segment::Register(DATA_REG_U64, argument),
        ],
    );
    let missing = builder.account_is_empty(position);
    builder.invoke(initialize, Some(missing));
    builder.build().expect("template builds")
}
// #endregion initialize-only-if-missing

// ------------------------------------------------------------------ loops

// #region waterfall-until-the-money-runs-out
/// Pay creditors in row order, each the smaller of what it is owed and what is left.
pub fn waterfall_until_the_money_runs_out() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    let mut builder = ProgramBuilder::new();
    let system_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let treasury = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let creditor = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
    builder.batch(8, 1); // 1 to 8 rows
    let reserve_input = builder.input(VALUE_U64, 0);
    let owed_input = builder.row_input(VALUE_U64, 0);

    let reserve = builder.load_input(reserve_input);
    let zero = builder.const_u64(0);
    let balance = builder.account_lamports(treasury);
    let remaining = builder.binary(OP_SUB, balance, reserve);

    let transfer_ix = builder.blob(&[2, 0, 0, 0]);
    // `remaining` is carried: its register keeps its value from one row to the next.
    builder.for_each(1 << remaining, |row| {
        let owed = row.load_input(owed_input);
        let pay = row.binary(OP_MIN, remaining, owed);
        let transfer = row.cpi(
            system_program,
            &[
                (treasury, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (creditor, ACCOUNT_WRITABLE),
            ],
            &[
                Segment::Literal(transfer_ix),
                Segment::Register(DATA_REG_U64, pay),
            ],
        );
        let pays_something = row.binary(OP_GT, pay, zero);
        row.invoke(transfer, Some(pays_something));
        let left = row.binary(OP_SUB, remaining, pay);
        row.mov(remaining, left);
    });
    builder.build().expect("template builds")
}
// #endregion waterfall-until-the-money-runs-out

// #region consolidate-only-the-funded-accounts
/// Move each row's whole token balance into the vault, skipping empty accounts.
pub fn consolidate_only_the_funded_accounts() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, TOKEN_PROGRAM_ID};

    const TOKEN_ACCOUNT_LEN: u32 = 165;
    const AMOUNT_OFFSET: u64 = 64;

    let mut builder = ProgramBuilder::new();
    let token = Some(TOKEN_PROGRAM_ID.to_bytes());
    let token_program = builder.account(ACCOUNT_EXECUTABLE, token, None, 0);
    let vault = builder.account(ACCOUNT_WRITABLE, None, token, TOKEN_ACCOUNT_LEN);
    let authority = builder.account(ACCOUNT_SIGNER, None, None, 0);
    let source = builder.row_account(ACCOUNT_WRITABLE, None, token, TOKEN_ACCOUNT_LEN);
    builder.batch(8, 1);

    let zero = builder.const_u64(0);
    let transfer_ix = builder.blob(&[3]);
    builder.for_each(0, |row| {
        let amount = row.read(OP_READ_U64, source, AMOUNT_OFFSET);
        let transfer = row.cpi(
            token_program,
            &[
                (source, ACCOUNT_WRITABLE),
                (vault, ACCOUNT_WRITABLE),
                (authority, ACCOUNT_SIGNER),
            ],
            &[
                Segment::Literal(transfer_ix),
                Segment::Register(DATA_REG_U64, amount),
            ],
        );
        let funded = row.binary(OP_GT, amount, zero);
        row.invoke(transfer, Some(funded));
    });
    builder.build().expect("template builds")
}
// #endregion consolidate-only-the-funded-accounts

// #region crank-only-the-ripe-entries
/// Settle each queue entry whose deadline has passed; skip the rest.
pub fn crank_only_the_ripe_entries() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    // Stand-ins so the example runs as written: replace them with the queue program's address,
    // its settle instruction data, and the offset of the deadline in its entry account.
    const QUEUE_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const SETTLE_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const SETTLE_ARGUMENT: u64 = 10_000;
    const DEADLINE_OFFSET: u64 = 8;

    let mut builder = ProgramBuilder::new();
    let queue_program = builder.account(ACCOUNT_EXECUTABLE, Some(QUEUE_PROGRAM), None, 0);
    let keeper = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let entry = builder.row_account(ACCOUNT_WRITABLE, None, Some(QUEUE_PROGRAM), 128);
    builder.batch(8, 1);

    let argument = builder.const_u64(SETTLE_ARGUMENT);
    let settle_ix = builder.blob(&SETTLE_DISCRIMINATOR);
    builder.for_each(0, |row| {
        let settle = row.cpi(
            queue_program,
            &[
                (keeper, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (entry, ACCOUNT_WRITABLE),
            ],
            &[
                Segment::Literal(settle_ix),
                Segment::Register(DATA_REG_U64, argument),
            ],
        );
        let deadline = row.read(OP_READ_I64, entry, DEADLINE_OFFSET);
        let now = row.clock_timestamp();
        let due = row.binary(OP_LTE, deadline, now);
        row.invoke(settle, Some(due));
    });
    builder.build().expect("template builds")
}
// #endregion crank-only-the-ripe-entries

// #region distribute-a-runtime-pot-pro-rata
/// Pay each holder `weightBps` of the vault's balance above `reserve`.
pub fn distribute_a_runtime_pot_pro_rata() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    let mut builder = ProgramBuilder::new();
    let system_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let vault = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let holder = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
    builder.batch(8, 1);
    let reserve_input = builder.input(VALUE_U64, 0);
    let weight_bps_input = builder.row_input(VALUE_U64, 0);

    let reserve = builder.load_input(reserve_input);
    let ten_thousand = builder.const_u64(10_000);
    let balance = builder.account_lamports(vault);
    let pot = builder.binary(OP_SUB, balance, reserve);

    let transfer_ix = builder.blob(&[2, 0, 0, 0]);
    builder.for_each(0, |row| {
        let weight_bps = row.load_input(weight_bps_input);
        let weighted = row.binary(OP_MUL, pot, weight_bps);
        let share = row.binary(OP_DIV, weighted, ten_thousand);
        let transfer = row.cpi(
            system_program,
            &[
                (vault, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (holder, ACCOUNT_WRITABLE),
            ],
            &[
                Segment::Literal(transfer_ix),
                Segment::Register(DATA_REG_U64, share),
            ],
        );
        row.invoke(transfer, None);
    });
    builder.build().expect("template builds")
}
// #endregion distribute-a-runtime-pot-pro-rata

// #region crank-once-per-waiting-entry
/// Crank the queue once for each waiting entry, at most eight times.
pub fn crank_once_per_waiting_entry() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    // Stand-ins so the example runs as written: replace them with the queue program's address,
    // its crank instruction data, and the offset of the waiting count in its queue account.
    const QUEUE_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const CRANK_DATA: [u8; 12] = [2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0];
    const WAITING_OFFSET: u64 = 8;

    let mut builder = ProgramBuilder::new();
    let queue_program = builder.account(ACCOUNT_EXECUTABLE, Some(QUEUE_PROGRAM), None, 0);
    let keeper = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    // 16 bytes, so the u64 at offset 8 fits. The TypeScript compiler works this out itself.
    let queue = builder.account(ACCOUNT_WRITABLE, None, Some(QUEUE_PROGRAM), 16);

    let most = builder.const_u64(8);
    let waiting = builder.read(OP_READ_U64, queue, WAITING_OFFSET);
    let count = builder.binary(OP_MIN, waiting, most);
    let crank_ix = builder.blob(&CRANK_DATA);
    let crank = builder.cpi(
        queue_program,
        &[
            (keeper, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (queue, ACCOUNT_WRITABLE),
        ],
        &[Segment::Literal(crank_ix)],
    );
    // Runs `count` times, at most 8. The 0 is the carry mask: nothing is carried.
    builder.repeat(count, 8, 0, |pass| pass.invoke(crank, None));
    builder.build().expect("template builds")
}
// #endregion crank-once-per-waiting-entry

// ------------------------------------------------------------------ payments

// #region bounded-sol-payroll
/// Pay `amount` lamports from the treasury to each of 1 to 30 recipients.
pub fn bounded_sol_payroll() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    let mut builder = ProgramBuilder::new();
    let system_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let treasury = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let recipient = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
    builder.batch(30, 1);
    let amount_input = builder.input(VALUE_U64, 0);

    let amount = builder.load_input(amount_input);
    let transfer_ix = builder.blob(&[2, 0, 0, 0]);
    let transfer = builder.cpi(
        system_program,
        &[
            (treasury, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (recipient, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(transfer_ix),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    builder.for_each(0, |row| row.invoke(transfer, None));
    builder.build().expect("template builds")
}
// #endregion bounded-sol-payroll

// #region basis-point-revenue-split
/// Split `total` lamports: `partnerBps` of it to the partner, the rest to the treasury.
pub fn basis_point_revenue_split() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    let mut builder = ProgramBuilder::new();
    let system_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let source = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let partner = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let treasury = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let total_input = builder.input(VALUE_U64, 0);
    let partner_bps_input = builder.input(VALUE_U64, 0);

    let total = builder.load_input(total_input);
    let partner_bps = builder.load_input(partner_bps_input);
    let ten_thousand = builder.const_u64(10_000);

    let valid_share = builder.binary(OP_LTE, partner_bps, ten_thousand);
    builder.require(valid_share);
    let weighted = builder.binary(OP_MUL, total, partner_bps);
    let partner_amount = builder.binary(OP_DIV, weighted, ten_thousand);

    let to_partner_ix = builder.blob(&[2, 0, 0, 0]);
    let to_partner = builder.cpi(
        system_program,
        &[
            (source, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (partner, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(to_partner_ix),
            Segment::Register(DATA_REG_U64, partner_amount),
        ],
    );
    builder.invoke(to_partner, None);

    let rest = builder.binary(OP_SUB, total, partner_amount);
    let to_treasury_ix = builder.blob(&[2, 0, 0, 0]);
    let to_treasury = builder.cpi(
        system_program,
        &[
            (source, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (treasury, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(to_treasury_ix),
            Segment::Register(DATA_REG_U64, rest),
        ],
    );
    builder.invoke(to_treasury, None);
    builder.build().expect("template builds")
}
// #endregion basis-point-revenue-split

// #region index-weighted-rewards
/// Pay the recipient in row `i` (counting from 0) `(i + 1) × base` lamports.
pub fn index_weighted_rewards() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    let mut builder = ProgramBuilder::new();
    let system_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let treasury = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let recipient = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
    builder.batch(30, 1);
    let base_input = builder.input(VALUE_U64, 0);

    let base = builder.load_input(base_input);
    let one = builder.const_u64(1);
    let transfer_ix = builder.blob(&[2, 0, 0, 0]);
    builder.for_each(0, |row| {
        let index = row.loop_index();
        let multiple = row.binary(OP_ADD, index, one);
        let lamports = row.binary(OP_MUL, multiple, base);
        let transfer = row.cpi(
            system_program,
            &[
                (treasury, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (recipient, ACCOUNT_WRITABLE),
            ],
            &[
                Segment::Literal(transfer_ix),
                Segment::Register(DATA_REG_U64, lamports),
            ],
        );
        row.invoke(transfer, None);
    });
    builder.build().expect("template builds")
}
// #endregion index-weighted-rewards

// #region deadline-refund
/// Refund the customer only if the run executes at or before `deadline`.
pub fn deadline_refund() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    let mut builder = ProgramBuilder::new();
    let system_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let escrow_authority = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let customer = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let refund_amount_input = builder.input(VALUE_U64, 0);
    let deadline_input = builder.input(VALUE_I64, 0);

    let refund_amount = builder.load_input(refund_amount_input);
    let deadline = builder.load_input(deadline_input);

    let transfer_ix = builder.blob(&[2, 0, 0, 0]);
    let refund = builder.cpi(
        system_program,
        &[
            (escrow_authority, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (customer, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(transfer_ix),
            Segment::Register(DATA_REG_U64, refund_amount),
        ],
    );
    let now = builder.clock_timestamp();
    let in_time = builder.binary(OP_LTE, now, deadline);
    builder.invoke(refund, Some(in_time));
    builder.build().expect("template builds")
}
// #endregion deadline-refund

// #region reserve-preserving-sweep
/// Move up to `cap` lamports to the vault without taking the payer below `reserve`.
pub fn reserve_preserving_sweep() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    let mut builder = ProgramBuilder::new();
    let system_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let vault = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let reserve_input = builder.input(VALUE_U64, 0);
    let cap_input = builder.input(VALUE_U64, 0);

    let reserve = builder.load_input(reserve_input);
    let cap = builder.load_input(cap_input);

    let before = builder.account_lamports(payer);
    let starts_above = builder.binary(OP_GTE, before, reserve);
    builder.require(starts_above);

    let above_reserve = builder.binary(OP_SUB, before, reserve);
    let lamports = builder.binary(OP_MIN, above_reserve, cap);
    let transfer_ix = builder.blob(&[2, 0, 0, 0]);
    let transfer = builder.cpi(
        system_program,
        &[
            (payer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (vault, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(transfer_ix),
            Segment::Register(DATA_REG_U64, lamports),
        ],
    );
    builder.invoke(transfer, None);

    let after = builder.account_lamports(payer);
    let ends_above = builder.binary(OP_GTE, after, reserve);
    builder.require(ends_above);
    builder.build().expect("template builds")
}
// #endregion reserve-preserving-sweep

// ------------------------------------------------------------------ token accounts

// #region assert-create-then-transfer
/// For each row: prove the destination is the recipient's ATA, create it if missing, then pay.
pub fn assert_create_then_transfer() -> Vec<u8> {
    use ballista_sdk::{
        ballista_common::template::*, ProgramBuilder, Segment, ASSOCIATED_TOKEN_PROGRAM_ID,
        SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID,
    };

    const MINT_LEN: u32 = 82;
    const TOKEN_ACCOUNT_LEN: u32 = 165;

    let mut builder = ProgramBuilder::new();
    let token = Some(TOKEN_PROGRAM_ID.to_bytes());
    let ata_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(ASSOCIATED_TOKEN_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let token_program = builder.account(ACCOUNT_EXECUTABLE, token, None, 0);
    let system_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let mint = builder.account(0, None, token, MINT_LEN);
    let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let authority = builder.account(ACCOUNT_SIGNER, None, None, 0);
    let source = builder.account(ACCOUNT_WRITABLE, None, token, TOKEN_ACCOUNT_LEN);
    // Each row is two accounts: the recipient's wallet, then its ATA.
    let recipient = builder.row_account(0, None, None, 0);
    let destination_ata = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
    builder.batch(8, 1);
    let amount_input = builder.input(VALUE_U64, 0);

    let amount = builder.load_input(amount_input);
    builder.for_each(0, |row| {
        // assertAta: the ATA address is a PDA of the ATA program over (owner, token program, mint).
        let actual = row.account_key(destination_ata);
        let owner_key = row.account_key(recipient);
        let token_program_key = row.account_key(token_program);
        let mint_key = row.account_key(mint);
        let expected = row.derive_pda(
            ata_program,
            &[
                Segment::Register(DATA_REG_PUBKEY, owner_key),
                Segment::Register(DATA_REG_PUBKEY, token_program_key),
                Segment::Register(DATA_REG_PUBKEY, mint_key),
            ],
        );
        let is_ata = row.binary(OP_EQ, actual, expected);
        row.require(is_ata);

        // ensureAssociatedTokenAccount: ATA Create (empty data), only while the ATA is empty.
        let create = row.cpi(
            ata_program,
            &[
                (payer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (destination_ata, ACCOUNT_WRITABLE),
                (recipient, 0),
                (mint, 0),
                (system_program, 0),
                (token_program, 0),
            ],
            &[],
        );
        let missing = row.account_is_empty(destination_ata);
        row.invoke(create, Some(missing));

        let transfer_ix = row.blob(&[3]);
        let transfer = row.cpi(
            token_program,
            &[
                (source, ACCOUNT_WRITABLE),
                (destination_ata, ACCOUNT_WRITABLE),
                (authority, ACCOUNT_SIGNER),
            ],
            &[
                Segment::Literal(transfer_ix),
                Segment::Register(DATA_REG_U64, amount),
            ],
        );
        row.invoke(transfer, None);
    });
    builder.build().expect("template builds")
}
// #endregion assert-create-then-transfer

// #region existing-account-token-payroll
/// Pay `amount` tokens from one source to each of 1 to 32 existing token accounts.
pub fn existing_account_token_payroll() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, TOKEN_PROGRAM_ID};

    const TOKEN_ACCOUNT_LEN: u32 = 165;

    let mut builder = ProgramBuilder::new();
    let token = Some(TOKEN_PROGRAM_ID.to_bytes());
    let token_program = builder.account(ACCOUNT_EXECUTABLE, token, None, 0);
    let source = builder.account(ACCOUNT_WRITABLE, None, token, TOKEN_ACCOUNT_LEN);
    let authority = builder.account(ACCOUNT_SIGNER, None, None, 0);
    let destination = builder.row_account(ACCOUNT_WRITABLE, None, token, TOKEN_ACCOUNT_LEN);
    builder.batch(32, 1);
    let amount_input = builder.input(VALUE_U64, 0);

    let amount = builder.load_input(amount_input);
    let transfer_ix = builder.blob(&[3]);
    let transfer = builder.cpi(
        token_program,
        &[
            (source, ACCOUNT_WRITABLE),
            (destination, ACCOUNT_WRITABLE),
            (authority, ACCOUNT_SIGNER),
        ],
        &[
            Segment::Literal(transfer_ix),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    builder.for_each(0, |row| row.invoke(transfer, None));
    builder.build().expect("template builds")
}
// #endregion existing-account-token-payroll

// #region conditional-ata-setup
/// Create the wallet's ATA with the ATA program's `Create`, only if it does not exist yet.
pub fn conditional_ata_setup() -> Vec<u8> {
    use ballista_sdk::{
        ballista_common::template::*, ProgramBuilder, ASSOCIATED_TOKEN_PROGRAM_ID,
        SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID,
    };

    let mut builder = ProgramBuilder::new();
    let token = Some(TOKEN_PROGRAM_ID.to_bytes());
    let ata_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(ASSOCIATED_TOKEN_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let token_program = builder.account(ACCOUNT_EXECUTABLE, token, None, 0);
    let system_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let mint = builder.account(0, None, token, 82);
    let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let wallet = builder.account(0, None, None, 0);
    let ata = builder.account(ACCOUNT_WRITABLE, None, None, 0);

    let create = builder.cpi(
        ata_program,
        &[
            (payer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (ata, ACCOUNT_WRITABLE),
            (wallet, 0),
            (mint, 0),
            (system_program, 0),
            (token_program, 0),
        ],
        &[], // ATA Create takes no instruction data
    );
    let missing = builder.account_is_empty(ata);
    builder.invoke(create, Some(missing));
    builder.build().expect("template builds")
}
// #endregion conditional-ata-setup

// #region close-empty-token-accounts
/// Close each row's token account whose balance is zero; skip the others.
pub fn close_empty_token_accounts() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, TOKEN_PROGRAM_ID};

    const TOKEN_ACCOUNT_LEN: u32 = 165;
    const AMOUNT_OFFSET: u64 = 64;

    let mut builder = ProgramBuilder::new();
    let token = Some(TOKEN_PROGRAM_ID.to_bytes());
    let token_program = builder.account(ACCOUNT_EXECUTABLE, token, None, 0);
    let rent_destination = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let authority = builder.account(ACCOUNT_SIGNER, None, None, 0);
    let token_account = builder.row_account(ACCOUNT_WRITABLE, None, token, TOKEN_ACCOUNT_LEN);
    builder.batch(16, 1);

    let zero = builder.const_u64(0);
    let close_ix = builder.blob(&[9]); // SPL Token CloseAccount
    builder.for_each(0, |row| {
        let close = row.cpi(
            token_program,
            &[
                (token_account, ACCOUNT_WRITABLE),
                (rent_destination, ACCOUNT_WRITABLE),
                (authority, ACCOUNT_SIGNER),
            ],
            &[Segment::Literal(close_ix)],
        );
        let balance = row.read(OP_READ_U64, token_account, AMOUNT_OFFSET);
        let is_empty = row.binary(OP_EQ, balance, zero);
        row.invoke(close, Some(is_empty));
    });
    builder.build().expect("template builds")
}
// #endregion close-empty-token-accounts

// #region exact-token-debit
/// Transfer `amount` tokens, then require the source fell by exactly that much.
pub fn exact_token_debit() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, TOKEN_PROGRAM_ID};

    const TOKEN_ACCOUNT_LEN: u32 = 165;
    const AMOUNT_OFFSET: u64 = 64;

    let mut builder = ProgramBuilder::new();
    let token = Some(TOKEN_PROGRAM_ID.to_bytes());
    let token_program = builder.account(ACCOUNT_EXECUTABLE, token, None, 0);
    let source = builder.account(ACCOUNT_WRITABLE, None, token, TOKEN_ACCOUNT_LEN);
    let destination = builder.account(ACCOUNT_WRITABLE, None, token, TOKEN_ACCOUNT_LEN);
    let authority = builder.account(ACCOUNT_SIGNER, None, None, 0);
    let amount_input = builder.input(VALUE_U64, 0);

    let amount = builder.load_input(amount_input);
    let before = builder.read(OP_READ_U64, source, AMOUNT_OFFSET);

    let transfer_ix = builder.blob(&[3]);
    let transfer = builder.cpi(
        token_program,
        &[
            (source, ACCOUNT_WRITABLE),
            (destination, ACCOUNT_WRITABLE),
            (authority, ACCOUNT_SIGNER),
        ],
        &[
            Segment::Literal(transfer_ix),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    builder.invoke(transfer, None);

    let after = builder.read(OP_READ_U64, source, AMOUNT_OFFSET);
    let expected = builder.binary(OP_SUB, before, amount);
    let exact = builder.binary(OP_EQ, after, expected);
    builder.require(exact);
    builder.build().expect("template builds")
}
// #endregion exact-token-debit

// ------------------------------------------------------------------ guardrails

// #region deadline-and-minimum-output
/// Forward the client's swap only if the quote is unexpired and promises at least `minimumOut`.
pub fn deadline_and_minimum_output() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    // Stand-in so the example runs as written: replace it with the swap program's address.
    const SWAP_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const ROUTE_DATA_MAX: u16 = 256;

    let mut builder = ProgramBuilder::new();
    let swap_program = builder.account(ACCOUNT_EXECUTABLE, Some(SWAP_PROGRAM), None, 0);
    let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let pool = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let deadline_input = builder.input(VALUE_I64, 0);
    let quoted_out_input = builder.input(VALUE_U64, 0);
    let minimum_out_input = builder.input(VALUE_U64, 0);
    let route_data_input = builder.input(VALUE_BYTES, ROUTE_DATA_MAX);

    let deadline = builder.load_input(deadline_input);
    let quoted_out = builder.load_input(quoted_out_input);
    let minimum_out = builder.load_input(minimum_out_input);
    let route_data = builder.load_input(route_data_input);

    // `require(and(a, b))` compiles to two requires.
    let now = builder.clock_timestamp();
    let in_time = builder.binary(OP_LTE, now, deadline);
    builder.require(in_time);
    let enough_out = builder.binary(OP_GTE, quoted_out, minimum_out);
    builder.require(enough_out);

    let swap = builder.cpi(
        swap_program,
        &[
            (payer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (pool, ACCOUNT_WRITABLE),
        ],
        &[Segment::Register(DATA_REG_BYTES, route_data)],
    );
    // A bytes segment's size is its input's maximum length, which the builder cannot see.
    builder.set_cpi_max_data_len(swap, ROUTE_DATA_MAX);
    builder.invoke(swap, None);
    builder.build().expect("template builds")
}
// #endregion deadline-and-minimum-output

// #region pinned-program-and-owner
/// Call one program, with the program address and the position's owner pinned in the schema.
pub fn pinned_program_and_owner() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    // Stand-ins so the example runs as written: replace them with your protocol's address and the
    // instruction's discriminator.
    const PROTOCOL_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const INSTRUCTION_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];

    let mut builder = ProgramBuilder::new();
    let protocol_program = builder.account(ACCOUNT_EXECUTABLE, Some(PROTOCOL_PROGRAM), None, 0);
    let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let position = builder.account(ACCOUNT_WRITABLE, None, Some(PROTOCOL_PROGRAM), 128);
    let amount_input = builder.input(VALUE_U64, 0);

    let amount = builder.load_input(amount_input);
    let instruction_ix = builder.blob(&INSTRUCTION_DISCRIMINATOR);
    let call = builder.cpi(
        protocol_program,
        &[
            (payer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (position, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(instruction_ix),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    builder.invoke(call, None);
    builder.build().expect("template builds")
}
// #endregion pinned-program-and-owner

// #region oracle-price-band
/// Call the protocol only while the oracle's price lies within `[minimumPrice, maximumPrice]`.
pub fn oracle_price_band() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    // Stand-ins so the example runs as written: replace them with the oracle program (the owner
    // of the price account), the price's offset in its layout, and the protocol call.
    const ORACLE_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const PRICE_OFFSET: u64 = 8;
    const PROTOCOL_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const INSTRUCTION_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const INSTRUCTION_ARGUMENT: u64 = 10_000;

    let mut builder = ProgramBuilder::new();
    let oracle = builder.account(0, None, Some(ORACLE_PROGRAM), 128);
    let protocol_program = builder.account(ACCOUNT_EXECUTABLE, Some(PROTOCOL_PROGRAM), None, 0);
    let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let pool = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let minimum_price_input = builder.input(VALUE_I64, 0);
    let maximum_price_input = builder.input(VALUE_I64, 0);

    let minimum_price = builder.load_input(minimum_price_input);
    let maximum_price = builder.load_input(maximum_price_input);
    let argument = builder.const_u64(INSTRUCTION_ARGUMENT);

    // Each side of the band reads the price again; a read is cheap and nothing runs in between.
    let price = builder.read(OP_READ_I64, oracle, PRICE_OFFSET);
    let above_floor = builder.binary(OP_GTE, price, minimum_price);
    builder.require(above_floor);
    let price_again = builder.read(OP_READ_I64, oracle, PRICE_OFFSET);
    let below_ceiling = builder.binary(OP_LTE, price_again, maximum_price);
    builder.require(below_ceiling);

    let instruction_ix = builder.blob(&INSTRUCTION_DISCRIMINATOR);
    let call = builder.cpi(
        protocol_program,
        &[
            (payer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (pool, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(instruction_ix),
            Segment::Register(DATA_REG_U64, argument),
        ],
    );
    builder.invoke(call, None);
    builder.build().expect("template builds")
}
// #endregion oracle-price-band

// #region maximum-lamport-spend
/// Make the call, then fail the run if the payer's balance fell by more than `maximumSpend`.
pub fn maximum_lamport_spend() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    // Stand-ins so the example runs as written: replace them with the protocol call to protect.
    const PROTOCOL_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const INSTRUCTION_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const INSTRUCTION_ARGUMENT: u64 = 10_000;

    let mut builder = ProgramBuilder::new();
    let protocol_program = builder.account(ACCOUNT_EXECUTABLE, Some(PROTOCOL_PROGRAM), None, 0);
    let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let pool = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let maximum_spend_input = builder.input(VALUE_U64, 0);

    let maximum_spend = builder.load_input(maximum_spend_input);
    let argument = builder.const_u64(INSTRUCTION_ARGUMENT);
    let before = builder.account_lamports(payer);

    let instruction_ix = builder.blob(&INSTRUCTION_DISCRIMINATOR);
    let call = builder.cpi(
        protocol_program,
        &[
            (payer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (pool, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(instruction_ix),
            Segment::Register(DATA_REG_U64, argument),
        ],
    );
    builder.invoke(call, None);

    let after = builder.account_lamports(payer);
    let spent = builder.binary(OP_SUB, before, after);
    let within_limit = builder.binary(OP_LTE, spent, maximum_spend);
    builder.require(within_limit);
    builder.build().expect("template builds")
}
// #endregion maximum-lamport-spend

// #region canonical-position-account
/// Require `position` to be the PDA the protocol derives from ("position", owner, positionId).
pub fn canonical_position_account() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    // Stand-ins so the example runs as written: replace them with your protocol's address and the
    // instruction to call on the position.
    const PROTOCOL_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const INSTRUCTION_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const INSTRUCTION_ARGUMENT: u64 = 10_000;

    let mut builder = ProgramBuilder::new();
    let protocol_program = builder.account(ACCOUNT_EXECUTABLE, Some(PROTOCOL_PROGRAM), None, 0);
    let owner = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let position = builder.account(ACCOUNT_WRITABLE, None, Some(PROTOCOL_PROGRAM), 128);
    let position_id_input = builder.input(VALUE_BYTES, 8);

    let position_id = builder.load_input(position_id_input);
    let prefix = builder.const_bytes(b"position");
    let argument = builder.const_u64(INSTRUCTION_ARGUMENT);

    // assertPda: the account's key must equal the derived address.
    let actual = builder.account_key(position);
    let owner_key = builder.account_key(owner);
    let expected = builder.derive_pda(
        protocol_program,
        &[
            Segment::Register(DATA_REG_BYTES, prefix),
            Segment::Register(DATA_REG_PUBKEY, owner_key),
            Segment::Register(DATA_REG_BYTES, position_id),
        ],
    );
    let is_canonical = builder.binary(OP_EQ, actual, expected);
    builder.require(is_canonical);

    let instruction_ix = builder.blob(&INSTRUCTION_DISCRIMINATOR);
    let call = builder.cpi(
        protocol_program,
        &[
            (owner, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (position, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(instruction_ix),
            Segment::Register(DATA_REG_U64, argument),
        ],
    );
    builder.invoke(call, None);
    builder.build().expect("template builds")
}
// #endregion canonical-position-account

// ------------------------------------------------------------------ composition

// #region swap-then-deposit
/// Swap, require at least `minimumOut` arrived, then deposit.
pub fn swap_then_deposit() -> Vec<u8> {
    use ballista_sdk::{
        ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID,
    };

    // Stand-ins so the example runs as written: replace them with the swap and vault programs.
    const SWAP_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const VAULT_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const DATA_MAX: u16 = 256;
    const AMOUNT_OFFSET: u64 = 64;

    let mut builder = ProgramBuilder::new();
    let swap_program = builder.account(ACCOUNT_EXECUTABLE, Some(SWAP_PROGRAM), None, 0);
    let vault_program = builder.account(ACCOUNT_EXECUTABLE, Some(VAULT_PROGRAM), None, 0);
    let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let pool = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let received_tokens = builder.account(
        ACCOUNT_WRITABLE,
        None,
        Some(TOKEN_PROGRAM_ID.to_bytes()),
        165,
    );
    let minimum_out_input = builder.input(VALUE_U64, 0);
    let swap_data_input = builder.input(VALUE_BYTES, DATA_MAX);
    let deposit_data_input = builder.input(VALUE_BYTES, DATA_MAX);

    let minimum_out = builder.load_input(minimum_out_input);
    let swap_data = builder.load_input(swap_data_input);
    let deposit_data = builder.load_input(deposit_data_input);

    let before = builder.read(OP_READ_U64, received_tokens, AMOUNT_OFFSET);
    let swap = builder.cpi(
        swap_program,
        &[
            (payer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (pool, ACCOUNT_WRITABLE),
        ],
        &[Segment::Register(DATA_REG_BYTES, swap_data)],
    );
    builder.set_cpi_max_data_len(swap, DATA_MAX);
    builder.invoke(swap, None);

    let after = builder.read(OP_READ_U64, received_tokens, AMOUNT_OFFSET);
    let received = builder.binary(OP_SUB, after, before);
    let enough = builder.binary(OP_GTE, received, minimum_out);
    builder.require(enough);

    let deposit = builder.cpi(
        vault_program,
        &[
            (payer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (pool, ACCOUNT_WRITABLE),
        ],
        &[Segment::Register(DATA_REG_BYTES, deposit_data)],
    );
    builder.set_cpi_max_data_len(deposit, DATA_MAX);
    builder.invoke(deposit, None);
    builder.build().expect("template builds")
}
// #endregion swap-then-deposit

// #region claim-then-distribute
/// Claim once, then pay `amountPerRecipient` tokens to each row's token account.
pub fn claim_then_distribute() -> Vec<u8> {
    use ballista_sdk::{
        ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID,
    };

    // Stand-ins so the example runs as written: replace them with the rewards program's address
    // and its claim instruction data.
    const REWARDS_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const CLAIM_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const CLAIM_ARGUMENT: u64 = 10_000;
    const TOKEN_ACCOUNT_LEN: u32 = 165;

    let mut builder = ProgramBuilder::new();
    let token = Some(TOKEN_PROGRAM_ID.to_bytes());
    let rewards_program = builder.account(ACCOUNT_EXECUTABLE, Some(REWARDS_PROGRAM), None, 0);
    let token_program = builder.account(ACCOUNT_EXECUTABLE, token, None, 0);
    let claimer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let pool = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let treasury_tokens = builder.account(ACCOUNT_WRITABLE, None, token, TOKEN_ACCOUNT_LEN);
    let authority = builder.account(ACCOUNT_SIGNER, None, None, 0);
    let recipient_tokens = builder.row_account(ACCOUNT_WRITABLE, None, token, TOKEN_ACCOUNT_LEN);
    builder.batch(16, 1);
    let amount_input = builder.input(VALUE_U64, 0);

    let amount = builder.load_input(amount_input);
    let claim_argument = builder.const_u64(CLAIM_ARGUMENT);

    let claim_ix = builder.blob(&CLAIM_DISCRIMINATOR);
    let claim = builder.cpi(
        rewards_program,
        &[
            (claimer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (pool, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(claim_ix),
            Segment::Register(DATA_REG_U64, claim_argument),
        ],
    );
    builder.invoke(claim, None);

    let transfer_ix = builder.blob(&[3]);
    let transfer = builder.cpi(
        token_program,
        &[
            (treasury_tokens, ACCOUNT_WRITABLE),
            (recipient_tokens, ACCOUNT_WRITABLE),
            (authority, ACCOUNT_SIGNER),
        ],
        &[
            Segment::Literal(transfer_ix),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    builder.for_each(0, |row| row.invoke(transfer, None));
    builder.build().expect("template builds")
}
// #endregion claim-then-distribute

// #region primary-or-fallback-route
/// Call exactly one of two routes, chosen by `usePrimary`.
pub fn primary_or_fallback_route() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    // Stand-ins so the example runs as written: replace them with the two routes' programs.
    const PRIMARY_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const FALLBACK_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const DATA_MAX: u16 = 256;

    let mut builder = ProgramBuilder::new();
    let primary_program = builder.account(ACCOUNT_EXECUTABLE, Some(PRIMARY_PROGRAM), None, 0);
    let fallback_program = builder.account(ACCOUNT_EXECUTABLE, Some(FALLBACK_PROGRAM), None, 0);
    let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let pool = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let use_primary_input = builder.input(VALUE_BOOL, 0);
    let primary_data_input = builder.input(VALUE_BYTES, DATA_MAX);
    let fallback_data_input = builder.input(VALUE_BYTES, DATA_MAX);

    let use_primary = builder.load_input(use_primary_input);
    let primary_data = builder.load_input(primary_data_input);
    let fallback_data = builder.load_input(fallback_data_input);

    let primary = builder.cpi(
        primary_program,
        &[
            (payer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (pool, ACCOUNT_WRITABLE),
        ],
        &[Segment::Register(DATA_REG_BYTES, primary_data)],
    );
    builder.set_cpi_max_data_len(primary, DATA_MAX);
    builder.invoke(primary, Some(use_primary));

    let fallback = builder.cpi(
        fallback_program,
        &[
            (payer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (pool, ACCOUNT_WRITABLE),
        ],
        &[Segment::Register(DATA_REG_BYTES, fallback_data)],
    );
    builder.set_cpi_max_data_len(fallback, DATA_MAX);
    let use_fallback = builder.not(use_primary);
    builder.invoke(fallback, Some(use_fallback));
    builder.build().expect("template builds")
}
// #endregion primary-or-fallback-route

// #region time-gated-governance-execution
/// Forward the execute instruction only once the proposal is approved and its time has come.
pub fn time_gated_governance_execution() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    // Stand-ins so the example runs as written: replace them with your governance program's
    // address and the offsets of the two fields in its proposal account.
    const GOVERNANCE_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const APPROVED_OFFSET: u64 = 0;
    const TIME_OFFSET: u64 = 8;
    const DATA_MAX: u16 = 256;

    let mut builder = ProgramBuilder::new();
    let governance_program = builder.account(ACCOUNT_EXECUTABLE, Some(GOVERNANCE_PROGRAM), None, 0);
    let proposal = builder.account(0, None, Some(GOVERNANCE_PROGRAM), 128);
    let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let target = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let execute_data_input = builder.input(VALUE_BYTES, DATA_MAX);

    let execute_data = builder.load_input(execute_data_input);

    let approved = builder.read(OP_READ_BOOL, proposal, APPROVED_OFFSET);
    builder.require(approved);
    let now = builder.clock_timestamp();
    let executable_after = builder.read(OP_READ_I64, proposal, TIME_OFFSET);
    let time_has_come = builder.binary(OP_GTE, now, executable_after);
    builder.require(time_has_come);

    let execute = builder.cpi(
        governance_program,
        &[
            (payer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (target, ACCOUNT_WRITABLE),
        ],
        &[Segment::Register(DATA_REG_BYTES, execute_data)],
    );
    builder.set_cpi_max_data_len(execute, DATA_MAX);
    builder.invoke(execute, None);
    builder.build().expect("template builds")
}
// #endregion time-gated-governance-execution

// #region bounded-keeper-crank
/// Call the crank instruction once for each (market, queue) row.
pub fn bounded_keeper_crank() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    // Stand-ins so the example runs as written: replace them with the protocol's address and its
    // crank instruction data.
    const PROTOCOL_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const CRANK_DISCRIMINATOR: [u8; 4] = [2, 0, 0, 0];
    const CRANK_ARGUMENT: u64 = 1_000;

    let mut builder = ProgramBuilder::new();
    let protocol_program = builder.account(ACCOUNT_EXECUTABLE, Some(PROTOCOL_PROGRAM), None, 0);
    let keeper = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    // Each row is two accounts: a market, then its queue.
    let market = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
    let queue = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
    builder.batch(24, 1);

    let argument = builder.const_u64(CRANK_ARGUMENT);
    let crank_ix = builder.blob(&CRANK_DISCRIMINATOR);
    let crank = builder.cpi(
        protocol_program,
        &[
            (keeper, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (market, ACCOUNT_WRITABLE),
            (queue, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(crank_ix),
            Segment::Register(DATA_REG_U64, argument),
        ],
    );
    builder.for_each(0, |row| row.invoke(crank, None));
    builder.build().expect("template builds")
}
// #endregion bounded-keeper-crank

// ------------------------------------------------------------------ guide pages

// #region row-amounts
/// Pay each recipient its own amount, carried as a row input.
pub fn row_amounts() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    let mut builder = ProgramBuilder::new();
    let system_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let treasury = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let recipient = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
    builder.batch(30, 1);
    let amount_input = builder.row_input(VALUE_U64, 0);

    let transfer_ix = builder.blob(&[2, 0, 0, 0]);
    builder.for_each(0, |row| {
        let amount = row.load_input(amount_input); // this row's value
        let transfer = row.cpi(
            system_program,
            &[
                (treasury, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (recipient, ACCOUNT_WRITABLE),
            ],
            &[
                Segment::Literal(transfer_ix),
                Segment::Register(DATA_REG_U64, amount),
            ],
        );
        row.invoke(transfer, None);
    });
    builder.build().expect("template builds")
}
// #endregion row-amounts

// #region budgeted-payroll
/// Pay `amount` to every recipient, then require the total stays within `budget`.
pub fn budgeted_payroll() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    let mut builder = ProgramBuilder::new();
    let system_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let treasury = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let recipient = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
    builder.batch(30, 1);
    let amount_input = builder.input(VALUE_U64, 0);
    let budget_input = builder.input(VALUE_U64, 0);

    let amount = builder.load_input(amount_input);
    let budget = builder.load_input(budget_input);
    let total = builder.const_u64(0);

    let transfer_ix = builder.blob(&[2, 0, 0, 0]);
    let transfer = builder.cpi(
        system_program,
        &[
            (treasury, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (recipient, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(transfer_ix),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    // Bit `total` of the mask carries that register from row to row and out of the loop.
    builder.for_each(1 << total, |row| {
        row.invoke(transfer, None);
        let sum = row.binary(OP_ADD, total, amount);
        row.mov(total, sum);
    });
    let within_budget = builder.binary(OP_LTE, total, budget);
    builder.require(within_budget);
    builder.build().expect("template builds")
}
// #endregion budgeted-payroll

// #region exact-lamport-delta
/// Transfer `amount` lamports, then require the sender's balance fell by exactly that much.
pub fn exact_lamport_delta() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    let mut builder = ProgramBuilder::new();
    let system_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let sender = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let recipient = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let amount_input = builder.input(VALUE_U64, 0);

    let amount = builder.load_input(amount_input);
    let before = builder.account_lamports(sender); // the snapshot

    let transfer_ix = builder.blob(&[2, 0, 0, 0]);
    let transfer = builder.cpi(
        system_program,
        &[
            (sender, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (recipient, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(transfer_ix),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    builder.invoke(transfer, None);

    let after = builder.account_lamports(sender);
    let expected = builder.binary(OP_SUB, before, amount);
    let exact = builder.binary(OP_EQ, after, expected);
    builder.require(exact);
    builder.build().expect("template builds")
}
// #endregion exact-lamport-delta

// #region token-transfer
/// One SPL Token transfer, the same CPI `tokenTransfer` compiles to.
pub fn token_transfer() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, TOKEN_PROGRAM_ID};

    let mut builder = ProgramBuilder::new();
    let token = Some(TOKEN_PROGRAM_ID.to_bytes());
    let token_program = builder.account(ACCOUNT_EXECUTABLE, token, None, 0);
    let authority = builder.account(ACCOUNT_SIGNER, None, None, 0);
    let source = builder.account(ACCOUNT_WRITABLE, None, token, 165);
    let destination = builder.account(ACCOUNT_WRITABLE, None, token, 165);
    let amount_input = builder.input(VALUE_U64, 0);

    let amount = builder.load_input(amount_input);
    let transfer_ix = builder.blob(&[3]); // the byte that selects Transfer
    let transfer = builder.cpi(
        token_program,
        &[
            (source, ACCOUNT_WRITABLE),
            (destination, ACCOUNT_WRITABLE),
            (authority, ACCOUNT_SIGNER),
        ],
        &[
            Segment::Literal(transfer_ix),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    builder.invoke(transfer, None);
    builder.build().expect("template builds")
}
// #endregion token-transfer

// #region generic-cpi
/// A CPI built from parts: literal bytes, an encoded `u64`, and caller bytes, only when `enabled`.
pub fn generic_cpi() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment};

    // Placeholders: replace them with your program's address and its instruction discriminator.
    const MY_PROGRAM: [u8; 32] = [7; 32];
    const MY_DISCRIMINATOR: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    const PAYLOAD_MAX: u16 = 128;

    let mut builder = ProgramBuilder::new();
    let program = builder.account(ACCOUNT_EXECUTABLE, Some(MY_PROGRAM), None, 0);
    let vault = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let authority = builder.account(ACCOUNT_SIGNER, None, None, 0);
    let amount_input = builder.input(VALUE_U64, 0);
    let client_payload_input = builder.input(VALUE_BYTES, PAYLOAD_MAX);
    let enabled_input = builder.input(VALUE_BOOL, 0);

    let amount = builder.load_input(amount_input);
    let client_payload = builder.load_input(client_payload_input);
    let enabled = builder.load_input(enabled_input);

    let discriminator = builder.blob(&MY_DISCRIMINATOR);
    let call = builder.cpi(
        program,
        &[(vault, ACCOUNT_WRITABLE), (authority, ACCOUNT_SIGNER)],
        &[
            Segment::Literal(discriminator),
            Segment::Register(DATA_REG_U64, amount),
            Segment::Register(DATA_REG_BYTES, client_payload),
        ],
    );
    // 8 literal bytes + 8 for the u64 + up to PAYLOAD_MAX for the bytes.
    builder.set_cpi_max_data_len(call, 8 + 8 + PAYLOAD_MAX);
    builder.invoke(call, Some(enabled));
    builder.build().expect("template builds")
}
// #endregion generic-cpi

// #region rebalance-three-swaps
/// Three optional Jupiter swaps, each forwarding its own account group, each checked afterwards.
pub fn rebalance_three_swaps() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, TOKEN_PROGRAM_ID};
    use solana_program::pubkey;

    const JUPITER_V6: [u8; 32] = pubkey!("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4").to_bytes();
    const ROUTE_MAX: u16 = 256;
    const TOKEN_ACCOUNT_LEN: u32 = 165;
    const OWNER_OFFSET: u64 = 32; // SPL Token account: owner is the pubkey at byte 32
    const AMOUNT_OFFSET: u64 = 64; // and amount is the u64 at byte 64

    let mut builder = ProgramBuilder::new();
    let token = Some(TOKEN_PROGRAM_ID.to_bytes());
    let jupiter = builder.account(ACCOUNT_EXECUTABLE, Some(JUPITER_V6), None, 0);
    let token_program = builder.account(ACCOUNT_EXECUTABLE, token, None, 0);
    let user = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    // (source, destination) for swaps A, B and C.
    let legs: Vec<(u8, u8)> = (0..3)
        .map(|_| {
            let source = builder.account(ACCOUNT_WRITABLE, None, token, TOKEN_ACCOUNT_LEN);
            let destination = builder.account(ACCOUNT_WRITABLE, None, token, TOKEN_ACCOUNT_LEN);
            (source, destination)
        })
        .collect();
    let route_inputs: Vec<u8> = (0..3)
        .map(|_| builder.input(VALUE_BYTES, ROUTE_MAX))
        .collect();
    let target_inputs: Vec<u8> = (0..3).map(|_| builder.input(VALUE_U64, 0)).collect();
    let min_out_inputs: Vec<u8> = (0..3).map(|_| builder.input(VALUE_U64, 0)).collect();
    builder.account_groups(3); // ammA, ammB, ammC

    let routes: Vec<u8> = route_inputs
        .iter()
        .map(|&input| builder.load_input(input))
        .collect();
    let targets: Vec<u8> = target_inputs
        .iter()
        .map(|&input| builder.load_input(input))
        .collect();
    let min_outs: Vec<u8> = min_out_inputs
        .iter()
        .map(|&input| builder.load_input(input))
        .collect();

    for (leg, &(source, destination)) in legs.iter().enumerate() {
        let balance = builder.read(OP_READ_U64, destination, AMOUNT_OFFSET);
        let needed = builder.binary(OP_LT, balance, targets[leg]);

        let destination_owner = builder.read(OP_READ_PUBKEY, destination, OWNER_OFFSET);
        let user_key = builder.account_key(user);
        let owned_by_user = builder.binary(OP_EQ, destination_owner, user_key);
        builder.require(owned_by_user);

        let swap = builder.cpi_with_group(
            jupiter,
            &[
                (token_program, 0),
                (user, ACCOUNT_SIGNER),
                (source, ACCOUNT_WRITABLE),
                (destination, ACCOUNT_WRITABLE),
            ],
            &[Segment::Register(DATA_REG_BYTES, routes[leg])],
            leg as u8, // forwards group ammA, ammB or ammC after these four accounts
        );
        builder.set_cpi_max_data_len(swap, ROUTE_MAX);
        builder.invoke(swap, Some(needed));

        // Skipped, or the balance rose by at least the minimum.
        let skipped = builder.not(needed);
        let after = builder.read(OP_READ_U64, destination, AMOUNT_OFFSET);
        let received = builder.binary(OP_SUB, after, balance);
        let enough = builder.binary(OP_GTE, received, min_outs[leg]);
        let acceptable = builder.binary(OP_OR, skipped, enough);
        builder.require(acceptable);
    }
    builder.build().expect("template builds")
}
// #endregion rebalance-three-swaps
