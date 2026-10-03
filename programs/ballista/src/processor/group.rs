//! `GROUP_LENGTH`, `GROUP_ANY` and `GROUP_COUNT`: what a template can learn about an account group
//! the caller supplied. The length is the caller's own number from the run data. A filter tests
//! each member against the programs, the data-length floor, the match values and the except keys
//! the instruction names, in one bounded pass over the group: a group's members are runtime
//! accounts, at most `MAX_RUNTIME_ACCOUNTS` of them.
//!
//! A member's data is read in place, only while the filter tests that member, and only when no
//! mutable borrow of it is outstanding: nothing it reads outlives the opcode.

use ballista_common::template::*;
use pinocchio::AccountView;

use super::execute::{get, RunResult, RuntimeValue};
use crate::error::BallistaError;

/// How many of `members` match the filter `instruction` names, stopping at the first when `any`.
///
/// Out of line and not cold, unlike its caller: LLVM optimizes a cold function for size, which left
/// the per-member comparisons as calls and cost each member over a hundred compute units more.
#[inline(never)]
pub fn count_matches(
    program: &ProgramView<'_>,
    registers: &[RuntimeValue<'_>],
    instruction: &InstructionRecord,
    members: &[AccountView],
    any: bool,
) -> RunResult<u64> {
    let invalid = || BallistaError::InvalidTemplateProgram;
    let first = &program
        .pubkeys
        .get(instruction.b as usize)
        .ok_or_else(invalid)?
        .bytes;
    let second = match instruction.c {
        NO_INDEX => first,
        index => {
            &program
                .pubkeys
                .get(index as usize)
                .ok_or_else(invalid)?
                .bytes
        }
    };
    let filter = GroupScan::decode(instruction.immediate());
    let matches = filter.matches as usize;
    let excepts = filter.excepts as usize;
    if matches == 0 || matches > MAX_GROUP_MATCHES || excepts > MAX_GROUP_EXCEPTS {
        return Err(invalid().into());
    }
    let (start, end) = filter.segment_range();
    let segments = program.data_segments.get(start..end).ok_or_else(invalid)?;
    let (match_segments, except_segments) = segments.split_at(matches);

    // Each match value is encoded once, as invocation data encodes it, then compared with every
    // member's bytes at its offset, a little-endian word at a time.
    let mut values = [Match::default(); MAX_GROUP_MATCHES];
    for (segment, value) in match_segments.iter().zip(&mut values) {
        let mut bytes = [0u8; 32];
        let len = encode(segment, get(registers, segment.register)?, &mut bytes)?;
        *value = Match::new(segment.offset(), &bytes, len);
    }
    let mut keys = [[0u8; 32]; MAX_GROUP_EXCEPTS];
    for (segment, key) in except_segments.iter().zip(&mut keys) {
        match get(registers, segment.register)? {
            RuntimeValue::Pubkey(value) => *key = value,
            _ => return Err(BallistaError::TypeMismatch.into()),
        }
    }
    let values = &values[..matches];
    let keys = &keys[..excepts];
    let min_data_len = filter.min_data_len as usize;

    let mut count = 0u64;
    for member in members {
        let owner = member.owner().as_array();
        if (owner != first && owner != second) || member.data_len() < min_data_len {
            continue;
        }
        let address = member.address().as_array();
        if keys.iter().any(|key| key == address) {
            continue;
        }
        member.check_borrow()?;
        // SAFETY: `check_borrow` found no mutable borrow of the data, and nothing can take one
        // before the slice is dropped at the end of this iteration: nothing runs in between.
        let data = unsafe { member.borrow_unchecked() };
        if values.iter().all(|value| value.holds(data)) {
            // At most `MAX_RUNTIME_ACCOUNTS` members, so this cannot wrap.
            count = count.wrapping_add(1);
            if any {
                break;
            }
        }
    }
    Ok(count)
}

/// One match: the bytes a member's data must hold at `offset`, as little-endian words. A `bool` is
/// one byte, held in the low byte of the first word; the other types fill one, two or four words.
#[derive(Clone, Copy, Default)]
struct Match {
    offset: usize,
    len: usize,
    words: [u64; 4],
}

impl Match {
    fn new(offset: usize, bytes: &[u8; 32], len: usize) -> Self {
        let words = core::array::from_fn(|index| {
            let mut word = [0u8; 8];
            word.copy_from_slice(&bytes[index * 8..index * 8 + 8]);
            u64::from_le_bytes(word)
        });
        Self { offset, len, words }
    }

    /// Whether `data` holds this match's bytes, compared a word at a time with the widths spelled
    /// out. A slice comparison, and a loop over the words, both became a call to a byte-at-a-time
    /// `bcmp`: about 110 compute units a member for a 32-byte match, against about 20 this way.
    #[inline(always)]
    fn holds(&self, data: &[u8]) -> bool {
        // The offset is at most 65,535 and the length at most 32, so the sum cannot wrap.
        let Some(bytes) = data.get(self.offset..self.offset.wrapping_add(self.len)) else {
            return false;
        };
        let start = bytes.as_ptr().cast::<u64>();
        // SAFETY: `bytes` holds `len` bytes from `start`, and each arm reads only the words that
        // `len` covers. The reads are unaligned, which the SBF virtual machine allows.
        let word = |index: usize| u64::from_le(unsafe { start.add(index).read_unaligned() });
        let [first, second, third, fourth] = self.words;
        match self.len {
            1 => u64::from(bytes[0]) == first,
            8 => word(0) == first,
            16 => word(0) == first && word(1) == second,
            32 => word(0) == first && word(1) == second && word(2) == third && word(3) == fourth,
            _ => false,
        }
    }
}

/// Writes `value`, the register a match segment names, into `out` as the segment's kind encodes
/// it, and returns its width. The verifier pinned each kind to its own type, so this is a copy.
/// Kept apart from `encode_register_segment`: a second caller of that encoder's fixed-buffer copy
/// stopped the compiler inlining it into `derive_pda`, which cost a canonical-bump derivation 90
/// compute units.
#[inline(always)]
fn encode(segment: &DataSegment, value: RuntimeValue<'_>, out: &mut [u8; 32]) -> RunResult<usize> {
    let width = match (segment.kind, value) {
        (DATA_REG_BOOL, RuntimeValue::Bool(flag)) => {
            out[0] = u8::from(flag);
            1
        }
        (DATA_REG_U64, RuntimeValue::U64(number)) => {
            out[..8].copy_from_slice(&number.to_le_bytes());
            8
        }
        (DATA_REG_I64, RuntimeValue::I64(number)) => {
            out[..8].copy_from_slice(&number.to_le_bytes());
            8
        }
        (DATA_REG_U128, RuntimeValue::U128(bytes)) => {
            out[..16].copy_from_slice(&bytes);
            16
        }
        (DATA_REG_PUBKEY, RuntimeValue::Pubkey(key)) => {
            *out = key;
            32
        }
        _ => return Err(BallistaError::TypeMismatch.into()),
    };
    Ok(width)
}
