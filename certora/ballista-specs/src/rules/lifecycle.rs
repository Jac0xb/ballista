//! Template accounts move in one direction: uploading, then finalized, then immutable forever.
//!
//! These rules call the program's entrypoint with a Ballista-owned template account holding the
//! canonical SOL transfer template, so every handler is in scope. The two CPIs the program can make
//! (creating the template account, and the template's own invocation) are opaque to the prover:
//! see the inlining and summary files.

use ballista::process_instruction;
use ballista_common::instruction::{IX_CANCEL_TEMPLATE, IX_RUN, IX_WRITE_TEMPLATE_CHUNK};
use ballista_common::template::*;
use cvlr::prelude::*;
use cvlr_pinocchio::{heap_bytes, heap_views, nondet_address, AccountSlot};
use pinocchio::Address;

use super::util::{heap_transfer_payload, TRANSFER_PAYLOAD};

/// Payload of the canonical SOL transfer template.
const PAYLOAD: usize = TRANSFER_PAYLOAD.len();
/// Account data: the template header plus the payload.
const TEMPLATE_DATA: usize = TEMPLATE_ACCOUNT_HEADER_LEN + PAYLOAD;

/// A Ballista-owned template account holding the transfer payload in the given state, with a
/// nondeterministic creator, ID, bump, hash, and address.
fn template_slot(state: u8) -> (&'static mut AccountSlot<TEMPLATE_DATA>, [u8; 32]) {
    let creator = nondet_address().to_bytes();
    let mut header = TemplateAccountHeader::new_uploading(
        creator,
        nondet(),
        nondet(),
        PAYLOAD,
        nondet_address().to_bytes(),
    )
    .expect("payload length is valid");
    if state == TEMPLATE_STATE_FINALIZED {
        header.set_written_len(PAYLOAD).expect("fits");
        header.finalize().expect("fully written");
    } else {
        let written: usize = nondet();
        cvlr_assume!(written < PAYLOAD);
        header.set_written_len(written).expect("fits");
    }

    let slot = AccountSlot::<TEMPLATE_DATA>::nondet();
    slot.header.owner = ballista::ID;
    slot.header.data_len = TEMPLATE_DATA as u64;
    slot.data[..TEMPLATE_ACCOUNT_HEADER_LEN].copy_from_slice(header.as_bytes());
    slot.data[TEMPLATE_ACCOUNT_HEADER_LEN..].copy_from_slice(heap_transfer_payload());
    (slot, creator)
}

fn creator_slot(address: [u8; 32]) -> &'static mut AccountSlot<0> {
    let slot = AccountSlot::<0>::nondet();
    slot.header.address = Address::new_from_array(address);
    slot.header.is_signer = 1;
    slot.header.is_writable = 1;
    slot
}

/// Instruction data on the heap, where the program may index it with template-derived offsets.
fn instruction_data<const N: usize>(bytes: [u8; N]) -> &'static [u8] {
    heap_bytes(&bytes)
}

/// A `Run` instruction carrying eight nondeterministic input bytes.
fn run_data() -> &'static [u8] {
    let mut data = [0u8; 9];
    data[0] = IX_RUN;
    data[1..].copy_from_slice(&nondet::<u64>().to_le_bytes());
    instruction_data(data)
}

#[rule]
pub fn rule_finalized_templates_reject_chunk_writes() {
    let (template, creator) = template_slot(TEMPLATE_STATE_FINALIZED);
    template.header.is_writable = 1;
    let before: [u8; TEMPLATE_DATA] = template.data;
    let creator_account = creator_slot(creator);
    let accounts = heap_views([creator_account.view(), template.view()]);
    let offset: u32 = nondet();
    let mut data = [0u8; 6];
    data[0] = IX_WRITE_TEMPLATE_CHUNK;
    data[1..5].copy_from_slice(&offset.to_le_bytes());
    data[5] = nondet();
    let result = process_instruction(&ballista::ID, &mut accounts[..], instruction_data(data));
    cvlr_assert!(result.is_err());
    cvlr_assert!(template.data == before);
}

#[rule]
pub fn rule_finalized_templates_cannot_be_cancelled() {
    let (template, creator) = template_slot(TEMPLATE_STATE_FINALIZED);
    template.header.is_writable = 1;
    let lamports_before = template.header.lamports;
    let creator_account = creator_slot(creator);
    let accounts = heap_views([creator_account.view(), template.view()]);
    let result = process_instruction(
        &ballista::ID,
        &mut accounts[..],
        instruction_data([IX_CANCEL_TEMPLATE]),
    );
    cvlr_assert!(result.is_err());
    cvlr_assert!(template.header.lamports == lamports_before);
    cvlr_assert!(template.header.owner == ballista::ID);
}

#[rule]
pub fn rule_uploading_templates_never_run() {
    let (template, _) = template_slot(TEMPLATE_STATE_UPLOADING);
    let system = AccountSlot::<0>::nondet();
    let sender = AccountSlot::<0>::nondet();
    let recipient = AccountSlot::<0>::nondet();
    let accounts = heap_views([template.view(), system.view(), sender.view(), recipient.view()]);
    let result = process_instruction(&ballista::ID, &mut accounts[..], run_data());
    cvlr_assert!(result.is_err());
}

#[rule]
pub fn rule_run_never_writes_the_template_account() {
    let (template, _) = template_slot(TEMPLATE_STATE_FINALIZED);
    let before: [u8; TEMPLATE_DATA] = template.data;
    let lamports_before = template.header.lamports;
    let system = AccountSlot::<0>::nondet();
    let sender = AccountSlot::<0>::nondet();
    let recipient = AccountSlot::<0>::nondet();
    let accounts = heap_views([template.view(), system.view(), sender.view(), recipient.view()]);
    let _ = process_instruction(&ballista::ID, &mut accounts[..], run_data());
    cvlr_assert!(template.data == before);
    cvlr_assert!(template.header.lamports == lamports_before);
}
