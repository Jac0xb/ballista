//! A design finding the second critic asked to test: the verifier's `refuse_entry_data`
//! (common/src/template/verify.rs) refuses a raw read only of the entry's *own* declared slot, and
//! the run-time borrow mark that guards an open entry exists only *after* the open. So a Rust-built
//! template can declare a second, ordinary read-only account, read the entry's raw bytes through
//! it, and make a CPI — all in the window before the open, with nothing protecting the entry.
//!
//! The docs promise the opposite (reference/language.md, Registries): "Fields are the only way in.
//! `accountData` and `accountDataBytes` of an entry are refused," and (trust-model.md) an entry is
//! protected "between this run's read and its write." This module demonstrates the enabling gap
//! concretely, and marks the safety property it breaks with an `#[ignore]`d regression test.
//!
//! Root cause: `refuse_entry_data` keys on the instruction's own account reference
//! (`record.a == instruction.a`) and only for a slot declared writable, so an aliased read-only
//! slot holding the same account at run time is neither scanned nor refused; and `OPEN_REGISTRY`'s
//! borrow mark, the only run-time guard, is set by the open, not before it. The TypeScript compiler
//! hoists all opens to the front and pins data-read accounts, so it never emits this shape, but the
//! program's verifier — the sole check for a hand-built template — accepts it.

use ballista_common::template::{
    ProgramBuilder, Segment, ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_U64,
    OP_READ_U64, SYSTEM_PROGRAM_ADDRESS,
};
use ballista_fuzz_gen::scenario::{entry_address, entry_header};
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;
use solana_sdk_ids::system_program;

use super::harness::{Harness, BALLISTA_ID};

const CREATOR: Pubkey = Pubkey::new_from_array([0x4c; 32]);
const PAYER: Pubkey = Pubkey::new_from_array([0x4d; 32]);
const REGISTRY_INDEX: u8 = 0;
const REGISTRY_SIZE: u16 = 16;
const PLANTED_FIELD0: u64 = 0xABCD_1234_5678_9A01;

/// A template that reads the entry's raw field-0 bytes through an aliased read-only slot, makes a
/// System CPI, then opens the entry — the ordering the verifier should forbid but does not. Returns
/// the compiled payload. Slots: 0 system, 1 payer, 2 entry (opened), 3 alias (read-only).
fn aliased_read_template() -> Vec<u8> {
    let mut builder = ProgramBuilder::new();
    let system = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
    let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    // The alias: a second writable account with a minimum length that covers field 0, pinning
    // neither owner nor address (the program's verifier does not require it; that is compiler-only).
    // `verify_open_registry` scans CPIs only for the entry's own slot, so routing through this one
    // evades it, and `refuse_entry_data` does not refuse a read of a slot no open names.
    let alias = builder.account(ACCOUNT_WRITABLE, None, None, (72 + 8) as u32);

    // 1. Read the entry's raw field-0 bytes through the alias, before any open.
    let raw = builder.read(OP_READ_U64, alias, 72);
    // 2. A CPI in the read->open window that passes the entry (through the alias slot) writable. A
    // zero-lamport System transfer here is benign, but the entry reaches a CPI writable with no
    // borrow mark yet set — the window a nested run could use to change it under the read.
    let literal = builder.blob(&{
        let mut data = vec![2, 0, 0, 0];
        data.extend_from_slice(&0u64.to_le_bytes());
        data
    });
    let transfer = builder.cpi(
        system,
        &[(payer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE), (alias, ACCOUNT_WRITABLE)],
        &[Segment::Literal(literal)],
    );
    builder.invoke(transfer, None);
    // 3. Only now open the entry.
    builder.open_registry(entry, None, payer, REGISTRY_INDEX, REGISTRY_SIZE, system);
    // 4. Hand back what the raw aliased read saw, so the test can confirm it read the entry.
    builder.set_return_data(&[Segment::Register(DATA_REG_U64, raw)]);
    builder.build().unwrap()
}


/// Uploads the template and seeds an existing entry holding `PLANTED_FIELD0` in field 0, then runs
/// it. Returns the run result. The entry must already exist and be at least 80 bytes so the alias
/// slot's minimum-length check passes.
fn run_aliased_read(harness: &Harness, id: u16) -> mollusk_svm::result::types::InstructionResult {
    harness.reset();
    let payload = aliased_read_template();
    let (template, _) = harness.upload(&CREATOR, id, &payload).expect("the aliased-read template verifies and uploads");

    let key = [0u8; 32];
    let ballista = BALLISTA_ID.to_bytes();
    let entry = Pubkey::new_from_array(entry_address(
        &|seeds, program| {
            let (address, bump) = Pubkey::find_program_address(seeds, &Pubkey::new_from_array(*program));
            (address.to_bytes(), bump)
        },
        &ballista,
        &template.to_bytes(),
        REGISTRY_INDEX,
        &key,
    ));

    // Build the existing entry: the correct header, then fields, with field 0 set to the planted
    // value so a successful raw read returns exactly it.
    let mut data = entry_header(&template.to_bytes(), REGISTRY_INDEX, &key).to_vec();
    data.extend_from_slice(&PLANTED_FIELD0.to_le_bytes());
    data.extend_from_slice(&[0u8; (REGISTRY_SIZE as usize) - 8]);
    let rent = harness.rent_minimum(data.len());
    {
        let mut store = harness.context.account_store.borrow_mut();
        store.insert(PAYER, Account::new(10_000_000_000, 0, &system_program::id()));
        store.insert(entry, Account { lamports: rent, data, owner: BALLISTA_ID, executable: false, rent_epoch: 0 });
    }

    // Accounts: template, system, payer, entry (writable, slot 2), entry again (read-only, slot 3).
    let instruction = Instruction {
        program_id: BALLISTA_ID,
        accounts: vec![
            AccountMeta::new_readonly(template, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(PAYER, true),
            AccountMeta::new(entry, false),
            // The alias slot, writable, holding the same entry account.
            AccountMeta::new(entry, false),
        ],
        data: vec![ballista_common::instruction::IX_RUN],
    };
    harness.context.process_instruction(&instruction)
}

/// Demonstrates the enabling gap: the template verifies, uploads and runs, and the raw aliased read
/// returns the entry's own field-0 bytes — proving `accountData` of an entry is reachable through
/// an alias, and that a CPI runs in the read->open window. This passes against the program today.
#[test]
fn aliased_entry_read_and_window_cpi_are_accepted_today() {
    let harness = Harness::new();
    let result = run_aliased_read(&harness, 1);
    assert!(
        result.program_result.is_ok(),
        "the aliased-read template runs today (the gap): {result:?}"
    );
    assert_eq!(
        result.return_data,
        PLANTED_FIELD0.to_le_bytes(),
        "the raw aliased read returned the entry's field-0 bytes, bypassing the field interface"
    );
}

/// The safety property the docs promise: a template must not be able to read an entry's raw data
/// through an aliased slot, and nothing may touch an entry in the read->open window. If the
/// verifier refused this shape (as the compiler does), uploading it would fail. It does not today,
/// so this regression test fails; it is `#[ignore]`d until the owner closes the gap.
///
/// FINDING (registry-ordering / aliased entry read): `refuse_entry_data` guards only the entry's
/// own declared slot and only when that slot is writable, so a second read-only slot aliased to the
/// entry reads its raw bytes; and `OPEN_REGISTRY`'s borrow mark exists only after the open, so a
/// CPI in the read->open window is unguarded. Together these let a hand-built template read an
/// entry outside the field interface and act on a value a concurrent write could change (a
/// lost-update window). The fix would refuse a raw read or a CPI-writable pass of any account that
/// an open also names, by address at run time, not only by the entry's own slot reference.
#[test]
#[ignore = "documents the registry-ordering finding; fails until refuse_entry_data covers aliased slots"]
fn aliased_entry_read_should_be_refused() {
    let harness = Harness::new();
    let payload = aliased_read_template();
    harness.reset();
    // The safe behavior: the program rejects the template at create (as the compiler does), so the
    // upload fails. Today it succeeds, so this assertion fails, marking the open finding.
    let uploaded = harness.upload(&CREATOR, 2, &payload);
    assert!(
        uploaded.is_err(),
        "a template that reads an entry's raw data through an aliased slot should be refused at create, \
         but it verified and uploaded: the refuse_entry_data gap is open"
    );
}
