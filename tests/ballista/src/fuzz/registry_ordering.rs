//! A design finding the second critic asked to test, now closed. An open entry's borrow mark, the
//! run-time guard on it, exists only once the open runs, and `refuse_entry_data`
//! (common/src/template/verify.rs) refuses a raw read only of the entry's own slot. So a Rust-built
//! template could read the entry's raw bytes through a second slot that holds it, then make a CPI
//! that passes the entry writable through that slot, all before the open: a nested run in that
//! window could change the entry under the read (`critic_lost_write` builds one).
//!
//! The verifier now refuses an `OPEN_REGISTRY` with any `INVOKE` at a lower pc, in a loop or not
//! (`verify_open_registry`), so every CPI meets the mark. The TypeScript compiler always emitted
//! the opens first; these tests pin the rule for hand-built templates:
//!
//! - the template that showed the gap is refused at upload, with `InvalidRegistry` at its open;
//! - with the open moved first, its CPI fails the run with `RegistryReentry`;
//! - a raw read through the second slot with no call before the open still verifies and runs, and
//!   reads exactly the bytes the open checks, since nothing can run between the two.

use ballista_common::instruction::IX_RUN;
use ballista_common::template::{
    encode_error, ProgramBuilder, ProgramView, Segment, TemplateError, ACCOUNT_EXECUTABLE,
    ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_U64, OP_READ_U64, SYSTEM_PROGRAM_ADDRESS,
};
use ballista_fuzz_gen::scenario::{entry_address, entry_header};
use mollusk_svm::result::types::{InstructionResult, ProgramResult};
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_program_error::ProgramError;
use solana_pubkey::Pubkey;
use solana_sdk_ids::system_program;

use super::harness::{Harness, BALLISTA_ID};

const CREATOR: Pubkey = Pubkey::new_from_array([0x4c; 32]);
const PAYER: Pubkey = Pubkey::new_from_array([0x4d; 32]);
const REGISTRY_INDEX: u8 = 0;
const REGISTRY_SIZE: u16 = 16;
const PLANTED_FIELD0: u64 = 0xABCD_1234_5678_9A01;
const REGISTRY_REENTRY: u32 = 6026;

/// Where the template's one CPI sits, if it makes one.
#[derive(Clone, Copy, Debug)]
enum Call {
    /// Between the raw read and the open: the shape that showed the gap.
    BeforeTheOpen,
    /// The same call, guarded by a `true` register.
    GuardedBeforeTheOpen,
    /// The same call, in a one-pass `REPEAT` body.
    InALoopBeforeTheOpen,
    /// After the open, where the borrow mark meets it.
    AfterTheOpen,
    /// No call at all.
    None,
}

/// Reads the entry's raw field-0 bytes through a second writable slot that holds it at run time,
/// makes (per `call`) a zero-lamport System transfer that passes that slot writable, opens the
/// entry, and returns what the raw read saw. Slots: 0 system, 1 payer, 2 entry (opened), 3 the
/// alias. Returns the payload, the open's pc, and the invoke's pc if there is one.
fn aliased_read_template(call: Call) -> (Vec<u8>, usize, Option<usize>) {
    let mut builder = ProgramBuilder::new();
    let system = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
    let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    // A second writable slot whose minimum length covers field 0, pinning neither owner nor
    // address. No open names it, so neither registry rule that keys on the entry's own slot
    // applies to it.
    let alias = builder.account(ACCOUNT_WRITABLE, None, None, (72 + 8) as u32);

    let raw = builder.read(OP_READ_U64, alias, 72);
    let mut invoke = None;
    let mut call_here = |builder: &mut ProgramBuilder| {
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
        match call {
            Call::GuardedBeforeTheOpen => {
                let yes = builder.const_bool(true);
                invoke = Some(builder.instructions_mut().len());
                builder.invoke(transfer, Some(yes));
            }
            Call::InALoopBeforeTheOpen => {
                let once = builder.const_u64(1);
                builder.repeat(once, 1, 0, |body| {
                    invoke = Some(body.instructions_mut().len());
                    body.invoke(transfer, None);
                });
            }
            _ => {
                invoke = Some(builder.instructions_mut().len());
                builder.invoke(transfer, None);
            }
        }
    };
    let before = matches!(
        call,
        Call::BeforeTheOpen | Call::GuardedBeforeTheOpen | Call::InALoopBeforeTheOpen
    );
    if before {
        call_here(&mut builder);
    }
    let open = builder.open_registry(entry, None, payer, REGISTRY_INDEX, REGISTRY_SIZE, system);
    if matches!(call, Call::AfterTheOpen) {
        call_here(&mut builder);
    }
    builder.set_return_data(&[Segment::Register(DATA_REG_U64, raw)]);
    (builder.build().unwrap(), open, invoke)
}

/// What a failed upload or run reports for `error`, as `Harness::upload` formats it.
fn refused_with(kind: u32, context: u16) -> String {
    format!("{:?}", ProgramResult::Failure(ProgramError::Custom(encode_error(kind, context))))
}

/// The custom `(kind, context)` a run failed with.
fn custom(result: &InstructionResult) -> Option<(u32, u32)> {
    match &result.program_result {
        ProgramResult::Failure(ProgramError::Custom(code)) => Some((code & 0xffff, code >> 16)),
        _ => None,
    }
}

/// Uploads `payload` and seeds an existing entry holding `PLANTED_FIELD0` in field 0, then runs it
/// with the entry in both the entry slot and the alias slot, writable.
fn run_with_alias(harness: &Harness, id: u16, payload: &[u8]) -> InstructionResult {
    harness.reset();
    let (template, _) = harness.upload(&CREATOR, id, payload).expect("the template verifies and uploads");
    let entry = Pubkey::new_from_array(entry_address(
        &|seeds, program| {
            let (address, bump) = Pubkey::find_program_address(seeds, &Pubkey::new_from_array(*program));
            (address.to_bytes(), bump)
        },
        &BALLISTA_ID.to_bytes(),
        &template.to_bytes(),
        REGISTRY_INDEX,
        &[0u8; 32],
    ));
    let mut data = entry_header(&template.to_bytes(), REGISTRY_INDEX, &[0u8; 32]).to_vec();
    data.extend_from_slice(&PLANTED_FIELD0.to_le_bytes());
    data.extend_from_slice(&[0u8; (REGISTRY_SIZE as usize) - 8]);
    let rent = harness.rent_minimum(data.len());
    {
        let mut store = harness.context.account_store.borrow_mut();
        store.insert(PAYER, Account::new(10_000_000_000, 0, &system_program::id()));
        store.insert(entry, Account { lamports: rent, data, owner: BALLISTA_ID, executable: false, rent_epoch: 0 });
    }
    let instruction = Instruction {
        program_id: BALLISTA_ID,
        accounts: vec![
            AccountMeta::new_readonly(template, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(PAYER, true),
            AccountMeta::new(entry, false),
            AccountMeta::new(entry, false),
        ],
        data: vec![IX_RUN],
    };
    harness.context.process_instruction(&instruction)
}

/// The gap's own template, which verified, uploaded and ran before the rule: the upload now fails
/// with `InvalidRegistry` at the open, before any run.
#[test]
fn aliased_entry_read_and_window_cpi_are_refused_at_upload() {
    let harness = Harness::new();
    harness.reset();
    let (payload, open, _) = aliased_read_template(Call::BeforeTheOpen);
    let (kind, context) = TemplateError::InvalidRegistry(open).code();
    assert_eq!(
        harness.upload(&CREATOR, 1, &payload).map(|_| ()),
        Err(refused_with(kind, context)),
        "a call before the open is refused at create, with InvalidRegistry at the open"
    );
}

/// The regression the critic asked for: nothing may reach an entry in the window before its open.
/// Every variant of the call before the open is refused at its open; the raw read alone is not,
/// since with no call before the open it reads what the open checks.
#[test]
fn aliased_entry_read_should_be_refused() {
    for call in [Call::BeforeTheOpen, Call::GuardedBeforeTheOpen, Call::InALoopBeforeTheOpen] {
        let (payload, open, invoke) = aliased_read_template(call);
        assert!(invoke.is_some_and(|invoke| invoke < open), "{call:?}");
        let verdict = ProgramView::parse(&payload).and_then(|program| program.verify());
        assert_eq!(verdict.map(|_| ()), Err(TemplateError::InvalidRegistry(open)), "{call:?}");
    }
    for call in [Call::AfterTheOpen, Call::None] {
        let (payload, _, _) = aliased_read_template(call);
        let verdict = ProgramView::parse(&payload).and_then(|program| program.verify());
        assert!(verdict.is_ok(), "{call:?}: {verdict:?}");
    }
}

/// With the open first, the same CPI through the alias meets the borrow mark: the run fails with
/// `RegistryReentry` at the invoke, so no nested run can write the entry this run holds open.
#[test]
fn a_call_through_the_alias_after_the_open_fails_with_registry_reentry() {
    let harness = Harness::new();
    let (payload, _, invoke) = aliased_read_template(Call::AfterTheOpen);
    let result = run_with_alias(&harness, 2, &payload);
    assert_eq!(custom(&result), Some((REGISTRY_REENTRY, invoke.unwrap() as u32)), "{result:?}");
}

/// What the rule leaves: a raw read through the alias with no call before the open. It runs, and
/// returns the entry's field 0, the bytes the open then checks and marks.
#[test]
fn an_aliased_read_with_no_call_before_the_open_reads_what_the_open_checks() {
    let harness = Harness::new();
    let (payload, _, _) = aliased_read_template(Call::None);
    let result = run_with_alias(&harness, 3, &payload);
    assert!(result.program_result.is_ok(), "{result:?}");
    assert_eq!(result.return_data, PLANTED_FIELD0.to_le_bytes());
}
