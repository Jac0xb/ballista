//! Small rules that pin down how the prover models the memory the other rules depend on. They
//! carry no property of the program; a failure here means the specs, not Ballista, need work.

use ballista_common::template::*;
use cvlr::nondet::havoc::alloc_mut_ref_havoced;
use cvlr::prelude::*;
use cvlr_pinocchio::heap_bytes;

use super::util::{heap_spec_program_pure, SPEC_PROGRAM_PURE};

/// Byte stores into a fresh heap allocation read back as the word they spell.
#[rule]
pub fn rule_heap_byte_stores_read_back_as_words() {
    let heap = alloc_mut_ref_havoced::<[u8; 8]>();
    let base = heap.as_mut_ptr();
    // SAFETY: eight stores into an eight-byte allocation, written out so no loop is involved.
    unsafe {
        core::ptr::write_volatile(base, 1);
        core::ptr::write_volatile(base.add(1), 2);
        core::ptr::write_volatile(base.add(2), 3);
        core::ptr::write_volatile(base.add(3), 4);
        core::ptr::write_volatile(base.add(4), 5);
        core::ptr::write_volatile(base.add(5), 6);
        core::ptr::write_volatile(base.add(6), 7);
        core::ptr::write_volatile(base.add(7), 8);
    }
    let word = u64::from_le_bytes(*heap);
    clog!(word);
    cvlr_assert!(word == 0x0807_0605_0403_0201);
    cvlr_assert!(heap[3] == 4);
}

/// The constant program written with byte stores parses and reports its header fields.
#[rule]
pub fn rule_constant_program_parses_from_byte_stores() {
    let bytes = heap_spec_program_pure();
    let first_word = u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]);
    clog!(first_word);
    let parsed = ProgramView::parse(bytes);
    let ok = parsed.is_ok();
    clog!(ok);
    cvlr_assert!(ok);
    if let Ok(program) = parsed {
        cvlr_assert!(program.header.register_count() == 4);
        cvlr_assert!(program.header.fixed_account_count() == 0);
        cvlr_assert!(program.pubkeys.len() == 2);
    }
}

/// The same constant copied from the binary's data section with one memcpy.
#[rule]
pub fn rule_constant_program_parses_from_memcpy() {
    let bytes = heap_bytes(&SPEC_PROGRAM_PURE);
    let first_word = u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]);
    clog!(first_word);
    let parsed = ProgramView::parse(bytes);
    let ok = parsed.is_ok();
    clog!(ok);
    cvlr_assert!(ok);
    if let Ok(program) = parsed {
        cvlr_assert!(program.header.register_count() == 4);
    }
}
