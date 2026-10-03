//! The Rust snippets from the security pages: reading a stored template without its source, and
//! asserting an associated token account.
//!
//! ```bash
//! cargo run -p ballista-sdk --example docs_security
//! ```
//!
//! `cargo test -p ballista-sdk` builds this file, so every snippet compiles. Running it builds the
//! ATA template, stores it in a finalized template account, inspects that account, and builds a
//! run. The pages include each function by its `#region` name.
//!
//! The ATA template and its run are twins of the TypeScript tabs beside them on the PDA and ATA
//! assertions page, `clients/js/examples/docs/assert-recipient-ata.ts`: `tests/docs_examples.rs`
//! holds them to the same bytes, run data and account flags.

#![allow(dead_code)]

use solana_program::{instruction::Instruction, pubkey::Pubkey};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use ballista_sdk::{ballista_common::template::TemplateAccountHeader, template_hash};

    let payload = assert_recipient_ata();
    let mut header = TemplateAccountHeader::new_uploading(
        [1; 32],
        0,
        255,
        payload.len(),
        template_hash(&payload),
    )?;
    header.set_written_len(payload.len())?;
    header.finalize()?;
    let mut account_data = header.as_bytes().to_vec();
    account_data.extend_from_slice(&payload);
    inspect_template(&account_data)?;

    for (name, run) in RUNS {
        println!("{name} run: {} accounts", run(0).accounts.len());
    }
    Ok(())
}

/// An example's name and the function that builds its template.
pub type Example = (&'static str, fn() -> Vec<u8>);

/// The twin templates in this file, by the name `docs-examples.test.ts` records them under.
pub const TEMPLATES: &[Example] = &[("assert-recipient-ata", assert_recipient_ata)];

/// An example's name and a function that builds its run for a given number of batch rows. These
/// templates have no batch, so every run ignores it.
pub type ExampleRun = (&'static str, fn(usize) -> Instruction);

/// Their runs, with the inputs the TypeScript runs in `docs-examples.test.ts` use.
pub const RUNS: &[ExampleRun] = &[("assert-recipient-ata", |_| {
    run_assert_recipient_ata(key(200), key(1), key(2), key(3))
})];

/// A distinct placeholder key per position, standing in for real addresses.
fn key(index: u8) -> Pubkey {
    Pubkey::new_from_array([index + 1; 32])
}

// #region inspect-template
/// Print what a stored template asks of each account, and what each of its calls passes.
pub fn inspect_template(account_data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    use ballista_sdk::ballista_common::template::{
        TemplateAccount, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, NO_INDEX,
    };

    let account = TemplateAccount::parse(account_data)?;
    let program = account.finalized_program()?;
    let stats = program.verify()?; // the same checks the program ran at finalization

    let pinned = |index: u8| {
        (index != NO_INDEX).then(|| Pubkey::new_from_array(program.pubkeys[index as usize].bytes))
    };
    for (index, constraint) in program.accounts.iter().enumerate() {
        println!(
            "account {index}: signer={} writable={} address={:?} owner={:?} min_len={}",
            constraint.flags & ACCOUNT_SIGNER != 0,
            constraint.flags & ACCOUNT_WRITABLE != 0,
            pinned(constraint.address_index),
            pinned(constraint.owner_index),
            constraint.min_data_len(),
        );
    }
    for (call, cpi) in program.cpis.iter().enumerate() {
        let passed = &program.cpi_accounts[cpi.account_start()..][..cpi.account_len as usize];
        let signers: Vec<u8> = passed
            .iter()
            .filter(|meta| meta.flags & ACCOUNT_SIGNER != 0)
            .map(|meta| meta.account)
            .collect();
        println!(
            "call {call}: program in account {}, signers {signers:?}, account group {:?}",
            cpi.program_account,
            cpi.account_group(),
        );
    }
    println!("at most {} calls per run", stats.max_expanded_cpis);
    Ok(())
}
// #endregion inspect-template

// #region assert-ata-template
/// Require `destination_ata` to be the associated token account of the recipient and mint.
pub fn assert_recipient_ata() -> Vec<u8> {
    use ballista_sdk::{
        ballista_common::template::{ACCOUNT_EXECUTABLE, ACCOUNT_WRITABLE, DATA_REG_PUBKEY, OP_EQ},
        ProgramBuilder, Segment, ASSOCIATED_TOKEN_PROGRAM_ID,
    };

    let mut builder = ProgramBuilder::new();
    let associated_token_program = builder.account(
        ACCOUNT_EXECUTABLE,
        Some(ASSOCIATED_TOKEN_PROGRAM_ID.to_bytes()),
        None,
        0,
    );
    let token_program = builder.account(0, None, None, 0);
    let recipient = builder.account(0, None, None, 0);
    let mint = builder.account(0, None, None, 0);
    let destination_ata = builder.account(ACCOUNT_WRITABLE, None, None, 0);

    // Derive the ATA from [owner, token program, mint] and require the passed account to match.
    let ata_key = builder.account_key(destination_ata);
    let owner_key = builder.account_key(recipient);
    let token_program_key = builder.account_key(token_program);
    let mint_key = builder.account_key(mint);
    let derived = builder.derive_pda(
        associated_token_program,
        &[
            Segment::Register(DATA_REG_PUBKEY, owner_key),
            Segment::Register(DATA_REG_PUBKEY, token_program_key),
            Segment::Register(DATA_REG_PUBKEY, mint_key),
        ],
    );
    let matches = builder.binary(OP_EQ, ata_key, derived);
    builder.require(matches);
    builder.build().expect("template builds")
}
// #endregion assert-ata-template

// #region assert-ata-run
/// Derive the ATA as the template does, and pass the accounts in the order it declares them.
pub fn run_assert_recipient_ata(
    template: Pubkey,
    recipient: Pubkey,
    mint: Pubkey,
    token_program: Pubkey,
) -> Instruction {
    use ballista_sdk::{run_instruction, ASSOCIATED_TOKEN_PROGRAM_ID};
    use solana_program::instruction::AccountMeta;

    let (destination_ata, _bump) = Pubkey::find_program_address(
        &[recipient.as_ref(), token_program.as_ref(), mint.as_ref()],
        &ASSOCIATED_TOKEN_PROGRAM_ID,
    );
    let accounts = vec![
        AccountMeta::new_readonly(ASSOCIATED_TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(token_program, false),
        AccountMeta::new_readonly(recipient, false),
        AccountMeta::new_readonly(mint, false),
        AccountMeta::new(destination_ata, false),
    ];
    run_instruction(template, accounts, &[])
}
// #endregion assert-ata-run
