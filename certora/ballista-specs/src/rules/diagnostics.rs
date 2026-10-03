//! Small rules that pin down how the prover models the memory the other rules depend on. They
//! carry no property of the program. Each states its expected result: the ones expected to fail
//! show a limit the blocked rules run into, and the ones expected to prove are their controls, so a
//! prover run confirms or refutes the diagnosis in `certora/README.md`.
//!
//! - Expected to fail: byte stores read back as a word (heap); the constant program parsed from
//!   byte stores, or from a `memcpy` of the binary's data (a global copied by `memcpy` is never
//!   initialized in the prover's encoding: only globals loaded directly and branched on, or compared
//!   by a 32-byte `memcmp`, are); the code at offset 4 of a stack word whose tag was a one-byte
//!   store, after one eight-byte copy, as executor errors are copied.
//! - Expected to prove: a word stored and read back as a word; both halves of the same copy when
//!   each half was a four-byte store, the one merge the prover rebuilds on the stack.
//! - Probe: whether the one-byte tag of that copy survives.

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

/// Expected to prove, the control for the first rule: a word stored and loaded at one width.
#[rule]
pub fn rule_heap_word_stores_read_back_as_words() {
    let heap = alloc_mut_ref_havoced::<u64>();
    // SAFETY: one aligned store and one aligned load of the allocation's only word.
    let word = unsafe {
        core::ptr::write_volatile(heap as *mut u64, 0x0807_0605_0403_0201);
        core::ptr::read_volatile(heap as *const u64)
    };
    cvlr_assert!(word == 0x0807_0605_0403_0201);
}

/// The shape of an error the executor returns: a tag at offset 0, a four-byte code at offset 4,
/// built in a stack temporary and copied to the heap with one eight-byte move, as
/// `validate_account`, `registry::open`, `mul_div` and `cast` do with theirs. The tag is one byte
/// with three bytes never written (`short`), or a full four-byte half. Returns the copied word,
/// read back at the width it was stored, so the stack copy is the only width change.
#[inline(always)]
fn copied_word(short: bool) -> u64 {
    let mut temporary = core::mem::MaybeUninit::<u64>::uninit();
    let base = temporary.as_mut_ptr().cast::<u8>();
    let heap = alloc_mut_ref_havoced::<u64>();
    // SAFETY: every access stays inside the eight-byte temporary or the eight-byte allocation; the
    // four-byte stores sit at offsets 0 and 4 of an eight-byte-aligned word.
    unsafe {
        if short {
            core::ptr::write_volatile(base, 7u8);
        } else {
            core::ptr::write_volatile(base.cast::<u32>(), 7);
        }
        core::ptr::write_volatile(base.add(4).cast::<u32>(), 6012);
        let word = core::ptr::read_volatile(base.cast::<u64>());
        core::ptr::write_volatile(heap as *mut u64, word);
        core::ptr::read_volatile(heap as *const u64)
    }
}

/// Expected to fail: the code at offset 4 does not survive a copy of a word whose first store was
/// narrower. The prover keeps stack values by offset and rebuilds a word only from two four-byte
/// halves, so the copy carries the tag's store alone, or an unknown value. Rules that read an
/// error's kind after such a copy (the account, `mul_div`, registry-refusal, ceiling and typing
/// rules) are suspected blocked on this.
#[rule]
pub fn rule_stack_word_copy_keeps_the_code_after_a_short_tag() {
    cvlr_assert!(copied_word(true) >> 32 == 6012);
}

/// A probe, either result informative: whether the one-byte tag itself survives the same copy. If
/// it proves, rules that read only an `Ok` or `Err` tag after such a copy are not blocked by it.
#[rule]
pub fn rule_stack_word_copy_keeps_a_short_tag() {
    cvlr_assert!(copied_word(true) & 0xff == 7);
}

/// Expected to prove, the control: the same copy keeps both halves when each was a four-byte
/// store, the one merge the prover rebuilds on the stack.
#[rule]
pub fn rule_stack_word_copy_keeps_both_halves() {
    let word = copied_word(false);
    cvlr_assert!(word & 0xffff_ffff == 7 && word >> 32 == 6012);
}
