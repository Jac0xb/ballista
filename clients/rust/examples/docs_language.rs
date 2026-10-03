//! The registry examples from "Remember state between runs" (`docs/guide/registries.md`),
//! authored in Rust with `ProgramBuilder`.
//!
//! Each template function builds the same bytes the TypeScript compiler produces for the example
//! of the same name in `clients/js/examples/docs/`, which the page shows beside it, and each run
//! function the same run data and account flags as the TypeScript run. `tests/docs_examples.rs`
//! checks both against `tests/fixtures/docs-examples.json`.
//!
//! ```bash
//! cargo run -p ballista-sdk --example docs_language
//! ```
//!
//! The page includes each function by its `#region` name.

#![allow(dead_code)]

use solana_program::{instruction::Instruction, pubkey::Pubkey};

fn main() {
    for (name, build) in TEMPLATES {
        let payload = build();
        let program = ballista_sdk::ballista_common::template::ProgramView::parse(&payload)
            .expect("payload parses");
        let stats = program.verify().expect("payload verifies");
        println!(
            "{name:24} {:4} bytes  {:3} instructions  {:2} registers",
            payload.len(),
            stats.instructions,
            stats.registers
        );
    }
    for (name, run) in RUNS {
        let instruction = run(0);
        println!(
            "{name:24} {:3} accounts  {:4} data bytes",
            instruction.accounts.len(),
            instruction.data.len()
        );
    }
}

/// An example's name and the function that builds its template.
pub type Example = (&'static str, fn() -> Vec<u8>);

/// Every template in this file, by the name the page's regions use.
pub const TEMPLATES: &[Example] = &[
    ("count-runs", count_runs),
    ("daily-limit-per-caller", daily_limit_per_caller),
    ("listed-callers-only", listed_callers_only),
];

/// An example's name and a function that builds its run for a given number of batch rows. These
/// templates have no batch, so every run ignores it.
pub type ExampleRun = (&'static str, fn(usize) -> Instruction);

/// Every run in this file, with the inputs the TypeScript runs in `docs-examples.test.ts` use. The
/// page shows no run for `count-runs`.
pub const RUNS: &[ExampleRun] = &[
    ("daily-limit-per-caller", |_| {
        run_daily_limit_per_caller(key(200), key(1), key(2), 1_000)
    }),
    // The author's run, the one that reads both inputs: key(6) is the AUTHOR stand-in, 32 sevens.
    ("listed-callers-only", |_| {
        run_listed_callers_only(key(200), key(6), key(2), Some((key(3), true)))
    }),
];

/// A distinct placeholder key per position, standing in for real addresses.
fn key(index: u8) -> Pubkey {
    Pubkey::new_from_array([index + 1; 32])
}

// #region count-runs
/// Count each caller's runs, in an entry of their own.
pub fn count_runs() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder};

    let mut builder = ProgramBuilder::new();
    let caller = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let caller_runs = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let system_program = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);

    let one = builder.const_u64(1);
    // Registry 0 (`runs`, one u64: 8 bytes), keyed by the caller's address. The caller pays.
    let key = builder.account_key(caller);
    builder.open_registry(caller_runs, Some(key), caller, 0, 8, system_program);
    let count = builder.read_registry(caller_runs, 0, OP_READ_U64);
    let next = builder.binary(OP_ADD, count, one);
    builder.write_registry(caller_runs, 0, OP_READ_U64, next);
    builder.build().expect("template builds")
}
// #endregion count-runs

// #region daily-limit-per-caller
/// Send SOL, at most 1 SOL at once per caller, refilling over about a day.
pub fn daily_limit_per_caller() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment};

    let mut builder = ProgramBuilder::new();
    let caller = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let recipient = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let caller_limit = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let system_program = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
    let amount_input = builder.input(VALUE_U64, 0);

    let amount = builder.load_input(amount_input);
    let refill_per_second = builder.const_u64(11_574); // 1 SOL over 86,400 seconds, rounded down
    let cap = builder.const_u64(1_000_000_000); // 1 SOL

    // Registry 0 (`limits`): `spent`, a u64 at offset 0, then `lastSpend`, an i64 at offset 8.
    let key = builder.account_key(caller);
    builder.open_registry(caller_limit, Some(key), caller, 0, 16, system_program);

    // The steps `rateLimit` writes out.
    let last = builder.read_registry(caller_limit, 8, OP_READ_I64);
    let clock = builder.clock_timestamp();
    let now = builder.binary(OP_MAX, clock, last);
    let spent = builder.read_registry(caller_limit, 0, OP_READ_U64);
    let spent = builder.cast(OP_CAST_U128, spent);
    let elapsed = builder.binary(OP_SUB, now, last);
    let elapsed = builder.cast(OP_CAST_U128, elapsed);
    let rate = builder.cast(OP_CAST_U128, refill_per_second);
    let refill = builder.binary(OP_MUL, elapsed, rate);
    let refilled = builder.binary(OP_MIN, spent, refill);
    let still_spent = builder.binary(OP_SUB, spent, refilled);
    let amount_u128 = builder.cast(OP_CAST_U128, amount);
    let total = builder.binary(OP_ADD, still_spent, amount_u128);
    let cap = builder.cast(OP_CAST_U128, cap);
    let within_rate_limit = builder.binary(OP_LTE, total, cap);
    builder.require(within_rate_limit);
    let total = builder.cast(OP_CAST_U64, total);
    builder.write_registry(caller_limit, 0, OP_READ_U64, total);
    builder.write_registry(caller_limit, 8, OP_READ_I64, now);

    let transfer_ix = builder.blob(&[2, 0, 0, 0]);
    let transfer = builder.cpi(
        system_program,
        &[
            (caller, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (recipient, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(transfer_ix),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    builder.invoke(transfer, None);
    builder.build().expect("template builds")
}
// #endregion daily-limit-per-caller

// #region run-daily-limit-per-caller
pub fn run_daily_limit_per_caller(
    template: Pubkey,
    caller: Pubkey,
    recipient: Pubkey,
    amount: u64,
) -> Instruction {
    use ballista_sdk::{
        find_registry_entry_address, run_instruction, RunInputs, SYSTEM_PROGRAM_ID,
    };
    use solana_program::instruction::AccountMeta;

    // The caller's entry: registry 0 (`limits`), keyed by the caller's address.
    let (caller_limit, _) = find_registry_entry_address(&template, 0, &caller.to_bytes());
    let inputs = RunInputs::new().u64(amount).finish();
    let accounts = vec![
        AccountMeta::new(caller, true),
        AccountMeta::new(recipient, false),
        AccountMeta::new(caller_limit, false),
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion run-daily-limit-per-caller

// #region listed-callers-only
/// Only listed callers make the call. The author's runs add or remove a member instead.
pub fn listed_callers_only() -> Vec<u8> {
    use ballista_sdk::{ballista_common::template::*, ProgramBuilder, Segment, SYSTEM_PROGRAM_ID};

    // Stand-ins so the example runs as written: replace AUTHOR with the author's address, and the
    // program and data with the call the list guards.
    const AUTHOR: [u8; 32] = [7; 32];
    const PROTOCOL_PROGRAM: [u8; 32] = SYSTEM_PROGRAM_ID.to_bytes();
    const CALL_DATA: [u8; 12] = [2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0];

    let mut builder = ProgramBuilder::new();
    // The compiler records a constant pubkey before the accounts' addresses.
    builder.pubkey(AUTHOR);
    let caller = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let system_program = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
    let protocol_program = builder.account(ACCOUNT_EXECUTABLE, Some(PROTOCOL_PROGRAM), None, 0);
    let pool = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let member_input = builder.input(VALUE_PUBKEY, 0);
    let allow_input = builder.input(VALUE_BOOL, 0);

    let allow = builder.load_input(allow_input);
    let author = builder.const_pubkey(AUTHOR);
    // True only in the author's runs: `caller` must sign, so only the author can match AUTHOR.
    // `isAuthor` is an expression, not a value, so the compiler writes it out at each use.
    let is_author = |builder: &mut ProgramBuilder| {
        let caller_key = builder.account_key(caller);
        builder.binary(OP_EQ, caller_key, author)
    };

    // The author's runs open the member's entry. Everyone else's runs open their own.
    let author_runs = is_author(&mut builder);
    let member = builder.load_input(member_input);
    let caller_key = builder.account_key(caller);
    let key = builder.select(author_runs, member, caller_key);
    // Registry 0 (`allowed`, one bool: 1 byte).
    builder.open_registry(entry, Some(key), caller, 0, 1, system_program);

    // The author-only branch: set the member's flag. Other runs write back the flag already there.
    let author_runs = is_author(&mut builder);
    let listed = builder.read_registry(entry, 0, OP_READ_BOOL);
    let flag = builder.select(author_runs, allow, listed);
    builder.write_registry(entry, 0, OP_READ_BOOL, flag);
    // Everyone but the author must be listed.
    let author_runs = is_author(&mut builder);
    let listed = builder.read_registry(entry, 0, OP_READ_BOOL);
    let may_run = builder.binary(OP_OR, author_runs, listed);
    builder.require(may_run);
    // The call the list guards. The author's runs skip it.
    let author_runs = is_author(&mut builder);
    let not_author = builder.not(author_runs);
    let call_data = builder.blob(&CALL_DATA);
    let call = builder.cpi(
        protocol_program,
        &[
            (caller, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (pool, ACCOUNT_WRITABLE),
        ],
        &[Segment::Literal(call_data)],
    );
    builder.invoke(call, Some(not_author));
    builder.build().expect("template builds")
}
// #endregion listed-callers-only

// #region run-listed-callers-only
/// `set` is for the author's runs only: the member to add or remove, and the flag to set.
pub fn run_listed_callers_only(
    template: Pubkey,
    caller: Pubkey,
    pool: Pubkey,
    set: Option<(Pubkey, bool)>,
) -> Instruction {
    use ballista_sdk::{
        find_registry_entry_address, run_instruction, RunInputs, SYSTEM_PROGRAM_ID,
    };
    use solana_program::instruction::AccountMeta;

    const PROTOCOL_PROGRAM: Pubkey = SYSTEM_PROGRAM_ID; // the same stand-in

    // The key the template computes: the member in the author's runs, the caller in everyone else's.
    let (member, allow) = set.unwrap_or((caller, false));
    let (entry, _) = find_registry_entry_address(&template, 0, &member.to_bytes());
    // Every run passes both inputs. Only the author's runs read them.
    let inputs = RunInputs::new().pubkey(&member).bool(allow).finish();
    let accounts = vec![
        AccountMeta::new(caller, true),
        AccountMeta::new(entry, false),
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new_readonly(PROTOCOL_PROGRAM, false),
        AccountMeta::new(pool, false),
    ];
    run_instruction(template, accounts, &inputs)
}
// #endregion run-listed-callers-only
