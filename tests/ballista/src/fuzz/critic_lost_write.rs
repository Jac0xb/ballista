//! Threat critic, second pass: is a lost registry write reachable through the read->open window
//! that `registry_ordering` documents? FV3 showed the window but not a lost write ("needs a nested
//! run in the window"). This module builds that nested run.
//!
//! The template, built with the Rust `ProgramBuilder` (a supported authoring path; it does not
//! hoist opens the way the TypeScript compiler does), increments field 0 of its template-wide
//! entry:
//!
//! 1. reads field 0's raw bytes through a second, unpinned writable slot that holds the entry;
//! 2. if the `nest` input is set, runs itself once more through a CPI into Ballista, passing the
//!    entry writable through that second slot (no borrow mark exists yet, so nothing refuses it);
//!    the nested run increments field 0 and returns;
//! 3. opens the entry and writes field 0 = (value from step 1, or a fresh field read) + 1.
//!
//! With the raw read (step 1) feeding the write, the outer run overwrites the nested run's
//! increment: two successful increments, one recorded. With a fresh `READ_REGISTRY` after the open,
//! both land. Only the template's own author can build this shape: no compiled template has it,
//! and a caller cannot add a raw read or move an open.
//!
//! Closed: the verifier now refuses an open with any invoke before it, so both templates are
//! refused at upload, with `InvalidRegistry` at the open. With the open first, the nested run
//! meets the entry's borrow mark and the run fails with `RegistryReentry`: no write is lost.

use ballista_common::instruction::IX_RUN;
use ballista_common::template::{
    encode_error, ProgramBuilder, Segment, TemplateError, ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER,
    ACCOUNT_WRITABLE, OP_ADD, OP_READ_U64, REGISTRY_ENTRY_HEADER_LEN, SYSTEM_PROGRAM_ADDRESS,
    VALUE_BOOL,
};
use mollusk_svm::result::types::{ProgramResult, TransactionProgramResult};
use ballista_fuzz_gen::scenario::{entry_address, entry_header};
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_program_error::ProgramError;
use solana_pubkey::Pubkey;
use solana_sdk_ids::system_program;

use super::harness::{Harness, BALLISTA_ID};

const CREATOR: Pubkey = Pubkey::new_from_array([0x5a; 32]);
const PAYER: Pubkey = Pubkey::new_from_array([0x5b; 32]);
const SIZE: u16 = 16;
const START: u64 = 5;

/// Where the template opens its entry, and what its write adds one to.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    /// Raw read, nested run, open, write the raw value plus one: the lost write.
    StaleRead,
    /// Raw read, nested run, open, write a fresh field read plus one: the critic's control.
    FreshRead,
    /// Open, nested run, write a fresh field read plus one: the order the verifier requires.
    OpenFirst,
}

/// Slots: 0 system, 1 payer, 2 entry (opened), 3 alias of the entry, 4 Ballista, 5 the template.
/// Returns the payload, the open's pc and the nested run's invoke pc.
fn counter_template(shape: Shape) -> (Vec<u8>, usize, usize) {
    let mut b = ProgramBuilder::new();
    let nest_input = b.input(VALUE_BOOL, 0);
    let system = b.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
    let payer = b.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let entry = b.account(ACCOUNT_WRITABLE, None, None, 0);
    let alias = b.account(
        ACCOUNT_WRITABLE,
        None,
        None,
        (REGISTRY_ENTRY_HEADER_LEN + 8) as u32,
    );
    let ballista = b.account(ACCOUNT_EXECUTABLE, Some(BALLISTA_ID.to_bytes()), None, 0);
    let this = b.account(0, None, None, 0);

    let nest = b.load_input(nest_input);
    let raw = (shape == Shape::StaleRead)
        .then(|| b.read(OP_READ_U64, alias, REGISTRY_ENTRY_HEADER_LEN as u64));
    // The nested run: the same template, `nest` false, with the entry in both entry slots.
    let run_data = b.blob(&[IX_RUN, 0]);
    let nested = b.cpi(
        ballista,
        &[
            (this, 0),
            (system, 0),
            (payer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (alias, ACCOUNT_WRITABLE),
            (alias, ACCOUNT_WRITABLE),
            (ballista, 0),
            (this, 0),
        ],
        &[Segment::Literal(run_data)],
    );
    let open_first = (shape == Shape::OpenFirst).then(|| b.open_registry(entry, None, payer, 0, SIZE, system));
    let invoke = b.instructions_mut().len();
    b.invoke(nested, Some(nest));
    let open = open_first.unwrap_or_else(|| b.open_registry(entry, None, payer, 0, SIZE, system));
    let base = raw.unwrap_or_else(|| b.read_registry(entry, 0, OP_READ_U64));
    let one = b.const_u64(1);
    let next = b.binary(OP_ADD, base, one);
    b.write_registry(entry, 0, OP_READ_U64, next);
    (b.build().unwrap(), open, invoke)
}

/// What `Harness::upload` reports for a create refused with `error`.
fn refused_with(error: TemplateError) -> String {
    let (kind, context) = error.code();
    format!("{:?}", ProgramResult::Failure(ProgramError::Custom(encode_error(kind, context))))
}

/// What a run of the outer template did: whether it succeeded, the custom code a failure carried
/// as `(kind, context)`, field 0 after it, and how many Ballista self-CPIs ran.
struct Outcome {
    ok: bool,
    code: Option<(u32, u32)>,
    field: u64,
    self_cpis: usize,
}

/// Uploads the `shape` template, seeds its entry at `START`, and runs it with `nest`.
fn run(shape: Shape, id: u16, nest: bool) -> Outcome {
    let harness = Harness::new();
    harness.reset();
    let (template, _) = harness
        .upload(&CREATOR, id, &counter_template(shape).0)
        .expect("the template verifies");
    let entry = Pubkey::new_from_array(entry_address(
        &|seeds, program| {
            let (address, bump) =
                Pubkey::find_program_address(seeds, &Pubkey::new_from_array(*program));
            (address.to_bytes(), bump)
        },
        &BALLISTA_ID.to_bytes(),
        &template.to_bytes(),
        0,
        &[0u8; 32],
    ));
    let mut data = entry_header(&template.to_bytes(), 0, &[0u8; 32]).to_vec();
    data.extend_from_slice(&START.to_le_bytes());
    data.extend_from_slice(&[0u8; SIZE as usize - 8]);
    {
        let mut store = harness.context.account_store.borrow_mut();
        store.insert(
            PAYER,
            Account::new(10_000_000_000, 0, &system_program::id()),
        );
        let lamports = harness.rent_minimum(data.len());
        store.insert(
            entry,
            Account {
                lamports,
                data,
                owner: BALLISTA_ID,
                executable: false,
                rent_epoch: 0,
            },
        );
    }
    let instruction = Instruction {
        program_id: BALLISTA_ID,
        accounts: vec![
            AccountMeta::new_readonly(template, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new(PAYER, true),
            AccountMeta::new(entry, false),
            AccountMeta::new(entry, false),
            AccountMeta::new_readonly(BALLISTA_ID, false),
            AccountMeta::new_readonly(template, false),
        ],
        data: vec![IX_RUN, u8::from(nest)],
    };
    let result = harness
        .context
        .process_transaction_instructions(&[instruction]);
    let ok = result.program_result.is_ok();
    let code = match &result.program_result {
        TransactionProgramResult::Failure(_, ProgramError::Custom(code)) => {
            Some((code & 0xffff, code >> 16))
        }
        _ => None,
    };
    let after = harness
        .context
        .account_store
        .borrow()
        .get(&entry)
        .cloned()
        .expect("entry");
    let field = u64::from_le_bytes(
        after.data[REGISTRY_ENTRY_HEADER_LEN..REGISTRY_ENTRY_HEADER_LEN + 8]
            .try_into()
            .unwrap(),
    );
    let self_cpis = result
        .message
        .as_ref()
        .map(|message| {
            let keys = message.account_keys();
            result
                .inner_instructions
                .first()
                .map(|inner| {
                    inner
                        .iter()
                        .filter(|i| {
                            keys.get(i.instruction.program_id_index as usize) == Some(&BALLISTA_ID)
                        })
                        .count()
                })
                .unwrap_or(0)
        })
        .unwrap_or(0);
    Outcome { ok, code, field, self_cpis }
}

/// The critic's demonstration, which passed before the rule: two successful increments, one
/// recorded, since the outer run wrote its pre-open raw read plus one over the nested run's
/// write. Its template is now refused at upload, at the open.
#[test]
fn critic_a_raw_read_before_the_open_cannot_lose_a_nested_runs_write() {
    let harness = Harness::new();
    harness.reset();
    let (payload, open, invoke) = counter_template(Shape::StaleRead);
    assert!(invoke < open);
    assert_eq!(
        harness.upload(&CREATOR, 1, &payload).map(|_| ()),
        Err(refused_with(TemplateError::InvalidRegistry(open)))
    );
}

/// The property the docs state (language.md, Registries: "no other run can change an entry
/// between this run's read and its write"): the verifier refuses a call before an open, whatever
/// the write is based on, so the critic's control is refused too.
#[test]
fn critic_the_verifier_refuses_a_call_before_an_open() {
    for (id, shape) in [(2, Shape::StaleRead), (3, Shape::FreshRead)] {
        let harness = Harness::new();
        harness.reset();
        let (payload, open, _) = counter_template(shape);
        assert_eq!(
            harness.upload(&CREATOR, id, &payload).map(|_| ()),
            Err(refused_with(TemplateError::InvalidRegistry(open))),
            "{shape:?}"
        );
    }
}

/// The order the verifier requires: with the open first, the nested run passes the entry
/// writable through the alias slot, meets the borrow mark, and the run fails with
/// `RegistryReentry` at the invoke, leaving field 0 as it was. Without the nested run, the one
/// increment lands.
#[test]
fn critic_with_the_open_first_the_nested_run_meets_the_borrow_mark() {
    const REGISTRY_REENTRY: u32 = 6026;
    let (_, _, invoke) = counter_template(Shape::OpenFirst);
    let nested = run(Shape::OpenFirst, 4, true);
    assert!(!nested.ok);
    assert_eq!(nested.code, Some((REGISTRY_REENTRY, invoke as u32)));
    assert_eq!((nested.field, nested.self_cpis), (START, 0));

    let alone = run(Shape::OpenFirst, 5, false);
    assert!(alone.ok);
    assert_eq!((alone.field, alone.self_cpis), (START + 1, 0));
}
