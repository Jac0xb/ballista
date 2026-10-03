//! Template accounts move one way: uploading, then finalized, then immutable (`lib.rs`'s
//! handlers, called through `process_instruction`). These are the Certora lifecycle rules, which
//! are blocked on that prover's heap model, restated for Kani.
//!
//! The template's PDA search and SHA-256 are stubbed to return any value, so every outcome of the
//! address and hash checks is explored; the CPIs a handler makes are no-ops off-chain. Instruction
//! data is built as array literals, so the solver sees the discriminator as a constant and follows
//! only that handler.

use ballista::process_instruction;
use ballista_common::instruction::{IX_CANCEL_TEMPLATE, IX_FINALIZE_TEMPLATE, IX_WRITE_TEMPLATE_CHUNK};
use ballista_common::template::*;
use pinocchio::{AccountView, Address};

use crate::accounts::{AccountMemory, Fields};

/// Payload bytes of the template accounts here.
const PAYLOAD: usize = 12;
const DATA: usize = TEMPLATE_ACCOUNT_HEADER_LEN + PAYLOAD;

/// Stub for `pda::get_template_address`: any address and bump.
#[allow(dead_code)] // referenced only from `#[kani::stub]`
pub fn get_template_address(_creator: &Address, _id: u16) -> (Address, u8) {
    (Address::new_from_array(kani::any()), kani::any())
}

/// Stub for `solana_sha256_hasher::hash`: any digest.
#[allow(dead_code)] // referenced only from `#[kani::stub]`
pub fn hash(_val: &[u8]) -> solana_hash::Hash {
    solana_hash::Hash::new_from_array(kani::any())
}

/// A Ballista-owned template account holding a `PAYLOAD`-byte payload, any bytes, in `state`, with
/// any creator, id, bump, hash, written length (all of it when finalized), address, lamports and
/// flags; and its creator's account, any flags, at the creator's address or any other.
fn template(state: u8) -> (AccountMemory<DATA>, AccountMemory<0>) {
    let creator: [u8; 32] = kani::any();
    let mut header = TemplateAccountHeader::new_uploading(creator, kani::any(), kani::any(), PAYLOAD, kani::any())
        .expect("a valid payload length");
    if state == TEMPLATE_STATE_FINALIZED {
        header.set_written_len(PAYLOAD).unwrap();
        header.finalize().unwrap();
    } else {
        header.set_written_len(kani::any_where(|n: &usize| *n <= PAYLOAD)).unwrap();
    }
    let mut data: [u8; DATA] = kani::any();
    data[..TEMPLATE_ACCOUNT_HEADER_LEN].copy_from_slice(header.as_bytes());
    let mut fields = Fields::plain(kani::any(), ballista::ID.to_bytes());
    fields.lamports = kani::any();
    fields.writable = kani::any();
    fields.signer = kani::any();
    let template = AccountMemory::new(fields, data, DATA);

    let mut creator_fields = Fields::plain(if kani::any() { creator } else { kani::any() }, kani::any());
    creator_fields.signer = kani::any();
    creator_fields.writable = kani::any();
    creator_fields.lamports = kani::any();
    (template, AccountMemory::new(creator_fields, [], 0))
}

/// A fresh copy of an account, so each attempt starts from the same state.
fn copy<const N: usize>(memory: &AccountMemory<N>) -> AccountMemory<N> {
    let mut copy = AccountMemory::new(Fields::plain([0; 32], [0; 32]), memory.data, 0);
    copy.header = memory.header.clone();
    copy
}

/// Runs `instruction` against fresh copies of the creator and the template, and checks the
/// template's data, lamports, owner and length are unchanged. Returns whether it failed.
fn refused(template: &AccountMemory<DATA>, creator: &AccountMemory<0>, instruction: &[u8]) -> bool {
    let (mut template_copy, mut creator_copy) = (copy(template), copy(creator));
    let mut accounts: [AccountView; 2] = [creator_copy.view(), template_copy.view()];
    let result = process_instruction(&ballista::ID, &mut accounts, instruction);
    assert!(template_copy.data == template.data);
    assert_eq!(template_copy.header.lamports, template.header.lamports);
    assert!(template_copy.header.owner == template.header.owner);
    assert_eq!(template_copy.header.data_len, template.header.data_len);
    result.is_err()
}

/// A finalized template's account never changes: a chunk write (any offset, 1 to 4 bytes), a
/// finalize and a cancel all fail, whoever signs, and the template's data, lamports, owner and
/// length stay exactly as they were. Bound: a 12-byte payload; every header field, flag, address
/// and lamport balance symbolic.
#[kani::proof]
#[kani::unwind(93)]
#[kani::stub(ballista::utils::pda::get_template_address, crate::lifecycle::get_template_address)]
#[kani::stub(solana_sha256_hasher::hash, crate::lifecycle::hash)]
fn finalized_templates_never_change() {
    let (template, creator) = template(TEMPLATE_STATE_FINALIZED);
    let offset: [u8; 4] = kani::any();
    let bytes: [u8; 4] = kani::any();
    let write = [
        IX_WRITE_TEMPLATE_CHUNK, offset[0], offset[1], offset[2], offset[3], bytes[0], bytes[1], bytes[2], bytes[3],
    ];
    let chunk = kani::any_where(|n: &usize| (1..=4).contains(n));
    assert!(refused(&template, &creator, &write[..5 + chunk]));
    assert!(refused(&template, &creator, &[IX_FINALIZE_TEMPLATE]));
    assert!(refused(&template, &creator, &[IX_CANCEL_TEMPLATE]));
    let named = creator.header.address.to_bytes() == template.data[4..36];
    kani::cover!(named && creator.header.is_signer != 0 && creator.header.is_writable != 0, "the creator itself tries");
}

/// The run path refuses every template that is not finalized before any instruction runs: on any
/// account data whose state byte is not "finalized", `finalized_program_unchecked` returns nothing
/// and `TemplateAccount::parse(..).finalized_program()` fails, and those are the only two ways
/// `run_template` reaches `processor::run`. Bound: account data 0 to 10,320 bytes (the header plus
/// the payload cap), every byte symbolic but the state.
#[kani::proof]
#[kani::unwind(5)]
fn uploading_templates_never_run() {
    let bytes: [u8; TEMPLATE_ACCOUNT_HEADER_LEN + MAX_TEMPLATE_PAYLOAD_LEN] = kani::any();
    let len: usize = kani::any_where(|n: &usize| *n <= TEMPLATE_ACCOUNT_HEADER_LEN + MAX_TEMPLATE_PAYLOAD_LEN);
    kani::assume(len <= 2 || bytes[2] != TEMPLATE_STATE_FINALIZED);
    let data = &bytes[..len];
    assert!(TemplateAccount::finalized_program_unchecked(data).is_none());
    let parsed = TemplateAccount::parse(data);
    let is_uploading = parsed.as_ref().is_ok_and(|account| !account.header().is_finalized());
    assert!(parsed.and_then(|account| account.finalized_program()).is_err());
    kani::cover!(is_uploading && len == TEMPLATE_ACCOUNT_HEADER_LEN + 100, "an uploading template parses but does not run");
}

/// A chunk write to an uploading template succeeds only when the creator signs and is writable,
/// the template is writable, the creator is the one the header names, and the chunk starts at the
/// written length and fits the payload (the address check is stubbed to any answer); it then
/// writes exactly those payload bytes and advances the written length by the chunk's size, and
/// changes nothing else. Any failure changes nothing at all. Bound: a 12-byte payload, chunks of 1
/// to 4 bytes at any offset.
#[kani::proof]
#[kani::unwind(93)]
#[kani::stub(ballista::utils::pda::get_template_address, crate::lifecycle::get_template_address)]
#[kani::stub(solana_sha256_hasher::hash, crate::lifecycle::hash)]
fn chunk_writes_append_exactly_their_bytes() {
    let (mut template, mut creator) = template(TEMPLATE_STATE_UPLOADING);
    let before = template.data;
    let written = u32::from_le_bytes([before[44], before[45], before[46], before[47]]) as usize;
    let offset: [u8; 4] = kani::any();
    let bytes: [u8; 4] = kani::any();
    let len = kani::any_where(|n: &usize| (1..=4).contains(n));
    let write = [
        IX_WRITE_TEMPLATE_CHUNK, offset[0], offset[1], offset[2], offset[3], bytes[0], bytes[1], bytes[2], bytes[3],
    ];
    let offset = u32::from_le_bytes(offset) as usize;
    let creator_ok = creator.header.is_signer != 0
        && creator.header.is_writable != 0
        && template.header.is_writable != 0
        && creator.header.address.to_bytes() == before[4..36];
    let mut accounts: [AccountView; 2] = [creator.view(), template.view()];
    let result = process_instruction(&ballista::ID, &mut accounts, &write[..5 + len]);

    if result.is_ok() {
        assert!(creator_ok);
        assert_eq!(offset, written);
        assert!(written + len <= PAYLOAD);
        let end = ((written + len) as u32).to_le_bytes();
        for i in 0..DATA {
            let expected = if (44..48).contains(&i) {
                end[i - 44]
            } else if i >= TEMPLATE_ACCOUNT_HEADER_LEN + written && i < TEMPLATE_ACCOUNT_HEADER_LEN + written + len {
                bytes[i - TEMPLATE_ACCOUNT_HEADER_LEN - written]
            } else {
                before[i]
            };
            assert_eq!(template.data[i], expected);
        }
        kani::cover!(written + len == PAYLOAD, "the chunk that completes the payload");
    } else {
        assert!(template.data == before);
        kani::cover!(creator_ok && offset != written, "a chunk at the wrong offset");
    }
}

/// A cancel succeeds only for an uploading template, by its signing, writable creator, and then
/// moves all of the template's lamports to the creator (total unchanged) and closes the account;
/// any failure changes neither account's lamports. Bound: as `finalized_templates_never_change`.
#[kani::proof]
#[kani::unwind(93)]
#[kani::stub(ballista::utils::pda::get_template_address, crate::lifecycle::get_template_address)]
#[kani::stub(solana_sha256_hasher::hash, crate::lifecycle::hash)]
fn cancels_return_every_lamport_to_the_creator() {
    let state = if kani::any() { TEMPLATE_STATE_UPLOADING } else { TEMPLATE_STATE_FINALIZED };
    let (mut template, mut creator) = template(state);
    let (template_lamports, creator_lamports) = (template.header.lamports, creator.header.lamports);
    let creator_named = creator.header.address.to_bytes() == template.data[4..36];
    let mut accounts: [AccountView; 2] = [creator.view(), template.view()];
    let result = process_instruction(&ballista::ID, &mut accounts, &[IX_CANCEL_TEMPLATE]);
    if result.is_ok() {
        assert_eq!(state, TEMPLATE_STATE_UPLOADING);
        assert!(creator.header.is_signer != 0 && creator.header.is_writable != 0 && creator_named);
        assert_eq!(creator.header.lamports as u128, creator_lamports as u128 + template_lamports as u128);
        assert_eq!(template.header.lamports, 0);
        assert_eq!(template.header.data_len, 0);
        kani::cover!(template_lamports > 0, "a cancel that returns lamports");
    } else {
        assert_eq!(template.header.lamports, template_lamports);
        assert_eq!(creator.header.lamports, creator_lamports);
        kani::cover!(state == TEMPLATE_STATE_FINALIZED && creator_named, "the creator cannot cancel a finalized template");
    }
}
