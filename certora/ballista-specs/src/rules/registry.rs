//! Registry entries open only when they are the entry the run names, and stay closed to reads and
//! writes until an open marks them.
//!
//! An open succeeds exactly when it reaches `try_borrow_mut`, which sets the entry's borrow byte from
//! `NOT_BORROWED` to 0, and nothing else in an open writes that byte. The rules observe that byte
//! rather than the `RunResult` an open returns: `registry::open` builds its errors on its own stack
//! from a two-byte tag and a four-byte code and copies them out with an eight-byte move across two
//! bytes it never wrote, and the prover does not rebuild a word from cells with a gap, so the
//! returned tag of a failed open is unknown to it. The byte is written with one-byte stores and read
//! the same way everywhere.
//!
//! Creating an entry (the System program CPIs) is opaque here, with a nondeterministic result (see
//! `envs/`); these rules are about entries that already exist.

use ballista::processor::execute::{execute_instruction, RuntimeValue, Scratch};
use ballista::processor::registry::{open, EntryId};
use ballista_common::template::*;
use cvlr::prelude::*;
use cvlr_pinocchio::{heap_views, nondet_address, AccountSlot};
use pinocchio::account::NOT_BORROWED;

use super::symbolic::{
    self, assume_constraint, empty, payload_words, u64_registers, InstructionSlot, Shape,
};
use super::util::REGISTERS;

/// Field bytes the entries in these rules hold, after the 72-byte header.
const FIELDS: usize = 16;
/// An entry account with room for the header and [`FIELDS`] field bytes.
const ENTRY_DATA: usize = REGISTRY_ENTRY_HEADER_LEN + FIELDS;

/// The eight-byte word of `data` at `offset`, read as one load, the width the open's header
/// comparison reads entry data at.
fn word(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
}

/// An existing entry: Ballista owns it, it is unopened and writable, and its data is havoced.
fn existing_entry() -> &'static mut AccountSlot<ENTRY_DATA> {
    let entry = AccountSlot::<ENTRY_DATA>::nondet();
    entry.header.owner = ballista::ID;
    entry.header.borrow_state = NOT_BORROWED;
    entry
}

fn payer() -> &'static mut AccountSlot<0> {
    AccountSlot::<0>::nondet()
}

/// A read-only entry account never opens: its borrow byte is untouched.
#[rule]
pub fn rule_registry_open_requires_a_writable_entry() {
    let entry = existing_entry();
    entry.header.is_writable = 0;
    // Any owner: creation is refused for a read-only account too.
    if nondet::<bool>() {
        entry.header.owner = nondet_address();
    }
    let payer = payer();
    let template = nondet_address();
    let id = EntryId { template: &template, index: nondet(), key: nondet_address().to_bytes() };
    let views = heap_views([entry.view(), payer.view()]);
    let _ = open(&views[0], &views[1], &id, FIELDS);
    cvlr_assert!(!views[0].is_borrowed_mut());
    cvlr_assert!(entry.header.borrow_state == NOT_BORROWED);
}

/// An existing entry whose data length is not the header plus the registry's field size never
/// opens, whatever its header holds.
#[rule]
pub fn rule_registry_open_checks_the_entry_size() {
    let entry = existing_entry();
    entry.header.is_writable = 1;
    let size: u16 = nondet();
    cvlr_assume!(usize::from(size) <= MAX_REGISTRY_SIZE);
    clog!(size, entry.header.data_len);
    let payer = payer();
    let template = nondet_address();
    let id = EntryId { template: &template, index: nondet(), key: nondet_address().to_bytes() };
    let views = heap_views([entry.view(), payer.view()]);
    let _ = open(&views[0], &views[1], &id, usize::from(size));
    if entry.header.data_len != (REGISTRY_ENTRY_HEADER_LEN + usize::from(size)) as u64 {
        cvlr_assert!(entry.header.borrow_state == NOT_BORROWED);
    }
}

/// An existing entry opens only if its header names the running template and the key: bytes
/// 8..40 of its data are the template's address and bytes 40..72 the key. Another template's
/// entry, or another key's, never opens.
///
/// Needs `-solanaOptimisticMemcmp true` (see `run-candidates-memcmp.conf`). The open compares the
/// 72-byte header it builds on its stack with one `memcmp`, and the first eight bytes of that
/// header are written with a four-byte, two one-byte and a two-byte store, which the prover only
/// compares word by word under that flag. The flag leaves that first word unconstrained, so this
/// rule says nothing about the magic, version and registry index in it.
#[rule]
pub fn rule_registry_open_binds_the_template_and_key() {
    let entry = existing_entry();
    entry.header.is_writable = 1;
    entry.header.data_len = ENTRY_DATA as u64;
    let payer = payer();
    let template = nondet_address();
    let key = nondet_address().to_bytes();
    let id = EntryId { template: &template, index: nondet(), key };
    let views = heap_views([entry.view(), payer.view()]);
    let _ = open(&views[0], &views[1], &id, FIELDS);
    if views[0].is_borrowed_mut() {
        // Written out rather than looped: every offset stays a constant, so the prover reads the
        // template and key words at known stack offsets.
        let data = &entry.data;
        let template = template.as_array();
        cvlr_assert!(word(data, 8) == word(template, 0));
        cvlr_assert!(word(data, 16) == word(template, 8));
        cvlr_assert!(word(data, 24) == word(template, 16));
        cvlr_assert!(word(data, 32) == word(template, 24));
        cvlr_assert!(word(data, 40) == word(&key, 0));
        cvlr_assert!(word(data, 48) == word(&key, 8));
        cvlr_assert!(word(data, 56) == word(&key, 16));
        cvlr_assert!(word(data, 64) == word(&key, 24));
    }
}

/// Opening an entry that is open already fails. This is the fix in `registry::open`: two account
/// slots that hold one entry (two keys of one registry that came out equal) can no longer both be
/// open, so no template can read both and lose one write to the other.
///
/// Blocked: the only difference between the two outcomes is the `RunResult` the second open
/// returns, whose tag the prover cannot see (see the module comment). Both opens leave the borrow
/// byte at 0. It proves once `RunError` is laid out without padding in its first word.
#[rule]
pub fn rule_registry_open_refuses_an_open_entry() {
    let entry = existing_entry();
    entry.header.is_writable = 1;
    entry.header.data_len = ENTRY_DATA as u64;
    // Marked open, as an open earlier in this run leaves it.
    entry.header.borrow_state = 0;
    let payer = payer();
    let template = nondet_address();
    let id = EntryId { template: &template, index: nondet(), key: nondet_address().to_bytes() };
    let views = heap_views([entry.view(), payer.view()]);
    cvlr_assert!(open(&views[0], &views[1], &id, FIELDS).is_err());
}

/// `READ_REGISTRY` and `WRITE_REGISTRY` touch nothing unless an open in this run marked the entry:
/// on an unmarked entry a read leaves its destination register as it was and a write leaves the
/// entry's data as it was. Observed through the register's value word and the field's data word,
/// both written with eight-byte stores.
#[rule]
pub fn rule_registry_fields_need_an_open_entry() {
    let program = symbolic::program(Shape {
        registers: REGISTERS,
        ..Shape::accounts(1)
    });
    assume_constraint(&program.accounts[0], ACCOUNT_WRITABLE, NO_INDEX, NO_INDEX, 0);
    let entry = existing_entry();
    entry.header.is_writable = 1;
    entry.header.data_len = ENTRY_DATA as u64;
    // Any state but open.
    let borrow_state: u8 = nondet();
    cvlr_assume!(borrow_state != 0);
    entry.header.borrow_state = borrow_state;
    let views = heap_views([entry.view()]);

    // Every register holds a u64, so the read and the write have operands of the right type.
    let registers = u64_registers();
    let before_register = payload_words(&registers[1])[0];
    let field = REGISTRY_ENTRY_HEADER_LEN;
    let before_field = word(&entry.data, field);

    let read: bool = nondet();
    let immediate = RegistryField { offset: 0, selector: OP_READ_U64 }.encode();
    let mut record = core::mem::MaybeUninit::uninit();
    let instruction = if read {
        InstructionSlot::write(&mut record, OP_READ_REGISTRY, 1, 0, NO_INDEX, NO_INDEX, 0, immediate)
    } else {
        InstructionSlot::write(&mut record, OP_WRITE_REGISTRY, NO_INDEX, 0, 0, NO_INDEX, 0, immediate)
    };
    let mut scratch = Scratch::new(&program);
    let inputs: &[RuntimeValue] = empty();
    let _ = execute_instruction(&program, inputs, &views[..], registers, &mut scratch, instruction, None);

    cvlr_assert!(payload_words(&registers[1])[0] == before_register);
    cvlr_assert!(word(&entry.data, field) == before_field);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio::Address;

    #[test]
    fn entries_hold_a_header_and_the_fields() {
        assert_eq!(ENTRY_DATA, 88);
        let template = Address::new_from_array([7; 32]);
        let id = EntryId { template: &template, index: 2, key: [9; 32] };
        let header = id.header();
        assert_eq!(word(&header, 8), u64::from_le_bytes([7; 8]));
        assert_eq!(word(&header, 40), u64::from_le_bytes([9; 8]));
    }
}
