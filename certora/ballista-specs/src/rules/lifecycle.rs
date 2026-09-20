//! Template accounts move in one direction: uploading, then finalized, then immutable forever.

use ballista::process_instruction;
use ballista_common::instruction::{IX_CANCEL_TEMPLATE, IX_RUN, IX_WRITE_TEMPLATE_CHUNK};
use ballista_common::template::*;
use cvlr::prelude::*;
use cvlr_pinocchio::{nondet_address, AccountSlot};
use pinocchio::Address;

/// Payload of the canonical SOL transfer template, 152 bytes.
const PAYLOAD: usize = 152;
/// Account data: 80-byte template header plus the payload.
const TEMPLATE_DATA: usize = TEMPLATE_ACCOUNT_HEADER_LEN + PAYLOAD;

fn transfer_payload() -> Vec<u8> {
    let mut builder = ProgramBuilder::new();
    let system = builder.account(ACCOUNT_EXECUTABLE, Some([0; 32]), None, 0);
    let sender = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let recipient = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let lamports = builder.input(VALUE_U64, 0);
    let amount = builder.load_input(lamports);
    let discriminator = builder.blob(&[2, 0, 0, 0]);
    let cpi = builder.cpi(
        system,
        &[
            (sender, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
            (recipient, ACCOUNT_WRITABLE),
        ],
        &[
            Segment::Literal(discriminator),
            Segment::Register(DATA_REG_U64, amount),
        ],
    );
    builder.invoke(cpi, None);
    let payload = builder.build().expect("transfer template builds");
    cvlr_assert!(payload.len() == PAYLOAD);
    payload
}

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
    slot.data[TEMPLATE_ACCOUNT_HEADER_LEN..].copy_from_slice(&transfer_payload());
    (slot, creator)
}

fn creator_slot(address: [u8; 32]) -> &'static mut AccountSlot<0> {
    let slot = AccountSlot::<0>::nondet();
    slot.header.address = Address::new_from_array(address);
    slot.header.is_signer = 1;
    slot.header.is_writable = 1;
    slot
}

#[rule]
pub fn rule_finalized_templates_reject_chunk_writes() {
    let (template, creator) = template_slot(TEMPLATE_STATE_FINALIZED);
    template.header.is_writable = 1;
    let before: [u8; TEMPLATE_DATA] = template.data;
    let creator_account = creator_slot(creator);
    let mut accounts = [creator_account.view(), template.view()];
    let offset: u32 = nondet();
    let mut data = vec![IX_WRITE_TEMPLATE_CHUNK];
    data.extend_from_slice(&offset.to_le_bytes());
    data.push(nondet());
    let result = process_instruction(&ballista::ID, &mut accounts, &data);
    cvlr_assert!(result.is_err());
    cvlr_assert!(template.data == before);
}

#[rule]
pub fn rule_finalized_templates_cannot_be_cancelled() {
    let (template, creator) = template_slot(TEMPLATE_STATE_FINALIZED);
    template.header.is_writable = 1;
    let lamports_before = template.header.lamports;
    let creator_account = creator_slot(creator);
    let mut accounts = [creator_account.view(), template.view()];
    let result = process_instruction(&ballista::ID, &mut accounts, &[IX_CANCEL_TEMPLATE]);
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
    let mut accounts = [template.view(), system.view(), sender.view(), recipient.view()];
    let mut data = vec![IX_RUN];
    data.extend_from_slice(&nondet::<u64>().to_le_bytes());
    let result = process_instruction(&ballista::ID, &mut accounts, &data);
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
    let mut accounts = [template.view(), system.view(), sender.view(), recipient.view()];
    let mut data = vec![IX_RUN];
    data.extend_from_slice(&nondet::<u64>().to_le_bytes());
    let _ = process_instruction(&ballista::ID, &mut accounts, &data);
    cvlr_assert!(template.data == before);
    cvlr_assert!(template.header.lamports == lamports_before);
}
