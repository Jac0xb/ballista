//! Programs, accounts and instructions built so the prover reads every byte at the width the
//! program reads it.
//!
//! The prover keys each heap cell by its address and the width it was written or first read at.
//! A load of a different width than the last store to a cell gets an unrelated value: only the
//! stack reconstructs a word from narrower stores, and only from two four-byte halves (see
//! `PointerDomain.reconstructFromIntegerCells` in the prover's source). `ProgramView::parse` reads
//! the header and records with word-sized loads (SBF allows unaligned loads, so LLVM merges
//! adjacent byte reads), which is why every rule that parsed a constant written one byte at a time
//! saw arbitrary bytes and could not prove.
//!
//! The helpers here write nothing the program reads at another width:
//!
//! - A [`program`] view points at freshly allocated, havoced heap memory. A rule constrains the
//!   fields it depends on with assumptions made through the program's own accessors, so the rule
//!   and the program read each field at the same width. The shape a successful parse guarantees
//!   (every section exactly as long as the header says) is assumed rather than parsed;
//!   `rules::parser::rule_parsed_sections_exactly_consume_the_payload` proves it.
//! - An [`InstructionSlot`] is an instruction record on the stack written at the widths the
//!   executor reads: one byte per operand, the immediate as one aligned eight-byte store. The
//!   record stays on the stack so the prover's scalar analysis knows the opcode, and slices away
//!   every other opcode's code before the memory analysis runs.

use ballista::processor::execute::RuntimeValue;
use ballista_common::template::*;
use cvlr::asserts::cvlr_assume;
use cvlr::nondet::havoc::{alloc_mut_ref_havoced, alloc_ref_havoced};
use cvlr::nondet::nondet;

use super::util::REGISTERS;

/// Most fixed accounts a symbolic program declares.
pub const MAX_FIXED_ACCOUNTS: usize = 3;
/// Most pubkeys a symbolic program declares.
pub const MAX_PUBKEYS: usize = 2;
/// Most CPI descriptors, and CPI account records, a symbolic program declares.
pub const MAX_CPIS: usize = 1;

/// An empty slice whose pointer is a heap allocation rather than the dangling address an empty
/// array literal has, which the prover's pointer analysis cannot classify.
pub fn empty<T: 'static>() -> &'static [T] {
    &alloc_ref_havoced::<[T; 1]>()[..0]
}

/// How many entries each section of a symbolic program has. Every other section is empty, the
/// program has no batch and no account groups, and its header agrees with the sections.
#[derive(Clone, Copy)]
pub struct Shape {
    pub fixed_accounts: usize,
    pub pubkeys: usize,
    pub registers: usize,
    pub cpis: usize,
    pub cpi_accounts: usize,
}

impl Shape {
    /// `fixed_accounts` fixed accounts and nothing else.
    pub const fn accounts(fixed_accounts: usize) -> Self {
        Self {
            fixed_accounts,
            pubkeys: 0,
            registers: 0,
            cpis: 0,
            cpi_accounts: 0,
        }
    }
}

/// A program view of the given shape over havoced heap memory. Record contents are unconstrained;
/// rules assume what they need through the records' own accessors.
pub fn program(shape: Shape) -> ProgramView<'static> {
    let header = alloc_ref_havoced::<ProgramHeader>();
    cvlr_assume!(header.fixed_account_count() == shape.fixed_accounts);
    cvlr_assume!(header.batch_stride() == 0);
    cvlr_assume!(header.account_group_count() == 0);
    cvlr_assume!(header.register_count() == shape.registers);
    cvlr_assume!(header.pubkey_count() == shape.pubkeys);
    cvlr_assume!(header.cpi_count() == shape.cpis);

    let accounts = alloc_ref_havoced::<[AccountConstraint; MAX_FIXED_ACCOUNTS]>();
    let pubkeys = alloc_ref_havoced::<[PubkeyRecord; MAX_PUBKEYS]>();
    let cpis = alloc_ref_havoced::<[CpiDescriptor; MAX_CPIS]>();
    let cpi_accounts = alloc_ref_havoced::<[CpiAccountRecord; MAX_CPIS]>();
    ProgramView {
        header,
        accounts: &accounts[..shape.fixed_accounts],
        inputs: empty(),
        instructions: empty(),
        cpis: &cpis[..shape.cpis],
        cpi_accounts: &cpi_accounts[..shape.cpi_accounts],
        data_segments: empty(),
        pubkeys: &pubkeys[..shape.pubkeys],
        blob: empty(),
    }
}

/// Assumes an account constraint holds exactly these values, read through the accessors
/// `validate_account` uses: one byte each for the flags and pubkey indexes, and four bytes for the
/// minimum data length.
pub fn assume_constraint(
    constraint: &AccountConstraint,
    flags: u8,
    address_index: u8,
    owner_index: u8,
    min_data_len: u32,
) {
    cvlr_assume!(constraint.flags == flags);
    cvlr_assume!(constraint.address_index == address_index);
    cvlr_assume!(constraint.owner_index == owner_index);
    cvlr_assume!(constraint.min_data_len() == min_data_len as usize);
}

/// An instruction record on the stack, eight-byte aligned at its immediate.
///
/// The record sits two bytes into the slot, so its immediate (record bytes 6..14) starts on an
/// eight-byte boundary and is written with one aligned store, the width the executor's
/// `InstructionRecord::immediate` loads it at. The six operand bytes and the two reserved bytes
/// are written with volatile stores of their own read widths, so LLVM cannot merge them.
///
/// The slot is written where the caller's stack holds it and the record is borrowed from there.
/// Returning the slot by value would copy it with eight-byte moves, and the prover does not
/// rebuild a word from eight one-byte stack cells: the copy's opcode would be unknown.
#[repr(C, align(8))]
pub struct InstructionSlot {
    _pad: [u8; 2],
    record: InstructionRecord,
}

impl InstructionSlot {
    /// Writes the record into `slot` and borrows it.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    pub fn write(
        slot: &mut core::mem::MaybeUninit<Self>,
        opcode: u8,
        dst: u8,
        a: u8,
        b: u8,
        c: u8,
        flags: u8,
        immediate: u64,
    ) -> &InstructionRecord {
        let base = slot.as_mut_ptr().cast::<u8>();
        // SAFETY: every offset lies inside the 24-byte slot; the immediate's offset is 8 and the
        // reserved bytes' 16, so both typed stores are aligned. Every byte of the record is
        // written before it is borrowed, and the padding is never read.
        unsafe {
            core::ptr::write_volatile(base.add(2), opcode);
            core::ptr::write_volatile(base.add(3), dst);
            core::ptr::write_volatile(base.add(4), a);
            core::ptr::write_volatile(base.add(5), b);
            core::ptr::write_volatile(base.add(6), c);
            core::ptr::write_volatile(base.add(7), flags);
            core::ptr::write_volatile(base.add(8).cast::<u64>(), immediate.to_le());
            core::ptr::write_volatile(base.add(16).cast::<u16>(), 0);
            &*base.add(2).cast::<InstructionRecord>()
        }
    }
}

/// The tag `RuntimeValue::U64` is stored with: under `repr(C, u8)` a variant's tag is its index.
pub const TAG_U64: u8 = 2;

/// `REGISTERS` registers on the heap, each a `u64` with a nondeterministic value. Each tag is
/// written with a one-byte store and each value with an eight-byte store, the widths the executor
/// reads them at; the other payload bytes stay havoced.
pub fn u64_registers() -> &'static mut [RuntimeValue<'static>] {
    let registers = alloc_mut_ref_havoced::<[RuntimeValue<'static>; REGISTERS]>();
    for register in registers.iter_mut() {
        let base = (register as *mut RuntimeValue).cast::<u8>();
        // SAFETY: `RuntimeValue` is `repr(C, u8)`: the tag is its first byte and a `U64`'s value
        // the eight bytes at offset 8. Both writes stay inside the 40-byte value.
        unsafe {
            core::ptr::write_volatile(base, TAG_U64);
            core::ptr::write_volatile(base.add(8).cast::<u64>(), nondet());
        }
    }
    &mut registers[..]
}

/// The four payload words of a register, bytes 8..40, each read with one eight-byte load at a
/// constant offset. The executor writes a register's payload with eight-byte stores, so these
/// read back exactly what it wrote; its one-byte tag it copies with an eight-byte move from a stack
/// temporary, which the prover cannot follow, so rules observe payloads rather than tags.
pub fn payload_words(value: &RuntimeValue<'_>) -> [u64; 4] {
    let base = (value as *const RuntimeValue).cast::<u8>();
    // SAFETY: every `RuntimeValue` is 40 bytes with its payload from offset 8.
    unsafe {
        [
            core::ptr::read_volatile(base.add(8).cast::<u64>()),
            core::ptr::read_volatile(base.add(16).cast::<u64>()),
            core::ptr::read_volatile(base.add(24).cast::<u64>()),
            core::ptr::read_volatile(base.add(32).cast::<u64>()),
        ]
    }
}

/// The four words of an address, each read with one eight-byte load: the width an account's
/// address is written and compared at.
pub fn address_words(address: &[u8; 32]) -> [u64; 4] {
    let word = |offset: usize| u64::from_le_bytes(address[offset..offset + 8].try_into().unwrap());
    [word(0), word(8), word(16), word(24)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_helpers_match_the_value_layout() {
        let value = RuntimeValue::U64(0x0102_0304_0506_0708);
        let base = (&value as *const RuntimeValue).cast::<u8>();
        assert_eq!(unsafe { *base }, TAG_U64);
        assert_eq!(payload_words(&value)[0], 0x0102_0304_0506_0708);
        assert_eq!(core::mem::size_of::<RuntimeValue>(), 40);
        let key = RuntimeValue::Pubkey([3; 32]);
        assert_eq!(payload_words(&key), address_words(&[3; 32]));
    }

    #[test]
    fn instruction_slots_hold_the_record_they_were_built_from() {
        let mut slot = core::mem::MaybeUninit::<InstructionSlot>::uninit();
        let slot_address = slot.as_ptr() as usize;
        let written = InstructionSlot::write(&mut slot, OP_ACCOUNT_KEY, 1, 2, NO_INDEX, 3, 4, 0x0102_0304_0506_0708);
        let expected = record(OP_ACCOUNT_KEY, 1, 2, NO_INDEX, 3, 4, 0x0102_0304_0506_0708);
        assert_eq!(written, &expected);
        let offset = written as *const InstructionRecord as usize - slot_address;
        assert_eq!(offset, 2);
        assert_eq!((offset + 6) % 8, 0, "the immediate is eight-byte aligned");
    }
}
