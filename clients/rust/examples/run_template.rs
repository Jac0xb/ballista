//! Encode inputs, build a `Run` instruction, and decode a failure code from Rust.
//!
//! ```bash
//! cargo run -p ballista-sdk --example run_template
//! ```
//!
//! Runtime accounts follow the template's schema order: fixed accounts first, then each batch row.
//! Inputs follow the template's input order. Both come from the template author; a Rust service
//! only needs to know that order, not the bytecode.

use ballista_sdk::{
    decode_ballista_error, find_template_pda, run_instruction, ErrorSource, RunInputs,
    SYSTEM_PROGRAM_ID,
};
use solana_program::{instruction::AccountMeta, pubkey::Pubkey};

fn main() {
    let creator = Pubkey::new_unique();
    let treasury = Pubkey::new_unique();
    let recipients: Vec<Pubkey> = (0..3).map(|_| Pubkey::new_unique()).collect();
    let (template, _) = find_template_pda(&creator, 1);

    // The budgeted payroll template from `author_template.rs` declares inputs `amount`, `budget`
    // and accounts `systemProgram`, `treasury`, then one `recipient` per row.
    let inputs = RunInputs::new().u64(50_000).u64(150_000).finish();
    let mut accounts = vec![
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new(treasury, true),
    ];
    accounts.extend(recipients.iter().map(|key| AccountMeta::new(*key, false)));
    let run = run_instruction(template, accounts, &inputs);

    println!("program          {}", run.program_id);
    println!("template         {}", run.accounts[0].pubkey);
    println!("runtime accounts {}", run.accounts.len() - 1);
    println!("rows             {}", recipients.len());
    println!("data bytes       {} (discriminator + inputs)", run.data.len());
    println!("data hex         {}", hex(&run.data));

    // A failed run returns a custom error whose low 16 bits name the failure and whose high 16
    // bits locate it: the program counter for VM failures, the account index for constraint
    // failures, or the input index for decoding failures.
    for code in [(7u32 << 16) | 6015, (3 << 16) | 6020, 6008, (2 << 16) | 6115, 1] {
        match decode_ballista_error(code) {
            Some(decoded) => {
                let where_ = match (decoded.source, decoded.name) {
                    (ErrorSource::Verifier, _) => "verifier context",
                    (_, "AccountConstraintFailed") => "runtime account index",
                    (_, "InvalidRunInputs") => "input index",
                    (_, "InvalidAccountRange") => "accounts or rows supplied",
                    _ => "program counter",
                };
                println!(
                    "0x{code:08x} -> {} ({where_} {})",
                    decoded.name, decoded.context
                );
            }
            None => println!("0x{code:08x} -> not a Ballista error; raised by an invoked program"),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
