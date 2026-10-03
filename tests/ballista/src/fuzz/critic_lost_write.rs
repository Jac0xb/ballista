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

use ballista_common::instruction::IX_RUN;
use ballista_common::template::{
    ProgramBuilder, Segment, ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, OP_ADD,
    OP_READ_U64, REGISTRY_ENTRY_HEADER_LEN, SYSTEM_PROGRAM_ADDRESS, VALUE_BOOL,
};
use ballista_fuzz_gen::scenario::{entry_address, entry_header};
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;
use solana_sdk_ids::system_program;

use super::harness::{Harness, BALLISTA_ID};

const CREATOR: Pubkey = Pubkey::new_from_array([0x5a; 32]);
const PAYER: Pubkey = Pubkey::new_from_array([0x5b; 32]);
const SIZE: u16 = 16;
const START: u64 = 5;

/// Slots: 0 system, 1 payer, 2 entry (opened), 3 alias of the entry, 4 Ballista, 5 the template.
fn counter_template(stale: bool) -> Vec<u8> {
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
    let raw = b.read(OP_READ_U64, alias, REGISTRY_ENTRY_HEADER_LEN as u64);
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
    b.invoke(nested, Some(nest));
    b.open_registry(entry, None, payer, 0, SIZE, system);
    let base = if stale {
        raw
    } else {
        b.read_registry(entry, 0, OP_READ_U64)
    };
    let one = b.const_u64(1);
    let next = b.binary(OP_ADD, base, one);
    b.write_registry(entry, 0, OP_READ_U64, next);
    b.build().unwrap()
}

/// Runs the outer (`nest` = true) run and returns (succeeded, field 0 after, Ballista self-CPIs).
fn run(stale: bool, id: u16) -> (bool, u64, usize) {
    let harness = Harness::new();
    harness.reset();
    let (template, _) = harness
        .upload(&CREATOR, id, &counter_template(stale))
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
        data: vec![IX_RUN, 1],
    };
    let result = harness
        .context
        .process_transaction_instructions(&[instruction]);
    let ok = result.program_result.is_ok();
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
    let message = result.message.as_ref().expect("message");
    let keys = message.account_keys();
    let self_cpis = result
        .inner_instructions
        .first()
        .map(|inner| {
            inner
                .iter()
                .filter(|i| keys.get(i.instruction.program_id_index as usize) == Some(&BALLISTA_ID))
                .count()
        })
        .unwrap_or(0);
    if !ok {
        eprintln!("run failed: {:?}", result.program_result);
    }
    (ok, field, self_cpis)
}

/// DEMONSTRATES A BUG (passes today): two successful increments, one recorded. The nested run
/// writes START + 1; the outer run, acting on its pre-open raw read, writes START + 1 over it.
#[test]
#[ignore = "critic demo: passes while the lost write is reachable, so it documents a bug, not a guarantee"]
fn critic_a_raw_read_before_the_open_loses_a_nested_runs_write() {
    let (ok, field, self_cpis) = run(true, 1);
    assert!(ok, "the run succeeds");
    assert_eq!(
        self_cpis, 1,
        "one nested run of the same template ran in the window"
    );
    assert_eq!(
        field,
        START + 1,
        "the nested run's increment was lost (START + 2 expected without the window)"
    );
}

/// Control: the same template reading the field after the open records both increments.
#[test]
#[ignore = "critic control for the demo; its template calls out before the open, which the proposed verifier rule refuses"]
fn critic_a_field_read_after_the_open_keeps_both_writes() {
    let (ok, field, self_cpis) = run(false, 2);
    assert!(ok, "the run succeeds");
    assert_eq!(self_cpis, 1);
    assert_eq!(field, START + 2);
}

/// The property the docs state (language.md, Registries: "no other run can change an entry
/// between this run's read and its write"): the verifier should refuse the stale shape, for
/// example by requiring every OPEN_REGISTRY to precede the first INVOKE and the first data read.
#[test]
#[ignore = "fails today: documents the reachable lost write; unignore once the verifier orders opens first"]
fn critic_the_verifier_refuses_a_call_or_data_read_before_an_open() {
    let harness = Harness::new();
    harness.reset();
    assert!(harness
        .upload(&CREATOR, 3, &counter_template(true))
        .is_err());
}
