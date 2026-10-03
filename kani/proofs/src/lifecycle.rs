//! Template accounts move one way: uploading, then finalized, then immutable (`lib.rs`'s
//! handlers, called through `process_instruction`). These are the Certora lifecycle rules, which
//! are blocked on that prover's heap model, restated for Kani.
//!
//! The template's PDA search and SHA-256 are stubbed to return any value, so every outcome of the
//! address and hash checks is explored; the CPIs a handler makes are no-ops off-chain.

use ballista::process_instruction;
use ballista_common::instruction::{
    IX_CANCEL_TEMPLATE, IX_FINALIZE_TEMPLATE, IX_RUN, IX_WRITE_TEMPLATE_CHUNK,
};
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

/// A finalized template's account never changes: a chunk write (any offset, 1 to 4 bytes), a
/// finalize and a cancel all fail, and its data, lamports, owner and length stay exactly as they
/// were, whoever signs. Bounded: a 12-byte payload; every header field, flag and address symbolic.
#[kani::proof]
#[kani::unwind(93)]
#[kani::stub(ballista::utils::pda::get_template_address, crate::lifecycle::get_template_address)]
#[kani::stub(solana_sha256_hasher::hash, crate::lifecycle::hash)]
fn finalized_templates_never_change() {
    let (mut template, mut creator) = template(TEMPLATE_STATE_FINALIZED);
    let before = (template.data, template.header.lamports, template.header.owner, template.header.data_len);
    let mut write = [0u8; 9];
    write[0] = IX_WRITE_TEMPLATE_CHUNK;
    write[1..5].copy_from_slice(&kani::any::<u32>().to_le_bytes());
    write[5..].copy_from_slice(&kani::any::<[u8; 4]>());
    let chunk = kani::any_where(|n: &usize| (1..=4).contains(n));
    let instruction: &[u8] = match kani::any::<u8>() % 3 {
        0 => &write[..5 + chunk],
        1 => &[IX_FINALIZE_TEMPLATE],
        _ => &[IX_CANCEL_TEMPLATE],
    };
    let mut accounts: [AccountView; 2] = [creator.view(), template.view()];
    let result = process_instruction(&ballista::ID, &mut accounts, instruction);
    assert!(result.is_err());
    let after = (template.data, template.header.lamports, template.header.owner, template.header.data_len);
    assert!(before == after);
    kani::cover!(
        instruction[0] == IX_WRITE_TEMPLATE_CHUNK && creator.header.is_signer != 0 && creator.header.address.to_bytes() == template.data[4..36],
        "the creator itself tries a chunk write"
    );
    kani::cover!(instruction[0] == IX_CANCEL_TEMPLATE && creator.header.is_signer != 0, "a signed cancel");
}

/// An uploading template never runs: a `Run` fails, whatever the payload and inputs (up to 4
/// bytes), and leaves the account as it was. Bounded as `finalized_templates_never_change`.
#[kani::proof]
#[kani::unwind(93)]
#[kani::stub(ballista::utils::pda::get_template_address, crate::lifecycle::get_template_address)]
#[kani::stub(solana_sha256_hasher::hash, crate::lifecycle::hash)]
fn uploading_templates_never_run() {
    let (mut template, mut other) = template(TEMPLATE_STATE_UPLOADING);
    let before = (template.data, template.header.lamports);
    let mut run = [0u8; 5];
    run[0] = IX_RUN;
    run[1..].copy_from_slice(&kani::any::<[u8; 4]>());
    let len = kani::any_where(|n: &usize| (1..=5).contains(n));
    let mut accounts: [AccountView; 2] = [template.view(), other.view()];
    assert!(process_instruction(&ballista::ID, &mut accounts, &run[..len]).is_err());
    assert!((template.data, template.header.lamports) == before);
    kani::cover!(template.data[44..48] == (PAYLOAD as u32).to_le_bytes(), "a fully written upload still does not run");
}

/// A chunk write to an uploading template succeeds only when the creator signs and is writable,
/// the template is writable, the creator is the one the header names, and the chunk starts at the
/// written length and fits the payload (the address check is stubbed to any answer); it then writes
/// exactly those payload bytes and advances the written length by the chunk's size, and changes
/// nothing else. Any failure changes nothing at all. Bounded: a 12-byte payload, chunks of 1 to 4
/// bytes at any offset.
#[kani::proof]
#[kani::unwind(93)]
#[kani::stub(ballista::utils::pda::get_template_address, crate::lifecycle::get_template_address)]
#[kani::stub(solana_sha256_hasher::hash, crate::lifecycle::hash)]
fn chunk_writes_append_exactly_their_bytes() {
    let (mut template, mut creator) = template(TEMPLATE_STATE_UPLOADING);
    let before = template.data;
    let written = u32::from_le_bytes(before[44..48].try_into().unwrap()) as usize;
    let offset: u32 = kani::any();
    let bytes: [u8; 4] = kani::any();
    let len = kani::any_where(|n: &usize| (1..=4).contains(n));
    let mut write = [0u8; 9];
    write[0] = IX_WRITE_TEMPLATE_CHUNK;
    write[1..5].copy_from_slice(&offset.to_le_bytes());
    write[5..].copy_from_slice(&bytes);
    let creator_ok = creator.header.is_signer != 0
        && creator.header.is_writable != 0
        && template.header.is_writable != 0
        && creator.header.address.to_bytes() == before[4..36];
    let mut accounts: [AccountView; 2] = [creator.view(), template.view()];
    let result = process_instruction(&ballista::ID, &mut accounts, &write[..5 + len]);

    if result.is_ok() {
        assert!(creator_ok);
        assert_eq!(offset as usize, written);
        assert!(written + len <= PAYLOAD);
        for i in 0..DATA {
            let expected = if (44..48).contains(&i) {
                ((written + len) as u32).to_le_bytes()[i - 44]
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
        kani::cover!(creator_ok && offset as usize != written, "a chunk at the wrong offset");
    }
}

/// A cancel succeeds only for an uploading template, by its signing, writable creator, and then
/// moves all of the template's lamports to the creator (total unchanged) and closes the account;
/// any failure changes neither account's lamports. Bounded as `finalized_templates_never_change`.
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
