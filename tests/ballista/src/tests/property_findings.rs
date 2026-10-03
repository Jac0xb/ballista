//! Confirmation runs for findings from the safety-property review
//! (`docs/superpowers/specs/2026-10-03-safety-properties.md`). Each test names the property it
//! checks and pins today's behaviour, which a check or a document elsewhere states differently.

use super::*;
use ballista_common::template::{OP_READ_BOOL, SYSTEM_PROGRAM_ADDRESS};

const TYPE_MISMATCH: u32 = 6012;

/// P-RUN-ERR-2. `TypeMismatch` (6012) is not only structural. A `bool` read fails with it when the
/// byte it reads is neither 0 nor 1, a value the caller controls. The program generator's
/// `STRUCTURAL_RUNTIME_ERRORS` and the Certora typing rule's `value_dependent` both treat 6012 as
/// proof that the verifier accepted something the executor rejects, so an oracle built on them
/// misreports this run once a generator or rule reaches a data read.
#[test]
fn a_bool_read_of_a_byte_above_one_fails_with_type_mismatch() {
    let mut builder = ProgramBuilder::new();
    let data = builder.account(0, None, None, 1);
    let flag = builder.read(OP_READ_BOOL, data, 0);
    builder.require(flag);
    let payload = builder.build().expect("builds");
    ProgramView::parse(&payload)
        .and_then(|program| program.verify())
        .expect("a fixed-offset bool read inside the minimum length verifies");

    let creator = Pubkey::new_unique();
    let holder = Pubkey::new_unique();
    let context = context(funded_accounts([creator], 10_000_000_000));
    let created = context.process_instruction(&create_template_instruction(creator, 1, &payload));
    assert!(created.program_result.is_ok(), "{created:#?}");
    let template = find_template_pda(&creator, 1).0;

    let run_with = |byte: u8| {
        let mut account = Account::new(1_000_000, 1, &Pubkey::new_unique());
        account.data = vec![byte];
        context.account_store.borrow_mut().insert(holder, account);
        context.process_instruction(&run_instruction(
            template,
            vec![AccountMeta::new_readonly(holder, false)],
            &[],
        ))
    };
    assert!(run_with(1).program_result.is_ok());
    // pc 0 is the read; the value, not the template's shape, made it fail.
    assert_eq!(custom_code(&run_with(2)), Some(TYPE_MISMATCH));
}

/// P-RUN-ERR-1. A failure the run raises carries a Ballista code, with one documented exception
/// (a missing signer). An open registry entry's data is marked borrowed for the rest of the run,
/// so a fixed-offset read of that entry through another slot, which the caller fills with the same
/// account, fails in the borrow check before any Ballista code is attached: the run fails with
/// Solana's `AccountBorrowFailed`, which `docs/reference/errors.md` does not list.
#[test]
fn a_data_read_of_an_open_entry_through_another_slot_fails_without_a_ballista_code() {
    let mut builder = ProgramBuilder::new();
    let system = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
    let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    // Any read-only declaration with room for the field: the caller decides what fills it.
    let other = builder.account(0, None, None, 80);
    builder.open_registry(entry, None, payer, 0, 8, system);
    let field = builder.read(OP_READ_U64, other, 72);
    let zero = builder.const_u64(0);
    let same = builder.binary(OP_EQ, field, zero);
    builder.require(same);
    let payload = builder.build().expect("builds");
    ProgramView::parse(&payload)
        .and_then(|program| program.verify())
        .expect("the verifier refuses data reads of the entry slot only");

    let creator = Pubkey::new_unique();
    let payer_key = Pubkey::new_unique();
    let mut accounts = funded_accounts([creator, payer_key], 10_000_000_000);
    let unrelated = Pubkey::new_unique();
    accounts.insert(unrelated, Account::new(1_000_000, 80, &Pubkey::new_unique()));
    let context = context(accounts);
    let created = context.process_instruction(&create_template_instruction(creator, 2, &payload));
    assert!(created.program_result.is_ok(), "{created:#?}");
    let template = find_template_pda(&creator, 2).0;

    // The template-wide entry of registry 0, seeded so it holds its 80 bytes before the run.
    let entry_key = Pubkey::find_program_address(
        &[b"registry", template.as_ref(), &[0], &[0; 32]],
        &ID,
    )
    .0;
    let mut seeded = Account::new(
        context.mollusk.sysvars.rent.minimum_balance(80),
        80,
        &ID,
    );
    seeded.data[..8].copy_from_slice(b"BREG\x01\x00\x00\x00");
    seeded.data[8..40].copy_from_slice(template.as_ref());
    context.account_store.borrow_mut().insert(entry_key, seeded);

    let run = |fourth: Pubkey| {
        context.process_instruction(&run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(payer_key, true),
                AccountMeta::new(entry_key, false),
                AccountMeta::new_readonly(fourth, false),
            ],
            &[],
        ))
    };
    // A distinct account in the fourth slot runs.
    let distinct = run(unrelated);
    assert!(distinct.program_result.is_ok(), "{distinct:#?}");

    // The entry in both slots: the read fails, and the failure carries no Ballista code.
    let aliased = run(entry_key);
    assert_eq!(custom_code(&aliased), None, "{aliased:#?}");
    assert!(
        matches!(
            aliased.program_result,
            mollusk_svm::result::ProgramResult::Failure(
                solana_program_error::ProgramError::AccountBorrowFailed
            )
        ),
        "{aliased:#?}"
    );
}
