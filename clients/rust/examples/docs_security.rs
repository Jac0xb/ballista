//! The Rust snippet from the security pages that reads a stored template without its source.
//!
//! ```bash
//! cargo run -p ballista-sdk --example docs_security
//! ```
//!
//! `cargo test -p ballista-sdk` builds this file, so the snippet compiles. Running it compiles the
//! PDA and ATA assertions page's template, stores it in a finalized template account, and inspects
//! that account. The page includes the function by its `#region` name.

#![allow(dead_code)]

#[path = "docs_templates.rs"]
mod templates;

use solana_program::pubkey::Pubkey;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use ballista_sdk::ballista_common::template::TemplateAccountHeader;

    let compiled = templates::assert_recipient_ata().compile()?;
    let mut header =
        TemplateAccountHeader::new_uploading([1; 32], 0, 255, compiled.bytes.len(), compiled.hash)?;
    header.set_written_len(compiled.bytes.len())?;
    header.finalize()?;
    let mut account_data = header.as_bytes().to_vec();
    account_data.extend_from_slice(&compiled.bytes);
    inspect_template(&account_data)
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
