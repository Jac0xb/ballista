//! The examples on the Rust SDK reference page, `docs/reference/rust.md`, which includes each by its
//! `#region` name. `tests/docs_rust_reference.rs` runs them: the templates compile and verify, the
//! instructions carry what the page says, and the upload sizes fit a transaction.

#![allow(dead_code)]

use ballista_sdk::ballista_common::template::VerificationStats;
use ballista_sdk::template::Template;
use solana_program::{instruction::Instruction, pubkey::Pubkey};

fn main() {}

// #region author
/// Pays `amount` lamports to each batch row's recipient, keeps a running total, and requires the
/// total to stay within `budget`.
pub fn payroll() -> Template {
    use ballista_sdk::template::prelude::*;

    Template::new()
        .input("amount", Type::U64)
        .input("budget", Type::U64)
        .account("systemProgram", account::program(SYSTEM_PROGRAM_ID))
        .account("treasury", account::signer().writable())
        .batch(
            Batch::new(30)
                .min_iterations(1)
                .account("recipient", account::writable()),
        )
        .step(step::let_("total", u64(0)))
        .step(
            step::for_each()
                .step(system_transfer(
                    "systemProgram",
                    "treasury",
                    account::iteration("recipient"),
                    input("amount"),
                ))
                .step(step::assign("total", var("total") + input("amount")))
                .carry("total"),
        )
        .step(step::require(var("total").lte(input("budget"))).label("withinBudget"))
}
// #endregion author

// #region output
/// Ends a template like the payroll above: logs the tag `PAID` and `total`, and returns `total`
/// to the caller. `emit_event` makes every successful run log its run event as well.
pub fn log_and_return(template: Template) -> Template {
    use ballista_sdk::template::prelude::*;

    template
        .emit_event()
        .step(step::emit([
            data::literal(b"PAID"),
            data::u64(var("total")),
        ]))
        .step(step::set_return_data([data::u64(var("total"))]))
}
// #endregion output

// #region introspection
/// Requires the instruction just before the run's to call the Ed25519 program.
pub fn after_an_ed25519_instruction() -> Template {
    use ballista_sdk::template::prelude::*;

    let previous = current_instruction_index("instructions") - u64(1);
    Template::new()
        .account(
            "instructions",
            account::readonly().address(INSTRUCTIONS_SYSVAR_ID),
        )
        .step(step::require(
            instruction_program("instructions", previous).eq(pubkey(ED25519_PROGRAM_ID)),
        ))
}
// #endregion introspection

// #region registry
/// Adds `amount` to the caller's running total, which must stay within 1 SOL. The total lives in
/// an entry of `totals` keyed by the caller, who pays for the entry the first time.
pub fn capped_total() -> Template {
    use ballista_sdk::template::prelude::*;

    let total = registry("callerTotal", "sent") + input("amount");
    Template::new()
        .input("amount", Type::U64)
        .registry("totals", [("sent", Type::U64)])
        .account("caller", account::signer().writable())
        .account(
            "callerTotal",
            account::registry("totals", "caller").key(account_key("caller")),
        )
        .account("systemProgram", account::system_program())
        .step(step::require(total.lte(u64(1_000_000_000))).label("withinCap"))
        .step(step::set_registry("callerTotal", "sent", total))
}
// #endregion registry

// #region own-program
/// Calls `deposit(amount: u64)` on your own Anchor program, with the vault writable.
pub fn deposit_into(my_program: Pubkey) -> Template {
    use ballista_sdk::{anchor_discriminator, template::prelude::*};

    Template::new()
        .input("amount", Type::U64)
        .account("myProgram", account::program(my_program))
        .account("vault", account::writable())
        .step(
            step::invoke("myProgram")
                .writable("vault")
                // Anchor instruction data: the handler's discriminator, then its arguments in
                // Borsh order.
                .data(data::literal(anchor_discriminator("deposit")))
                .data(data::u64(input("amount"))),
        )
}
// #endregion own-program

// #region addresses
/// The template `creator` publishes as `template_id`, the same ID under another deployment, and
/// `caller`'s entry of the template's registry `totals`.
pub fn addresses(
    creator: &Pubkey,
    template_id: u16,
    other_program_id: &Pubkey,
    caller: &Pubkey,
) -> Result<[Pubkey; 3], Box<dyn std::error::Error>> {
    use ballista_sdk::{
        find_registry_entry_address, find_template_pda, find_template_pda_for_program,
    };

    let (template, _bump) = find_template_pda(creator, template_id);
    let (elsewhere, _) = find_template_pda_for_program(creator, template_id, other_program_id);
    // A registry's index is its position among the template's registries.
    let totals = capped_total().compile()?.registry_index("totals").unwrap();
    let (entry, _) = find_registry_entry_address(&template, totals, &caller.to_bytes());
    Ok([template, elsewhere, entry])
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
) -> Result<Instruction, Box<dyn std::error::Error>> {
    use ballista_sdk::{template::Row, SYSTEM_PROGRAM_ID};

    let instruction = payroll()
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
