//! Integer operations beyond checked add, subtract, multiply and divide: multiply-then-divide
//! with an exact 256-bit product, remainder, shifts, bitwise operations and powers of ten. Each
//! returns the exact result or fails; none wraps, saturates or truncates silently.

use ballista_common::template::{OP_BIT_AND, OP_BIT_OR, OP_BIT_XOR, OP_REM, OP_SHL, OP_SHR};

use super::execute::{RunError, RunResult, RuntimeValue};
use crate::error::BallistaError;

/// `a × b ÷ c` for three `u64`s or three `u128`s, rounded down or, with `round_up`, up. The
/// product is exact, so only the quotient has to fit the operands' type.
#[inline(never)]
pub fn mul_div<'data>(
    round_up: bool,
    a: RuntimeValue<'data>,
    b: RuntimeValue<'data>,
    c: RuntimeValue<'data>,
) -> RunResult<RuntimeValue<'data>> {
    match (a, b, c) {
        (RuntimeValue::U64(a), RuntimeValue::U64(b), RuntimeValue::U64(c)) => {
            mul_div_u64(a, b, c, round_up).map(RuntimeValue::U64)
        }
        (RuntimeValue::U128(a), RuntimeValue::U128(b), RuntimeValue::U128(c)) => {
            let quotient = mul_div_u128(
                u128::from_le_bytes(a),
                u128::from_le_bytes(b),
                u128::from_le_bytes(c),
                round_up,
            )?;
            Ok(RuntimeValue::U128(quotient.to_le_bytes()))
        }
        _ => Err(BallistaError::TypeMismatch.into()),
    }
}

/// `a × b ÷ c` for three `u64`s, entirely in native 64-bit and 128-bit arithmetic. The `u128` arm
/// below has to reach for a `u128` division (compiler-rt's `__udivti3`) when the wide path is
/// unavoidable; a `u64` operation never needs to; going through it anyway (as `a × b ÷ c` did
/// before this) means paying for a division four times wider than the answer.
pub fn mul_div_u64(a: u64, b: u64, c: u64, round_up: bool) -> RunResult<u64> {
    if c == 0 {
        return Err(BallistaError::DivisionByZero.into());
    }
    let (high, low) = mul64(a, b);
    let (quotient, remainder) = if high == 0 {
        (low / c, low % c)
    } else if high < c {
        divlu(high, low, c)
    } else {
        // The quotient needs more than 64 bits.
        return Err(BallistaError::ArithmeticOverflow.into());
    };
    if round_up && remainder != 0 {
        quotient
            .checked_add(1)
            .ok_or_else(|| BallistaError::ArithmeticOverflow.into())
    } else {
        Ok(quotient)
    }
}

/// `a × b ÷ c` over `u128`, with the product held in 256 bits. Three fast paths skip a `u128`
/// division — compiler-rt's `__udivti3`/`__umodti3`; SBF has no hardware 128-bit divide, so that
/// is a software routine, not a native instruction, and avoiding it when a 64-bit divide will do
/// is worth the extra branch: one multiply when both factors fit `u64`, a native `u64` division
/// when the product and the divisor both do, and the remainder computed only when rounding up
/// needs it (`&&` short-circuits, so it is not computed at all when rounding down).
pub fn mul_div_u128(a: u128, b: u128, c: u128, round_up: bool) -> RunResult<u128> {
    if c == 0 {
        return Err(BallistaError::DivisionByZero.into());
    }
    let (high, low) = if (a | b) >> 64 == 0 {
        // Both factors fit `u64`: the compiler lowers this to one native 64 × 64 → 128 multiply
        // rather than the general (on SBF, software) 128 × 128 multiply `full_product` needs.
        (0, (a as u64 as u128) * (b as u64 as u128))
    } else {
        full_product(a, b)
    };
    let (quotient, inexact) = if high == 0 {
        if (low | c) >> 64 == 0 {
            let (low, c) = (low as u64, c as u64);
            ((low / c) as u128, round_up && low % c != 0)
        } else {
            (low / c, round_up && low % c != 0)
        }
    } else if high < c {
        let (quotient, remainder) = divide_wide(high, low, c);
        (quotient, round_up && remainder != 0)
    } else {
        // The quotient needs more than 128 bits.
        return Err(BallistaError::ArithmeticOverflow.into());
    };
    if inexact {
        quotient
            .checked_add(1)
            .ok_or_else(|| BallistaError::ArithmeticOverflow.into())
    } else {
        Ok(quotient)
    }
}

/// The 256-bit product of `a` and `b`, as its high and low 128-bit halves: `(high, low)`. The
/// order is the reverse of the standard library's unstable `u128::widening_mul`, which returns
/// `(low, high)`; this function has its own name so the two are never confused.
pub fn full_product(a: u128, b: u128) -> (u128, u128) {
    const WORD: u128 = u64::MAX as u128;
    let (a_high, a_low) = (a >> 64, a & WORD);
    let (b_high, b_low) = (b >> 64, b & WORD);
    // Four 64 × 64-bit products, none of which can overflow 128 bits.
    let low_low = a_low * b_low;
    let low_high = a_low * b_high;
    let high_low = a_high * b_low;
    let high_high = a_high * b_high;
    // Three terms below 2^64 each: their sum fits easily.
    let middle = (low_low >> 64) + (low_high & WORD) + (high_low & WORD);
    let low = (low_low & WORD) | (middle << 64);
    let high = high_high + (low_high >> 64) + (high_low >> 64) + (middle >> 64);
    (high, low)
}

/// `a × b` as `u64`s widened to 128 bits (`(high, low)`), entirely with native 32 × 32 → 64
/// multiplies. Unlike `(a as u128) * (b as u128)`, this never risks the compiler lowering the
/// multiply to compiler-rt's `__multi3`.
#[inline(always)]
fn mul64(a: u64, b: u64) -> (u64, u64) {
    const MASK: u64 = u32::MAX as u64;
    let (a1, a0) = (a >> 32, a & MASK);
    let (b1, b0) = (b >> 32, b & MASK);
    let p00 = a0 * b0;
    let p01 = a0 * b1;
    let p10 = a1 * b0;
    let p11 = a1 * b1;
    let middle = (p00 >> 32) + (p01 & MASK) + (p10 & MASK);
    let low = (p00 & MASK) | (middle << 32);
    let high = p11
        .wrapping_add(p01 >> 32)
        .wrapping_add(p10 >> 32)
        .wrapping_add(middle >> 32);
    (high, low)
}

/// `(high × 2^64 + low) ÷ divisor`, and the remainder — `(quotient, remainder)` — given
/// `high < divisor` so the quotient fits `u64`. Hacker's Delight's `divlu` (see also Go's
/// `math/bits.Div64`): normalize the divisor so its top bit is set, then two 32-bit digit
/// estimates, each corrected at most twice. Native 64-bit operations only, so this never calls
/// compiler-rt.
#[inline(always)]
fn divlu(high: u64, low: u64, divisor: u64) -> (u64, u64) {
    debug_assert!(high < divisor, "the quotient must fit u64");
    const HALF: u64 = 1 << 32;
    const MASK: u64 = HALF - 1;
    let shift = divisor.leading_zeros();
    let divisor = divisor << shift;
    let (d1, d0) = (divisor >> 32, divisor & MASK);
    // `low >> 64` would panic (the shift amount would equal the width) when `shift` is 0.
    let n32 = if shift == 0 { high } else { (high << shift) | (low >> (64 - shift)) };
    let n10 = low << shift;
    let (n1, n0) = (n10 >> 32, n10 & MASK);

    let mut q1 = n32 / d1;
    let mut rhat = n32 - q1 * d1;
    while q1 >= HALF || q1 * d0 > (rhat << 32) + n1 {
        q1 -= 1;
        rhat += d1;
        if rhat >= HALF {
            break;
        }
    }
    let n21 = (n32 << 32).wrapping_add(n1).wrapping_sub(q1.wrapping_mul(divisor));
    let mut q0 = n21 / d1;
    rhat = n21 - q0 * d1;
    while q0 >= HALF || q0 * d0 > (rhat << 32) + n0 {
        q0 -= 1;
        rhat += d1;
        if rhat >= HALF {
            break;
        }
    }
    let remainder = (n21 << 32).wrapping_add(n0).wrapping_sub(q0.wrapping_mul(divisor)) >> shift;
    ((q1 << 32) | q0, remainder)
}

/// One 3-by-2 step of Knuth's Algorithm D with 64-bit digits: `(rem × 2^64 + digit) ÷ divisor` —
/// `(quotient, remainder)` — given a normalized divisor (top bit set) and `rem < divisor`. The
/// quotient fits `u64`.
#[inline(always)]
fn divide_3by2(rem: u128, digit: u64, divisor: u128) -> (u64, u128) {
    debug_assert!(rem < divisor, "the quotient digit must fit u64");
    let (d1, d0) = ((divisor >> 64) as u64, divisor as u64);
    let (r1, r0) = ((rem >> 64) as u64, rem as u64);
    // Estimate from the top digits; Knuth's Theorem B bounds it to at most 2 above the true digit
    // once the divisor is normalized. `divlu` needs `r1 < d1` to fit its own quotient in `u64`;
    // when it does not, the true digit is already `u64::MAX` or within 1 of it, so capping the
    // estimate there is simpler than computing it.
    let mut q = if r1 >= d1 { u64::MAX } else { divlu(r1, r0, d1).0 };
    // `q × divisor` as 192 bits: (top, bottom).
    let (p1_hi, p1_lo) = mul64(q, d1);
    let (p0_hi, p0_lo) = mul64(q, d0);
    let (mid, carry) = p1_lo.overflowing_add(p0_hi);
    let mut top = p1_hi + carry as u64;
    let mut bottom = ((mid as u128) << 64) | p0_lo as u128;
    // The dividend window as 192 bits.
    let (n_top, n_bottom) = (r1, ((r0 as u128) << 64) | digit as u128);
    // Normalization bounds this to at most two corrections.
    while top > n_top || (top == n_top && bottom > n_bottom) {
        q -= 1;
        let (b, borrow) = bottom.overflowing_sub(divisor);
        bottom = b;
        top -= borrow as u64;
    }
    (q, n_bottom.wrapping_sub(bottom))
}

/// `(high × 2^128 + low) ÷ divisor`, and the remainder — `(quotient, remainder)` — given
/// `high < divisor` so the quotient fits 128 bits.
///
/// Two-digit Knuth Algorithm D on 64-bit digits (Hacker's Delight §9.5): the divisor is
/// normalized so its top bit is set, each digit's estimate comes from `divlu` and is corrected at
/// most twice in `divide_3by2`, and a divisor below 2^64 collapses to two direct `divlu` calls,
/// needing neither normalization nor a correction loop. This only runs when the product does not
/// fit 128 bits; `mul_div_u128`'s fast paths take a plain `u64` or `u128` division instead when
/// they can.
fn divide_wide(high: u128, low: u128, divisor: u128) -> (u128, u128) {
    debug_assert!(high < divisor, "the quotient must fit 128 bits");
    if divisor >> 64 == 0 {
        // A one-digit divisor: two direct `divlu` calls over the dividend's 64-bit digits need no
        // normalization or correction loop.
        let divisor = divisor as u64;
        let (q1, remainder) = divlu(high as u64, (low >> 64) as u64, divisor);
        let (q0, remainder) = divlu(remainder, low as u64, divisor);
        let quotient = ((q1 as u128) << 64) | q0 as u128;
        return (quotient, remainder as u128);
    }
    // Normalize so the divisor's top bit is set: Knuth's Theorem B then bounds each digit's
    // estimate to at most 2 above the true digit, so `divide_3by2`'s correction loop runs at most
    // twice.
    let shift = ((divisor >> 64) as u64).leading_zeros();
    let divisor = divisor << shift;
    // `high < divisor` before the shift, and the un-normalized divisor's top limb has `shift`
    // leading zeros, so `high` is below `2^(128 - shift)`: its top `shift` bits are already zero,
    // and shifting it left loses nothing. `shift` can be 0, when `low >> (128 - shift)` would
    // panic (the shift amount would equal the width), so that case skips the shift instead.
    let high = if shift == 0 { high } else { (high << shift) | (low >> (128 - shift)) };
    let low = low << shift;
    let (q1, remainder) = divide_3by2(high, (low >> 64) as u64, divisor);
    let (q0, remainder) = divide_3by2(remainder, low as u64, divisor);
    let quotient = ((q1 as u128) << 64) | q0 as u128;
    (quotient, remainder >> shift)
}

/// Remainder, shifts and bitwise operations: the two-operand integer opcodes that are not in
/// `execute::arithmetic`.
#[inline(always)]
pub fn integer<'data>(
    opcode: u8,
    left: RuntimeValue<'data>,
    right: RuntimeValue<'data>,
) -> RunResult<RuntimeValue<'data>> {
    match opcode {
        OP_REM => remainder(left, right),
        OP_SHL | OP_SHR => shift(opcode == OP_SHL, left, right),
        OP_BIT_AND | OP_BIT_OR | OP_BIT_XOR => bitwise(opcode, left, right),
        _ => Err(BallistaError::InvalidTemplateProgram.into()),
    }
}

fn remainder<'data>(
    left: RuntimeValue<'data>,
    right: RuntimeValue<'data>,
) -> RunResult<RuntimeValue<'data>> {
    // `checked_rem` fails for a zero divisor and for `i64::MIN % -1`, whose quotient overflows.
    macro_rules! checked {
        ($left:expr, $right:expr) => {
            $left.checked_rem($right).ok_or_else(|| {
                RunError::from(if $right == 0 {
                    BallistaError::DivisionByZero
                } else {
                    BallistaError::ArithmeticOverflow
                })
            })?
        };
    }
    Ok(match (left, right) {
        (RuntimeValue::U64(left), RuntimeValue::U64(right)) => RuntimeValue::U64(checked!(left, right)),
        (RuntimeValue::I64(left), RuntimeValue::I64(right)) => RuntimeValue::I64(checked!(left, right)),
        (RuntimeValue::U128(left), RuntimeValue::U128(right)) => {
            let (left, right) = (u128::from_le_bytes(left), u128::from_le_bytes(right));
            RuntimeValue::U128(checked!(left, right).to_le_bytes())
        }
        _ => return Err(BallistaError::TypeMismatch.into()),
    })
}

fn shift<'data>(
    left_shift: bool,
    value: RuntimeValue<'data>,
    bits: RuntimeValue<'data>,
) -> RunResult<RuntimeValue<'data>> {
    let RuntimeValue::U64(bits) = bits else {
        return Err(BallistaError::TypeMismatch.into());
    };
    // A left shift that loses a set bit fails; a right shift is floor division by 2^bits.
    macro_rules! shifted {
        ($value:expr, $width:expr) => {{
            let value = $value;
            if bits >= $width {
                if left_shift && value != 0 {
                    return Err(BallistaError::ArithmeticOverflow.into());
                }
                0
            } else if left_shift {
                let result = value << bits;
                if result >> bits != value {
                    return Err(BallistaError::ArithmeticOverflow.into());
                }
                result
            } else {
                value >> bits
            }
        }};
    }
    Ok(match value {
        RuntimeValue::U64(value) => RuntimeValue::U64(shifted!(value, 64)),
        RuntimeValue::U128(value) => {
            RuntimeValue::U128(shifted!(u128::from_le_bytes(value), 128).to_le_bytes())
        }
        _ => return Err(BallistaError::TypeMismatch.into()),
    })
}

fn bitwise<'data>(
    opcode: u8,
    left: RuntimeValue<'data>,
    right: RuntimeValue<'data>,
) -> RunResult<RuntimeValue<'data>> {
    macro_rules! apply {
        ($left:expr, $right:expr) => {
            match opcode {
                OP_BIT_AND => $left & $right,
                OP_BIT_OR => $left | $right,
                OP_BIT_XOR => $left ^ $right,
                // Unreachable: `integer` only calls `bitwise` for these three opcodes.
                _ => return Err(BallistaError::InvalidTemplateProgram.into()),
            }
        };
    }
    Ok(match (left, right) {
        (RuntimeValue::U64(left), RuntimeValue::U64(right)) => RuntimeValue::U64(apply!(left, right)),
        (RuntimeValue::U128(left), RuntimeValue::U128(right)) => RuntimeValue::U128(
            apply!(u128::from_le_bytes(left), u128::from_le_bytes(right)).to_le_bytes(),
        ),
        _ => return Err(BallistaError::TypeMismatch.into()),
    })
}

/// `10^0` through `10^38`, the largest power of ten a `u128` holds.
const POWERS_OF_TEN: [u128; 39] = {
    let mut table = [1u128; 39];
    let mut index = 1;
    while index < table.len() {
        table[index] = table[index - 1] * 10;
        index += 1;
    }
    table
};

/// `10^exponent` as a `u128`.
#[inline(always)]
pub fn pow10<'data>(exponent: RuntimeValue<'data>) -> RunResult<RuntimeValue<'data>> {
    let RuntimeValue::U64(exponent) = exponent else {
        return Err(BallistaError::TypeMismatch.into());
    };
    usize::try_from(exponent)
        .ok()
        .and_then(|index| POWERS_OF_TEN.get(index))
        .map(|value| RuntimeValue::U128(value.to_le_bytes()))
        .ok_or_else(|| BallistaError::ArithmeticOverflow.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use RuntimeValue::{Bool, I64, U128, U64};

    fn err(error: BallistaError) -> RunError {
        error.into()
    }

    /// xorshift64: deterministic, dependency-free inputs for the property checks.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        /// A `u128` of a random magnitude, so both the one-word and the wide paths are hit.
        fn wide(&mut self) -> u128 {
            let value = ((self.next() as u128) << 64) | self.next() as u128;
            match self.next() % 4 {
                0 => value >> (self.next() % 128),
                1 => value,
                2 => value & u64::MAX as u128,
                _ => u128::MAX - (self.next() as u128),
            }
        }
    }

    /// Shift-and-add multiplication: slow, obviously correct, and independent of `full_product`.
    fn slow_mul(a: u128, b: u128) -> (u128, u128) {
        let (mut high, mut low) = (0u128, 0u128);
        for bit in 0..128 {
            if b >> bit & 1 == 1 {
                // Add `a << bit` as a 256-bit value.
                let add_low = if bit == 0 { a } else { a << bit };
                let add_high = if bit == 0 { 0 } else { a >> (128 - bit) };
                let (sum, carry) = low.overflowing_add(add_low);
                low = sum;
                high = high + add_high + u128::from(carry);
            }
        }
        (high, low)
    }

    #[test]
    fn full_product_matches_shift_and_add() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        for _ in 0..20_000 {
            let (a, b) = (rng.wide(), rng.wide());
            assert_eq!(full_product(a, b), slow_mul(a, b), "{a} × {b}");
        }
        assert_eq!(full_product(u128::MAX, u128::MAX), (u128::MAX - 1, 1));
        assert_eq!(full_product(u128::MAX, 1), (0, u128::MAX));
        assert_eq!(full_product(1 << 64, 1 << 64), (1, 0));
    }

    /// `q` and `r` are the floor quotient and remainder of `a × b ÷ c` exactly when
    /// `q × c + r = a × b` and `r < c`: division with remainder is unique. `a × b` is computed
    /// with `slow_mul`, not `full_product`, so this checker cannot share a bug with the code
    /// under test.
    fn is_floor_division(a: u128, b: u128, c: u128, q: u128) -> bool {
        let (product_high, product_low) = slow_mul(a, b);
        let (qc_high, qc_low) = full_product(q, c);
        // r = a × b − q × c, as 256 bits; it must be below c.
        let (r_low, borrow) = product_low.overflowing_sub(qc_low);
        let Some(r_high) = product_high.checked_sub(qc_high + u128::from(borrow)) else {
            return false;
        };
        r_high == 0 && r_low < c
    }

    #[test]
    fn mul_div_is_the_exact_floor_or_ceiling() {
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        let mut wide_cases = 0;
        for _ in 0..50_000 {
            let (a, b) = (rng.wide(), rng.wide());
            let c = rng.wide().max(1);
            let (high, _) = full_product(a, b);
            match mul_div_u128(a, b, c, false) {
                Ok(q) => {
                    assert!(is_floor_division(a, b, c, q), "{a} × {b} ÷ {c} gave {q}");
                    // The assert above established floor division; whether it is exact (no
                    // remainder) is all that decides whether the ceiling is `q` or `q + 1`.
                    let exact = full_product(q, c) == slow_mul(a, b);
                    let expected_ceiling = if exact { Ok(q) } else { q.checked_add(1).ok_or(()) };
                    match (mul_div_u128(a, b, c, true), expected_ceiling) {
                        (Ok(ceiling), Ok(expected)) => assert_eq!(ceiling, expected),
                        (Err(error), Err(())) => assert_eq!(error, err(BallistaError::ArithmeticOverflow)),
                        (got, expected) => panic!("ceiling of {a} × {b} ÷ {c}: {got:?} vs {expected:?}"),
                    }
                    if high != 0 {
                        wide_cases += 1;
                    }
                }
                Err(error) => {
                    assert_eq!(error, err(BallistaError::ArithmeticOverflow));
                    // Overflow means the true quotient needs more than 128 bits: high ≥ c.
                    assert!(high >= c, "{a} × {b} ÷ {c} overflowed but fits");
                    // The floor already overflows, so the ceiling — at least as large — must too.
                    assert_eq!(
                        mul_div_u128(a, b, c, true),
                        Err(err(BallistaError::ArithmeticOverflow)),
                        "{a} × {b} ÷ {c}: floor overflowed but ceiling did not"
                    );
                }
            }
        }
        assert!(wide_cases > 1_000, "the 256-bit path was barely exercised: {wide_cases}");
    }

    #[test]
    fn mul_div_edges() {
        assert_eq!(mul_div_u128(1, 1, 0, false), Err(err(BallistaError::DivisionByZero)));
        assert_eq!(mul_div_u128(0, 0, 0, false), Err(err(BallistaError::DivisionByZero)));
        assert_eq!(mul_div_u128(0, 0, 0, true), Err(err(BallistaError::DivisionByZero)));
        assert_eq!(mul_div_u128(0, u128::MAX, 7, true), Ok(0));
        assert_eq!(mul_div_u128(u128::MAX, u128::MAX, u128::MAX, false), Ok(u128::MAX));
        assert_eq!(mul_div_u128(u128::MAX, u128::MAX, u128::MAX, true), Ok(u128::MAX));
        assert_eq!(mul_div_u128(u128::MAX, 2, 1, false), Err(err(BallistaError::ArithmeticOverflow)));
        // c = 1: the quotient is the product itself, exactly, however wide.
        assert_eq!(mul_div_u128(12345, 67890, 1, false), Ok(12345 * 67890));
        assert_eq!(mul_div_u128(u128::MAX, 1, 1, true), Ok(u128::MAX));
        // (2^100 + 1)^2 ÷ 2^90 = 2^110 + 2^11 + 2^-90: rounds to 2^110 + 2048, or up by one.
        let a = (1u128 << 100) + 1;
        assert_eq!(mul_div_u128(a, a, 1 << 90, false), Ok((1 << 110) + 2048));
        assert_eq!(mul_div_u128(a, a, 1 << 90, true), Ok((1 << 110) + 2049));
        // b == c: the quotient is a exactly, whatever a is, so there is nothing to round up.
        assert_eq!(
            mul_div_u128(u128::MAX, 3, 3, true),
            Ok(u128::MAX),
            "exact, so no rounding"
        );
        assert_eq!(
            mul_div_u128(u128::MAX, u128::MAX, u128::MAX - 1, false),
            Err(err(BallistaError::ArithmeticOverflow))
        );
        // The floor lands exactly on the type's maximum with a nonzero remainder: the ceiling
        // overflows. b = (2^129 - 1) ÷ 7, so 7 × b = 2^129 - 1 = 2 × u128::MAX + 1, and that
        // halved floors to u128::MAX with a remainder of 1.
        let b = 0x4924_9249_2492_4924_9249_2492_4924_9249;
        assert_eq!(mul_div_u128(7, b, 2, false), Ok(u128::MAX));
        assert_eq!(mul_div_u128(7, b, 2, true), Err(err(BallistaError::ArithmeticOverflow)));
    }

    #[test]
    fn mul_div_on_registers() {
        assert_eq!(mul_div(false, U64(1_000_003), U64(7), U64(3)), Ok(U64(2_333_340)));
        assert_eq!(mul_div(true, U64(1_000_003), U64(7), U64(3)), Ok(U64(2_333_341)));
        assert_eq!(mul_div(false, U64(u64::MAX), U64(u64::MAX), U64(u64::MAX)), Ok(U64(u64::MAX)));
        assert_eq!(
            mul_div(false, U64(u64::MAX), U64(2), U64(1)),
            Err(err(BallistaError::ArithmeticOverflow))
        );
        assert_eq!(mul_div(false, U64(1), U64(1), U64(0)), Err(err(BallistaError::DivisionByZero)));
        // The floor lands exactly on u64::MAX with a remainder: the ceiling overflows.
        assert_eq!(mul_div(false, U64(31), U64(1_190_112_520_884_487_201), U64(2)), Ok(U64(u64::MAX)));
        assert_eq!(
            mul_div(true, U64(31), U64(1_190_112_520_884_487_201), U64(2)),
            Err(err(BallistaError::ArithmeticOverflow))
        );
        // c = 1 on registers too.
        assert_eq!(mul_div(false, U64(123), U64(456), U64(1)), Ok(U64(123 * 456)));
        let a = (1u128 << 100) + 1;
        assert_eq!(
            mul_div(false, U128(a.to_le_bytes()), U128(a.to_le_bytes()), U128((1u128 << 90).to_le_bytes())),
            Ok(U128(((1u128 << 110) + 2048).to_le_bytes()))
        );
        assert_eq!(
            mul_div(false, U128(u128::MAX.to_le_bytes()), U128(1u128.to_le_bytes()), U128(1u128.to_le_bytes())),
            Ok(U128(u128::MAX.to_le_bytes()))
        );
        assert_eq!(
            mul_div(false, U64(1), U128(1u128.to_le_bytes()), U64(1)),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(mul_div(false, I64(1), I64(1), I64(1)), Err(err(BallistaError::TypeMismatch)));
    }

    #[test]
    fn mul_div_u64_matches_the_u128_path() {
        // The old `u64` arm before it got its own native path: narrow `mul_div_u128`'s answer.
        fn via_u128(a: u64, b: u64, c: u64, round_up: bool) -> RunResult<u64> {
            let quotient = mul_div_u128(a as u128, b as u128, c as u128, round_up)?;
            u64::try_from(quotient).map_err(|_| BallistaError::ArithmeticOverflow.into())
        }
        let mut rng = Rng(0x1111_2222_3333_4444);
        for _ in 0..50_000 {
            let (a, b, c) = (rng.next(), rng.next(), rng.next().max(1));
            let round_up = rng.next().is_multiple_of(2);
            assert_eq!(
                mul_div_u64(a, b, c, round_up),
                via_u128(a, b, c, round_up),
                "{a} × {b} ÷ {c}, round_up = {round_up}"
            );
        }
        assert_eq!(mul_div_u64(0, u64::MAX, 7, true), Ok(0));
        assert_eq!(mul_div_u64(0, 0, 0, false), Err(err(BallistaError::DivisionByZero)));
    }

    #[test]
    fn knuth_d_matches_the_bit_by_bit_reference() {
        /// The original bit-by-bit long division, kept only as an independent reference now that
        /// `divide_wide` uses Knuth D: agreement with this slow-but-obviously-correct version
        /// over many random and structured cases is strong evidence the replacement is right.
        fn divide_wide(mut high: u128, mut low: u128, divisor: u128) -> (u128, u128) {
            for _ in 0..128 {
                let carry = high >> 127;
                high = (high << 1) | (low >> 127);
                low <<= 1;
                if carry != 0 || high >= divisor {
                    high = high.wrapping_sub(divisor);
                    low |= 1;
                }
            }
            (low, high)
        }

        fn check(high: u128, low: u128, divisor: u128) {
            assert_eq!(
                super::divide_wide(high, low, divisor),
                divide_wide(high, low, divisor),
                "({high} << 128 | {low}) ÷ {divisor}"
            );
        }

        let mut rng = Rng(0x1234_5678_9abc_def1);
        for _ in 0..50_000 {
            let divisor = rng.wide().max(1);
            let high = rng.wide() % divisor;
            let low = rng.wide();
            check(high, low, divisor);
        }

        // A divisor whose top limb is exactly 2^63 (the smallest normalization shift, 0) and
        // whose low limb is all ones.
        let divisor = (1u128 << 127) | u64::MAX as u128;
        check(divisor - 1, u128::MAX, divisor);
        check(0, 12345, divisor);

        // high == c - 1: the largest dividend high half that does not overflow.
        let divisor = 0x9e37_79b9_7f4a_7c15_2545_f491_4f6c_dd1d;
        check(divisor - 1, u128::MAX, divisor);
        check(divisor - 1, 0, divisor);

        // c just above 2^64: the two-digit path with the smallest possible top limb.
        let divisor = (1u128 << 64) + 1;
        check(0, u128::MAX, divisor);
        check(divisor - 1, 42, divisor);

        // c in [2^127, 2^128): the widest divisors, needing no normalization shift.
        check(0, u128::MAX, u128::MAX);
        check((1u128 << 127) - 1, u128::MAX, 1u128 << 127);
        check(1, 0, (1u128 << 127) + 12345);

        // c below 2^64: the two-plain-`divlu`-steps path.
        check(0, u128::MAX, 3);
        check(2, u128::MAX, u64::MAX as u128);
        check(0, 0, 1);
    }

    #[test]
    fn divlu_matches_u128_division() {
        let mut rng = Rng(0xfeed_face_dead_beef);
        for _ in 0..50_000 {
            let divisor = rng.next().max(1);
            let high = rng.next() % divisor; // high < divisor, so the quotient fits u64.
            let low = rng.next();
            let dividend = ((high as u128) << 64) | low as u128;
            let (quotient, remainder) = divlu(high, low, divisor);
            assert_eq!(quotient as u128, dividend / divisor as u128, "{high} {low} {divisor}");
            assert_eq!(remainder as u128, dividend % divisor as u128, "{high} {low} {divisor}");
        }
        // A divisor that is already normalized (top bit set): `shift` is 0.
        assert_eq!(divlu(0, u64::MAX, 1u64 << 63), (1, u64::MAX >> 1));
        assert_eq!(divlu((1 << 63) - 1, u64::MAX, 1u64 << 63), (u64::MAX, u64::MAX >> 1));
    }

    #[test]
    fn remainder_follows_rust() {
        assert_eq!(integer(OP_REM, U64(1_000_003), U64(3)), Ok(U64(1)));
        assert_eq!(integer(OP_REM, I64(-7), I64(3)), Ok(I64(-1)));
        assert_eq!(integer(OP_REM, I64(7), I64(-3)), Ok(I64(1)));
        assert_eq!(
            integer(OP_REM, U128(10u128.to_le_bytes()), U128(4u128.to_le_bytes())),
            Ok(U128(2u128.to_le_bytes()))
        );
        assert_eq!(integer(OP_REM, U64(1), U64(0)), Err(err(BallistaError::DivisionByZero)));
        assert_eq!(integer(OP_REM, I64(1), I64(0)), Err(err(BallistaError::DivisionByZero)));
        assert_eq!(
            integer(OP_REM, U128(10u128.to_le_bytes()), U128(0u128.to_le_bytes())),
            Err(err(BallistaError::DivisionByZero))
        );
        assert_eq!(
            integer(OP_REM, I64(i64::MIN), I64(-1)),
            Err(err(BallistaError::ArithmeticOverflow))
        );
        assert_eq!(integer(OP_REM, U64(1), I64(1)), Err(err(BallistaError::TypeMismatch)));
    }

    #[test]
    fn shifts_never_drop_a_set_bit_silently() {
        assert_eq!(integer(OP_SHL, U64(1), U64(63)), Ok(U64(1 << 63)));
        assert_eq!(integer(OP_SHL, U64(2), U64(63)), Err(err(BallistaError::ArithmeticOverflow)));
        assert_eq!(integer(OP_SHL, U64(0), U64(500)), Ok(U64(0)));
        assert_eq!(integer(OP_SHL, U64(1), U64(64)), Err(err(BallistaError::ArithmeticOverflow)));
        assert_eq!(integer(OP_SHR, U64(0xabcd), U64(8)), Ok(U64(0xab)));
        assert_eq!(integer(OP_SHR, U64(u64::MAX), U64(64)), Ok(U64(0)));
        // A shift of zero is a no-op, however the value's bits are set.
        assert_eq!(integer(OP_SHL, U64(u64::MAX), U64(0)), Ok(U64(u64::MAX)));
        assert_eq!(integer(OP_SHL, U64(1 << 63), U64(0)), Ok(U64(1 << 63)));
        assert_eq!(integer(OP_SHR, U64(u64::MAX), U64(0)), Ok(U64(u64::MAX)));
        assert_eq!(integer(OP_SHR, U64(1 << 63), U64(0)), Ok(U64(1 << 63)));
        // A zero value survives any shift amount.
        assert_eq!(integer(OP_SHL, U64(0), U64(u64::MAX)), Ok(U64(0)));
        assert_eq!(integer(OP_SHR, U64(0), U64(u64::MAX)), Ok(U64(0)));
        assert_eq!(
            integer(OP_SHL, U128(1u128.to_le_bytes()), U64(127)),
            Ok(U128((1u128 << 127).to_le_bytes()))
        );
        assert_eq!(
            integer(OP_SHL, U128(3u128.to_le_bytes()), U64(127)),
            Err(err(BallistaError::ArithmeticOverflow))
        );
        assert_eq!(
            integer(OP_SHR, U128(u128::MAX.to_le_bytes()), U64(200)),
            Ok(U128(0u128.to_le_bytes()))
        );
        // A u128 shift of exactly the width: a set bit overflows, zero does not.
        assert_eq!(
            integer(OP_SHL, U128(1u128.to_le_bytes()), U64(128)),
            Err(err(BallistaError::ArithmeticOverflow))
        );
        assert_eq!(
            integer(OP_SHL, U128(0u128.to_le_bytes()), U64(128)),
            Ok(U128(0u128.to_le_bytes()))
        );
        assert_eq!(
            integer(OP_SHL, U128(0u128.to_le_bytes()), U64(u64::MAX)),
            Ok(U128(0u128.to_le_bytes()))
        );
        assert_eq!(
            integer(OP_SHR, U128(0u128.to_le_bytes()), U64(u64::MAX)),
            Ok(U128(0u128.to_le_bytes()))
        );
        assert_eq!(integer(OP_SHL, I64(1), U64(1)), Err(err(BallistaError::TypeMismatch)));
        assert_eq!(integer(OP_SHL, U64(1), U128(1u128.to_le_bytes())), Err(err(BallistaError::TypeMismatch)));
    }

    #[test]
    fn bitwise_operations() {
        assert_eq!(integer(OP_BIT_AND, U64(0xabcd), U64(0xff00)), Ok(U64(0xab00)));
        assert_eq!(integer(OP_BIT_OR, U64(0xabcd), U64(0x000f)), Ok(U64(0xabcf)));
        assert_eq!(integer(OP_BIT_XOR, U64(0xabcd), U64(0xffff)), Ok(U64(0x5432)));
        assert_eq!(
            integer(OP_BIT_XOR, U128(u128::MAX.to_le_bytes()), U128(1u128.to_le_bytes())),
            Ok(U128((u128::MAX - 1).to_le_bytes()))
        );
        assert_eq!(integer(OP_BIT_AND, I64(1), I64(1)), Err(err(BallistaError::TypeMismatch)));
        assert_eq!(integer(OP_BIT_OR, Bool(true), Bool(true)), Err(err(BallistaError::TypeMismatch)));
    }

    #[test]
    fn integer_rejects_an_opcode_it_does_not_own() {
        // 51 and 52 are OP_MUL_DIV and OP_MUL_DIV_CEIL: real opcodes, but not `integer`'s to run.
        assert_eq!(integer(51, U64(1), U64(1)), Err(err(BallistaError::InvalidTemplateProgram)));
        assert_eq!(integer(52, U64(1), U64(1)), Err(err(BallistaError::InvalidTemplateProgram)));
        assert_eq!(integer(0, U64(1), U64(1)), Err(err(BallistaError::InvalidTemplateProgram)));
        assert_eq!(integer(255, U64(1), U64(1)), Err(err(BallistaError::InvalidTemplateProgram)));
    }

    #[test]
    fn powers_of_ten_up_to_ten_to_the_thirty_eighth() {
        for exponent in 0u64..=38 {
            assert_eq!(
                pow10(U64(exponent)),
                Ok(U128(10u128.pow(exponent as u32).to_le_bytes())),
                "10^{exponent}"
            );
        }
        assert_eq!(pow10(U64(39)), Err(err(BallistaError::ArithmeticOverflow)));
        assert_eq!(pow10(U64(u64::MAX)), Err(err(BallistaError::ArithmeticOverflow)));
        assert_eq!(pow10(I64(2)), Err(err(BallistaError::TypeMismatch)));
    }
}
