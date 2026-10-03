//! The examples on the Rust SDK reference page, `docs/reference/rust.md`, which includes each by its
//! `#region` name. `tests/docs_rust_reference.rs` runs them: the templates verify, the instructions
//! carry what the page says, and the upload sizes fit a transaction.

#![allow(dead_code)]

use ballista_sdk::{
    ballista_common::template::{TemplateError, VerificationStats},
    ProgramBuilder,
};
use solana_program::{instruction::Instruction, pubkey::Pubkey};

fn main() {}

// #region author
/// Pays `amount` lamports to each batch row's recipient through the System Program, keeps a
/// running total, and requires the total to stay within `budget`.
pub fn payroll() -> Result<Vec<u8>, TemplateError> {
    use ballista_sdk::{
        ballista_common::template::{
            ProgramView, ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_U64,
            OP_ADD, OP_LTE, VALUE_U64,
        },
        ProgramBuilder, Segment, SYSTEM_PROGRAM_ID,
    };

    let mut builder = ProgramBuilder::new();
    let system = builder.account(
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
    let discriminator = builder.blob(&[2, 0, 0, 0]); // System Program Transfer
    let transfer = builder.cpi(
        system,
        &[
            (treasury, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (recipient, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(discriminator),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    builder.for_each(1 << total, |body| {
        body.invoke(transfer, None);
        let sum = body.binary(OP_ADD, total, amount);
        body.mov(total, sum);
    });
    let within = builder.binary(OP_LTE, total, budget);
    builder.require(within);

    let payload = builder.build()?;
    ProgramView::parse(&payload)?.verify()?;
    Ok(payload)
}
// #endregion author

// #region output
/// Ends a template like the payroll above: logs the tag `PAID` and `total`, and returns `total`
/// to the caller. The flag makes every successful run log its run event as well.
pub fn log_and_return(builder: &mut ProgramBuilder, total: u8) {
    use ballista_sdk::{
        ballista_common::template::{DATA_REG_U64, PROGRAM_FLAG_EMIT_EVENT},
        Segment,
    };

    builder.flags(PROGRAM_FLAG_EMIT_EVENT);
    let tag = builder.blob(b"PAID");
    builder.emit_data(&[
        Segment::Literal(tag),
        Segment::Register(DATA_REG_U64, total),
    ]);
    builder.set_return_data(&[Segment::Register(DATA_REG_U64, total)]);
}
// #endregion output

// #region introspection
/// Requires the instruction just before the run's to call the Ed25519 program.
pub fn after_an_ed25519_instruction() -> Result<Vec<u8>, TemplateError> {
    use ballista_sdk::{
        ballista_common::template::{
            NO_INDEX, OP_EQ, OP_INSTRUCTION_INDEX, OP_INSTRUCTION_PROGRAM, OP_SUB,
        },
        ProgramBuilder, ED25519_PROGRAM_ID, INSTRUCTIONS_SYSVAR_ID,
    };

    let mut builder = ProgramBuilder::new();
    let sysvar = builder.account(0, Some(INSTRUCTIONS_SYSVAR_ID.to_bytes()), None, 0);
    let one = builder.const_u64(1);
    let ed25519 = builder.const_pubkey(ED25519_PROGRAM_ID.to_bytes());

    let current = builder.introspect(OP_INSTRUCTION_INDEX, sysvar, NO_INDEX, NO_INDEX);
    let previous = builder.binary(OP_SUB, current, one);
    let program = builder.introspect(OP_INSTRUCTION_PROGRAM, sysvar, previous, NO_INDEX);
    let is_ed25519 = builder.binary(OP_EQ, program, ed25519);
    builder.require(is_ed25519);
    builder.build()
}
// #endregion introspection

// #region registry
/// Adds `amount` to the caller's running total, which must stay within 1 SOL. The total lives in
/// registry 0, one `u64` keyed by the caller, who pays for the entry the first time.
pub fn capped_total() -> Result<Vec<u8>, TemplateError> {
    use ballista_sdk::{
        ballista_common::template::{
            ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, OP_ADD, OP_LTE, OP_READ_U64,
            VALUE_U64,
        },
        ProgramBuilder, SYSTEM_PROGRAM_ID,
    };

    let mut builder = ProgramBuilder::new();
    let system = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(SYSTEM_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let caller = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    // The entry: declared writable and nothing else.
    let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let amount_input = builder.input(VALUE_U64, 0);
    let amount = builder.load_input(amount_input);
    let cap = builder.const_u64(1_000_000_000);

    let key = builder.account_key(caller);
    builder.open_registry(entry, Some(key), caller, 0, 8, system);
    let sent = builder.read_registry(entry, 0, OP_READ_U64);
    let total = builder.binary(OP_ADD, sent, amount);
    let within = builder.binary(OP_LTE, total, cap);
    builder.require(within);
    builder.write_registry(entry, 0, OP_READ_U64, total);
    builder.build()
}
// #endregion registry

// #region own-program
/// Calls `deposit(amount: u64)` on your own Anchor program, with the vault writable.
pub fn deposit_into(my_program: Pubkey) -> Result<Vec<u8>, TemplateError> {
    use ballista_sdk::{
        anchor_discriminator,
        ballista_common::template::{
            ACCOUNT_EXECUTABLE, ACCOUNT_WRITABLE, DATA_REG_U64, VALUE_U64,
        },
        ProgramBuilder, Segment,
    };

    let mut builder = ProgramBuilder::new();
    // A template holds an address as its 32 bytes.
    let program = builder.account(ACCOUNT_EXECUTABLE, Some(my_program.to_bytes()), None, 0);
    let vault = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let amount_input = builder.input(VALUE_U64, 0);
    let amount = builder.load_input(amount_input);
    // Anchor instruction data: the handler's discriminator, then its arguments in Borsh order.
    let deposit = builder.blob(&anchor_discriminator("deposit"));
    let call = builder.cpi(
        program,
        &[(vault, ACCOUNT_WRITABLE)],
        &[
            Segment::Literal(deposit),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    builder.invoke(call, None);
    builder.build()
}
// #endregion own-program

// #region addresses
/// The template `creator` publishes as `template_id`, the same ID under another deployment, and
/// `caller`'s entry of the template's registry 0.
pub fn addresses(
    creator: &Pubkey,
    template_id: u16,
    other_program_id: &Pubkey,
    caller: &Pubkey,
) -> [Pubkey; 3] {
    use ballista_sdk::{
        find_registry_entry_address, find_template_pda, find_template_pda_for_program,
    };

    let (template, _bump) = find_template_pda(creator, template_id);
    let (elsewhere, _) = find_template_pda_for_program(creator, template_id, other_program_id);
    let (entry, _) = find_registry_entry_address(&template, 0, &caller.to_bytes());
    [template, elsewhere, entry]
}
// #endregion addresses

// #region upload
/// The instructions that upload `payload` as `creator`'s template `id`, one per transaction, in
/// order: a single create when the payload fits, or begin, a write per chunk, and finalize.
pub fn upload(creator: Pubkey, id: u16, payload: &[u8]) -> Vec<Instruction> {
    use ballista_sdk::{
        begin_template_instruction, create_template_instruction, finalize_template_instruction,
        find_template_pda, template_hash, write_template_chunk_instruction,
    };

    // The most a legacy transaction holds when the creator signs it, pays the fee, and it carries
    // only this instruction. A v0 transaction holds 2 bytes less of each.
    const ONE_SHOT_LEN: usize = 960;
    const CHUNK_LEN: usize = 1_023;

    if payload.len() <= ONE_SHOT_LEN {
        return vec![create_template_instruction(creator, id, payload)];
    }
    let (template, _) = find_template_pda(&creator, id);
    let (length, hash) = (payload.len() as u32, template_hash(payload));
    let mut instructions = vec![begin_template_instruction(creator, id, length, hash)];
    for (index, chunk) in payload.chunks(CHUNK_LEN).enumerate() {
        let offset = (index * CHUNK_LEN) as u32;
        let write = write_template_chunk_instruction(creator, template, offset, chunk);
        instructions.push(write);
    }
    instructions.push(finalize_template_instruction(creator, template));
    instructions
}
// #endregion upload

// #region run
/// A run of the payroll template above: `amount` to each recipient, within `budget`.
pub fn run_payroll(
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
    accounts.extend(recipients.iter().map(|key| AccountMeta::new(*key, false)));
    run_instruction(template, accounts, &inputs)
}
// #endregion run

// #region run-output
/// What the runs in a transaction logged: run events, and the `PAID` totals the template above
/// emits. `logs` are the transaction's log lines.
pub fn print_run_output(logs: &[String]) -> Result<(), ballista_sdk::LogError> {
    use ballista_sdk::{program_data, BallistaOutput};

    for line in program_data(logs)? {
        match line.ballista_output(&ballista_sdk::ID) {
            Some(BallistaOutput::RunEvent(event)) => println!(
                "{} ran {} of the {} invokes it reached (mask {:#b})",
                event.template_address,
                event.executed.count_ones(),
                event.expanded,
                event.executed,
            ),
            Some(BallistaOutput::Emit(bytes)) if bytes.starts_with(b"PAID") => {
                let total = u64::from_le_bytes(bytes[4..12].try_into().unwrap());
                println!("paid {total} lamports, at stack height {}", line.height);
            }
            _ => {}
        }
    }
    Ok(())
}

/// The total a payroll run returned, from the transaction's return data as a simulation or the
/// transaction's metadata reports it: the program that set it, and the bytes.
pub fn returned_total(program_id: &Pubkey, data: &[u8]) -> Option<u64> {
    // Only the program that set the bytes counts: a run that sets none leaves a callee's.
    if *program_id != ballista_sdk::ID {
        return None;
    }
    Some(u64::from_le_bytes(data.get(..8)?.try_into().ok()?))
}
// #endregion run-output

// #region decode-failure
/// Names a failed transaction's custom error `code`, and where it happened, if it is Ballista's.
pub fn describe(code: u32) -> Option<String> {
    use ballista_sdk::{decode_ballista_error, ErrorSource};

    let error = decode_ballista_error(code)?;
    let context = match (error.source, error.name) {
        (ErrorSource::Verifier, _) => "verifier context",
        (_, "AccountConstraintFailed") => "runtime account index",
        (_, "InvalidRunInputs") => "input index",
        (_, "InvalidAccountRange") => "accounts or rows supplied",
        (_, "CpiAccountLimitExceeded") => "accounts in the call",
        _ => "program counter",
    };
    Some(format!("{} ({context} {})", error.name, error.context))
}
// #endregion decode-failure

// #region template-account
/// Checks the bytecode of a finalized template, read from its account's data.
pub fn inspect(account_data: &[u8]) -> Result<VerificationStats, Box<dyn std::error::Error>> {
    use ballista_sdk::ballista_common::template::TemplateAccount;

    let account = TemplateAccount::parse(account_data)?;
    let program = account.finalized_program()?;
    Ok(program.verify()?)
}
// #endregion template-account
