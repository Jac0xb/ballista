//! Helpers shared by the harnesses: symbolic choices, symbolic register values, and an
//! independent `u128` spec on 64-bit limbs.

use ballista::processor::execute::RuntimeValue;

/// One of `options`, chosen symbolically.
pub fn one_of<T: Copy>(options: &[T]) -> T {
    let index: usize = kani::any();
    kani::assume(index < options.len());
    options[index]
}

/// A register value of any variant with any payload. A `bytes` value borrows a symbolic-length
/// prefix of `bytes`, so its length ranges over `0..=bytes.len()`.
pub fn any_runtime_value(bytes: &[u8]) -> RuntimeValue<'_> {
    let variant: u8 = kani::any_where(|variant: &u8| *variant < 7);
    match variant {
        0 => RuntimeValue::Unset,
        1 => RuntimeValue::Bool(kani::any()),
        2 => RuntimeValue::U64(kani::any()),
        3 => RuntimeValue::I64(kani::any()),
        4 => RuntimeValue::U128(kani::any()),
        5 => RuntimeValue::Pubkey(kani::any()),
        _ => {
            let len: usize = kani::any_where(|len: &usize| *len <= bytes.len());
            RuntimeValue::Bytes(&bytes[..len])
        }
    }
}

/// A `u128` whose only symbolic bits are three `WINDOW`-bit windows: at bit 0, straddling bit 64
/// (the limb boundary), and at the top. Every other bit is zero. Products and quotients of such
/// values still carry across both limbs and overflow 128 bits, but the solver's multipliers and
/// dividers shrink to the windows' size. Used where a full-width `u128` multiply or divide on both
/// sides of a proof is out of reach.
pub fn windowed_u128(window: u32) -> u128 {
    let mask = (1u128 << window) - 1;
    let low: u128 = kani::any::<u128>() & mask;
    let middle: u128 = kani::any::<u128>() & mask;
    let top: u128 = kani::any::<u128>() & mask;
    low | (middle << (64 - window / 2)) | (top << (128 - window))
}

/// Two 32-byte arrays are equal, compared as two `u128` words: no loop, so a harness that compares
/// addresses keeps a small unwind bound.
pub fn eq32(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let word = |bytes: &[u8; 32], at: usize| {
        u128::from_le_bytes(bytes[at..at + 16].try_into().expect("16 bytes"))
    };
    word(a, 0) == word(b, 0) && word(a, 16) == word(b, 16)
}

/// Two register values are the same: the same variant and payload, and for `bytes` the very same
/// slice (pointer and length), which is what an untouched register holds. Loop-free.
pub fn same_value(a: &RuntimeValue<'_>, b: &RuntimeValue<'_>) -> bool {
    match (a, b) {
        (RuntimeValue::Unset, RuntimeValue::Unset) => true,
        (RuntimeValue::Bool(x), RuntimeValue::Bool(y)) => x == y,
        (RuntimeValue::U64(x), RuntimeValue::U64(y)) => x == y,
        (RuntimeValue::I64(x), RuntimeValue::I64(y)) => x == y,
        (RuntimeValue::U128(x), RuntimeValue::U128(y)) => u128::from_le_bytes(*x) == u128::from_le_bytes(*y),
        (RuntimeValue::Pubkey(x), RuntimeValue::Pubkey(y)) => eq32(x, y),
        (RuntimeValue::Bytes(x), RuntimeValue::Bytes(y)) => core::ptr::eq(*x, *y),
        _ => false,
    }
}

/// A `u64` whose only symbolic bits are three `window`-bit windows: at bit 0, straddling bit 32,
/// and at the top. Every other bit is zero.
pub fn windowed_u64(window: u32) -> u64 {
    let mask = (1u64 << window) - 1;
    let low = kani::any::<u64>() & mask;
    let middle = kani::any::<u64>() & mask;
    let top = kani::any::<u64>() & mask;
    low | (middle << (32 - window / 2)) | (top << (64 - window))
}

/// A windowed `u64` or its bitwise complement, as an `i64`: both signs, values near zero on
/// either side (`-1` is the complement of 0), and both extremes are reachable, while every bit
/// outside the windows stays constant.
pub fn windowed_i64(window: u32) -> i64 {
    let value = windowed_u64(window);
    (if kani::any() { value } else { !value }) as i64
}

/// A `u128` as two 64-bit limbs. Its arithmetic works limb by limb with explicit carries, so a spec
/// built on it shares no 128-bit operator with the code it checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct U128Parts {
    pub high: u64,
    pub low: u64,
}

impl U128Parts {
    pub fn of(value: u128) -> Self {
        Self { high: (value >> 64) as u64, low: value as u64 }
    }

    pub fn value(self) -> u128 {
        ((self.high as u128) << 64) | self.low as u128
    }

    /// `self + other`, or `None` past `u128::MAX`.
    pub fn add(self, other: Self) -> Option<u128> {
        let (low, carry) = self.low.overflowing_add(other.low);
        let high = self.high as u128 + other.high as u128 + carry as u128;
        (high >> 64 == 0).then(|| Self { high: high as u64, low }.value())
    }

    /// `self − other`, or `None` below zero.
    pub fn sub(self, other: Self) -> Option<u128> {
        let (low, borrow) = self.low.overflowing_sub(other.low);
        let high = self.high as i128 - other.high as i128 - borrow as i128;
        (high >= 0).then(|| Self { high: high as u64, low }.value())
    }

    pub fn less_than(self, other: Self) -> bool {
        self.high < other.high || (self.high == other.high && self.low < other.low)
    }

    /// The 256-bit product as `(high, low)` 128-bit halves, by schoolbook multiplication on 32-bit
    /// digits with 64-bit column sums: a different decomposition from `math::full_product`'s
    /// 64-bit one.
    pub fn widening_mul(self, other: Self) -> (u128, u128) {
        let digits = |parts: Self| {
            [
                parts.low & 0xffff_ffff,
                parts.low >> 32,
                parts.high & 0xffff_ffff,
                parts.high >> 32,
            ]
        };
        let (a, b) = (digits(self), digits(other));
        // Eight 32-bit result digits; each column sum is accumulated in a u128 with its carry.
        let mut result = [0u64; 8];
        let mut carry: u128 = 0;
        for column in 0..7 {
            let mut sum: u128 = carry;
            for i in 0..4 {
                if column >= i && column - i < 4 {
                    sum += a[i] as u128 * b[column - i] as u128;
                }
            }
            result[column] = (sum & 0xffff_ffff) as u64;
            carry = sum >> 32;
        }
        result[7] = carry as u64;
        let join = |d: &[u64]| {
            (d[0] as u128) | (d[1] as u128) << 32 | (d[2] as u128) << 64 | (d[3] as u128) << 96
        };
        (join(&result[4..8]), join(&result[0..4]))
    }
}

/// Records with every field symbolic. The wire types do not implement `kani::Arbitrary`, and adding
/// it would touch `ballista-common`, so these build them field by field.
pub mod any {
    use ballista_common::template::*;

    pub fn instruction() -> InstructionRecord {
        InstructionRecord {
            opcode: kani::any(),
            dst: kani::any(),
            a: kani::any(),
            b: kani::any(),
            c: kani::any(),
            flags: kani::any(),
            immediate_le: kani::any(),
            reserved: kani::any(),
        }
    }

    pub fn cpi_descriptor() -> CpiDescriptor {
        CpiDescriptor {
            program_account: kani::any(),
            account_group: kani::any(),
            account_start_le: kani::any(),
            account_len: kani::any(),
            segment_len: kani::any(),
            segment_start_le: kani::any(),
            max_data_len_le: kani::any(),
            reserved1: kani::any(),
        }
    }

    pub fn segment() -> DataSegment {
        DataSegment {
            kind: kani::any(),
            register: kani::any(),
            offset_le: kani::any(),
            len_le: kani::any(),
            reserved: kani::any(),
        }
    }

    pub fn input() -> InputDescriptor {
        InputDescriptor { value_type: kani::any(), reserved: kani::any(), max_len_le: kani::any() }
    }

    pub fn constraint() -> AccountConstraint {
        AccountConstraint {
            flags: kani::any(),
            address_index: kani::any(),
            owner_index: kani::any(),
            reserved: kani::any(),
            min_data_len_le: kani::any(),
        }
    }
}
