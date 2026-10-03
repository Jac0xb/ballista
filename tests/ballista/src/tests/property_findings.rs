//! Confirmation runs for findings from the safety-property review
//! (`docs/superpowers/specs/2026-10-03-safety-properties.md`). Each test names the property it
//! checks and pins today's behaviour, which a check or a document elsewhere states differently.

use super::*;
use ballista_common::template::{OP_READ_BOOL, SYSTEM_PROGRAM_ADDRESS};

const TYPE_MISMATCH: u32 = 6012;

/// P40 and P80, finding F2. `TypeMismatch` (6012) is not only structural. A `bool` read fails with
/// it when the byte it reads is neither 0 nor 1, a value the caller controls. The program
/// generator's `STRUCTURAL_RUNTIME_ERRORS` and the Certora typing rule's `value_dependent` both
/// treat 6012 as proof that the verifier accepted something the executor rejects, so an oracle
/// built on them misreports this run once a generator or rule reaches a data read.
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

/// P51, finding F6. The privilege ceiling is per slot, not per address. A template that pins
/// `vault` read-only still lets a CPI pass that address writable, through an account group (members
/// take the writable flag of Ballista's own instruction) or through another writable slot that the
/// caller fills with it. `language.md` says the schema "tells you the most any run of it can do
/// with the accounts it declares"; for an address that also fills a group or another slot, it does
/// not.
#[test]
fn a_pinned_read_only_account_reaches_a_cpi_writable_through_another_slot() {
    let creator = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let vault = Pubkey::new_unique();
    let mut accounts = funded_accounts([creator, payer], 10_000_000_000);
    accounts.insert(vault, Account::new(1_000_000, 0, &system_program::id()));
    let context = context(accounts);

    // System transfer of 1,000 lamports from `payer`; the recipient is the first member of group
    // 0, or the writable slot `destination`.
    let template = |id: u16, through_group: bool| {
        let mut builder = ProgramBuilder::new();
        if through_group {
            builder.account_groups(1);
        }
        let system = builder.account(ACCOUNT_EXECUTABLE, Some([0; 32]), None, 0);
        let from = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        // Read-only and pinned: the declaration a reviewer would read as "never written".
        builder.account(0, Some(vault.to_bytes()), None, 0);
        let discriminator = builder.blob(&[2, 0, 0, 0]);
        let amount = builder.blob(&1_000u64.to_le_bytes());
        let data = [Segment::Literal(discriminator), Segment::Literal(amount)];
        let transfer = if through_group {
            builder.cpi_with_group(
                system,
                &[(from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE)],
                &data,
                0,
            )
        } else {
            let destination = builder.account(ACCOUNT_WRITABLE, None, None, 0);
            builder.cpi(
                system,
                &[
                    (from, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                    (destination, ACCOUNT_WRITABLE),
                ],
                &data,
            )
        };
        builder.invoke(transfer, None);
        let payload = builder.build().expect("builds");
        ProgramView::parse(&payload)
            .and_then(|program| program.verify())
            .expect("verifies");
        let created =
            context.process_instruction(&create_template_instruction(creator, id, &payload));
        assert!(created.program_result.is_ok(), "{created:#?}");
        find_template_pda(&creator, id).0
    };
    let declared = vec![
        AccountMeta::new_readonly(system_program::id(), false),
        AccountMeta::new(payer, true),
        AccountMeta::new(vault, false),
    ];

    // Through the group: the template declares no writable slot the vault could fill.
    let grouped = template(1, true);
    let mut metas = declared.clone();
    metas.push(AccountMeta::new(vault, false));
    let before = lamports(&context, vault);
    let result = context.process_instruction(&run_instruction(grouped, metas, &[1]));
    assert!(result.program_result.is_ok(), "{result:#?}");
    assert_eq!(
        lamports(&context, vault),
        before + 1_000,
        "the CPI wrote the pinned read-only vault"
    );

    // Through another slot: the caller fills the writable `destination` with the vault.
    let aliased = template(2, false);
    let mut metas = declared;
    metas.push(AccountMeta::new(vault, false));
    let before = lamports(&context, vault);
    let result = context.process_instruction(&run_instruction(aliased, metas, &[]));
    assert!(result.program_result.is_ok(), "{result:#?}");
    assert_eq!(lamports(&context, vault), before + 1_000);
}

/// P18, finding F8. The verifier bounds instructions and CPIs, not compute. This template has no
/// CPI, 22 instructions and one count loop; at its maximum of 255 passes it runs 5,100 bump
/// searches and exhausts the 1.4M compute-unit budget. `language.md` says a template's worst case
/// is known "rather than being cut off by the transaction's compute budget"; only its step and CPI
/// counts are.
#[test]
fn a_verified_template_without_cpis_can_exhaust_the_compute_budget() {
    let mut builder = ProgramBuilder::new();
    let system = builder.account(ACCOUNT_EXECUTABLE, Some([0; 32]), None, 0);
    let seed = builder.blob(b"seed");
    let passes = builder.const_u64(255);
    builder.repeat(passes, 255, 0, |body| {
        for _ in 0..20 {
            body.derive_pda(system, &[Segment::Literal(seed)]);
        }
    });
    let payload = builder.build().expect("builds");
    let stats = ProgramView::parse(&payload)
        .and_then(|program| program.verify())
        .expect("verifies");
    assert_eq!((stats.instructions, stats.max_expanded_cpis), (22, 0));

    let creator = Pubkey::new_unique();
    let context = context(funded_accounts([creator], 10_000_000_000));
    let created = context.process_instruction(&create_template_instruction(creator, 3, &payload));
    assert!(created.program_result.is_ok(), "{created:#?}");
    let template = find_template_pda(&creator, 3).0;
    let result = context.process_instruction(&run_instruction(
        template,
        vec![AccountMeta::new_readonly(system_program::id(), false)],
        &[],
    ));
    assert!(result.program_result.is_err(), "{result:#?}");
    assert_eq!(
        custom_code(&result),
        None,
        "Solana's meter stops it, not a Ballista check"
    );
    assert_eq!(
        result.compute_units_consumed, context.mollusk.compute_budget.compute_unit_limit,
        "the whole budget is spent"
    );
}

/// P12, finding F7. Anyone can make an account Ballista-owned: the System program's `CreateAccount`
/// with Ballista as the owner needs only the new account's signature. `trust-model.md` says
/// templates and registry entries are the only accounts Ballista owns; this one is neither. It
/// holds zeros, and every instruction refuses it, so it stays inert: neither a template nor an
/// entry, and its lamports stay put.
#[test]
fn an_account_someone_else_makes_ballista_owned_is_refused_everywhere() {
    const INVALID_TEMPLATE_ACCOUNT: u32 = 6001;
    const INVALID_REGISTRY_ENTRY: u32 = 6025;
    let creator = Pubkey::new_unique();
    let stranger = Pubkey::new_unique();
    let context = context(funded_accounts([creator], 10_000_000_000));

    // 88 bytes: long enough to parse as a template header, and exactly the size of a 16-byte entry.
    let space = 88u64;
    let rent = context.mollusk.sysvars.rent.minimum_balance(space as usize);
    let mut data = 0u32.to_le_bytes().to_vec();
    data.extend_from_slice(&rent.to_le_bytes());
    data.extend_from_slice(&space.to_le_bytes());
    data.extend_from_slice(ID.as_ref());
    let create = Instruction {
        program_id: system_program::id(),
        accounts: vec![
            AccountMeta::new(creator, true),
            AccountMeta::new(stranger, true),
        ],
        data,
    };
    let created = context.process_instruction(&create);
    assert!(created.program_result.is_ok(), "{created:#?}");
    {
        let store = context.account_store.borrow();
        let made = &store[&stranger];
        assert_eq!(
            made.owner(),
            &ID,
            "a Ballista-owned account Ballista did not create"
        );
        assert!(made.data().iter().all(|byte| *byte == 0));
    }

    let code = |instruction: &Instruction| custom_code(&context.process_instruction(instruction));
    // As a template: every lifecycle instruction and a run refuse it.
    assert_eq!(
        code(&run_instruction(stranger, vec![], &[])),
        Some(INVALID_TEMPLATE_ACCOUNT)
    );
    assert_eq!(
        code(&write_template_chunk_instruction(
            creator,
            stranger,
            0,
            &[1]
        )),
        Some(INVALID_TEMPLATE_ACCOUNT)
    );
    assert_eq!(
        code(&finalize_template_instruction(creator, stranger)),
        Some(INVALID_TEMPLATE_ACCOUNT)
    );
    assert_eq!(
        code(&cancel_template_instruction(creator, stranger)),
        Some(INVALID_TEMPLATE_ACCOUNT)
    );

    // As a registry entry of the right size: the open, at pc 0, refuses its header.
    let mut builder = ProgramBuilder::new();
    let system = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
    let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    builder.open_registry(entry, None, payer, 0, 16, system);
    let payload = builder.build().expect("builds");
    let uploaded = context.process_instruction(&create_template_instruction(creator, 4, &payload));
    assert!(uploaded.program_result.is_ok(), "{uploaded:#?}");
    let opener = find_template_pda(&creator, 4).0;
    let open = run_instruction(
        opener,
        vec![
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(creator, true),
            AccountMeta::new(stranger, false),
        ],
        &[],
    );
    assert_eq!(code(&open), Some(INVALID_REGISTRY_ENTRY));
    assert_eq!(
        lamports(&context, stranger),
        rent,
        "nothing moved its lamports"
    );
}

/// P67, finding F3. A failure the run raises carries a Ballista code, with one documented exception
/// (a missing signer). An open registry entry's data is marked borrowed for the rest of the run, so
/// a fixed-offset read of that entry through another slot, which the caller fills with the same
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
    accounts.insert(
        unrelated,
        Account::new(1_000_000, 80, &Pubkey::new_unique()),
    );
    let context = context(accounts);
    let created = context.process_instruction(&create_template_instruction(creator, 2, &payload));
    assert!(created.program_result.is_ok(), "{created:#?}");
    let template = find_template_pda(&creator, 2).0;

    // The template-wide entry of registry 0, seeded so it holds its 80 bytes before the run.
    let entry_key =
        Pubkey::find_program_address(&[b"registry", template.as_ref(), &[0], &[0; 32]], &ID).0;
    let mut seeded = Account::new(context.mollusk.sysvars.rent.minimum_balance(80), 80, &ID);
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
