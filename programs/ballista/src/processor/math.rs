//! Integer operations beyond checked add, subtract, multiply and divide: multiply-then-divide
//! with an exact 256-bit product, remainder, shifts, bitwise operations and powers of ten. Each
//! returns the exact result or fails; none wraps, saturates or truncates silently.

use ballista_common::template::{OP_BIT_AND, OP_BIT_OR, OP_REM, OP_SHL, OP_SHR};

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
            let quotient = mul_div_u128(a as u128, b as u128, c as u128, round_up)?;
            u64::try_from(quotient)
                .map(RuntimeValue::U64)
                .map_err(|_| BallistaError::ArithmeticOverflow.into())
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

/// `a × b ÷ c` over `u128` with the product held in 256 bits.
pub fn mul_div_u128(a: u128, b: u128, c: u128, round_up: bool) -> RunResult<u128> {
    if c == 0 {
        return Err(BallistaError::DivisionByZero.into());
    }
    let (high, low) = widening_mul(a, b);
    let (quotient, remainder) = if high == 0 {
        (low / c, low % c)
    } else if high < c {
        divide_wide(high, low, c)
    } else {
        // The quotient needs more than 128 bits.
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

/// The 256-bit product of `a` and `b`, as its high and low 128-bit halves.
pub fn widening_mul(a: u128, b: u128) -> (u128, u128) {
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

/// `(high × 2^128 + low) ÷ divisor` and its remainder, given `high < divisor` so the quotient fits
/// 128 bits. Restoring long division, one quotient bit per step. It only runs when the product
/// does not fit 128 bits; smaller products take the native division in `mul_div_u128`.
fn divide_wide(mut high: u128, mut low: u128, divisor: u128) -> (u128, u128) {
    for _ in 0..128 {
        // The bit shifted out of `high` makes the running remainder 129 bits wide for a moment.
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

/// Remainder, shifts and bitwise operations: the two-operand integer opcodes that are not in
/// `execute::arithmetic`.
///
/// The `_` arm below sends every opcode other than [`OP_REM`], [`OP_SHL`] and [`OP_SHR`] to
/// [`bitwise`], and `bitwise` in turn treats anything that is not `OP_BIT_AND` or `OP_BIT_OR` as
/// `OP_BIT_XOR`. Neither match is exhaustive over `u8`. That is sound only because the dispatch
/// loop (`execute::execute_instruction`) calls `integer` exclusively for the six opcodes
/// `OP_REM | OP_SHL | OP_SHR | OP_BIT_AND | OP_BIT_OR | OP_BIT_XOR`, never for an arbitrary
/// opcode. Widening that call site to cover another opcode without also tightening the matches
/// here would silently route it to the wrong operation instead of failing.
pub fn integer<'data>(
    opcode: u8,
    left: RuntimeValue<'data>,
    right: RuntimeValue<'data>,
) -> RunResult<RuntimeValue<'data>> {
    match opcode {
        OP_REM => remainder(left, right),
        OP_SHL | OP_SHR => shift(opcode == OP_SHL, left, right),
        _ => bitwise(opcode, left, right),
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
                _ => $left ^ $right,
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
pub fn pow10<'data>(exponent: RuntimeValue<'data>) -> RunResult<RuntimeValue<'data>> {
    let RuntimeValue::U64(exponent) = exponent else {
        return Err(BallistaError::TypeMismatch.into());
    };
    POWERS_OF_TEN
        .get(exponent as usize)
        .filter(|_| exponent < POWERS_OF_TEN.len() as u64)
        .map(|value| RuntimeValue::U128(value.to_le_bytes()))
        .ok_or_else(|| BallistaError::ArithmeticOverflow.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ballista_common::template::{OP_BIT_XOR, OP_MUL_DIV};
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

    /// Shift-and-add multiplication: slow, obviously correct, and independent of `widening_mul`.
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
    fn widening_mul_matches_shift_and_add() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        for _ in 0..20_000 {
            let (a, b) = (rng.wide(), rng.wide());
            assert_eq!(widening_mul(a, b), slow_mul(a, b), "{a} × {b}");
        }
        assert_eq!(widening_mul(u128::MAX, u128::MAX), (u128::MAX - 1, 1));
        assert_eq!(widening_mul(u128::MAX, 1), (0, u128::MAX));
        assert_eq!(widening_mul(1 << 64, 1 << 64), (1, 0));
    }

    /// `q` and `r` are the floor quotient and remainder of `a × b ÷ c` exactly when
    /// `q × c + r = a × b` and `r < c`: division with remainder is unique.
    fn is_floor_division(a: u128, b: u128, c: u128, q: u128) -> bool {
        let (product_high, product_low) = widening_mul(a, b);
        let (qc_high, qc_low) = widening_mul(q, c);
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
            let (high, _) = widening_mul(a, b);
            match mul_div_u128(a, b, c, false) {
                Ok(q) => {
                    assert!(is_floor_division(a, b, c, q), "{a} × {b} ÷ {c} gave {q}");
                    let exact = is_floor_division(a, b, c, q) && {
                        let (qc_high, qc_low) = widening_mul(q, c);
                        (qc_high, qc_low) == widening_mul(a, b)
                    };
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
                }
            }
        }
        assert!(wide_cases > 1_000, "the 256-bit path was barely exercised: {wide_cases}");
    }

    #[test]
    fn mul_div_edges() {
        assert_eq!(mul_div_u128(1, 1, 0, false), Err(err(BallistaError::DivisionByZero)));
        assert_eq!(mul_div_u128(u128::MAX, u128::MAX, u128::MAX, false), Ok(u128::MAX));
        assert_eq!(mul_div_u128(u128::MAX, u128::MAX, u128::MAX, true), Ok(u128::MAX));
        assert_eq!(mul_div_u128(u128::MAX, 2, 1, false), Err(err(BallistaError::ArithmeticOverflow)));
        // (2^100 + 1)^2 ÷ 2^90 = 2^110 + 2^11 + 2^-90: rounds to 2^110 + 2048, or up by one.
        let a = (1u128 << 100) + 1;
        assert_eq!(mul_div_u128(a, a, 1 << 90, false), Ok((1 << 110) + 2048));
        assert_eq!(mul_div_u128(a, a, 1 << 90, true), Ok((1 << 110) + 2049));
        // A floor of u128::MAX with a remainder cannot round up.
        assert_eq!(
            mul_div_u128(u128::MAX, 3, 3, true),
            Ok(u128::MAX),
            "exact, so no rounding"
        );
        assert_eq!(
            mul_div_u128(u128::MAX, u128::MAX, u128::MAX - 1, false),
            Err(err(BallistaError::ArithmeticOverflow))
        );
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
        let a = (1u128 << 100) + 1;
        assert_eq!(
            mul_div(false, U128(a.to_le_bytes()), U128(a.to_le_bytes()), U128((1u128 << 90).to_le_bytes())),
            Ok(U128(((1u128 << 110) + 2048).to_le_bytes()))
        );
        assert_eq!(
            mul_div(false, U64(1), U128(1u128.to_le_bytes()), U64(1)),
            Err(err(BallistaError::TypeMismatch))
        );
        assert_eq!(mul_div(false, I64(1), I64(1), I64(1)), Err(err(BallistaError::TypeMismatch)));
        let _ = OP_MUL_DIV;
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
    fn powers_of_ten_up_to_ten_to_the_thirty_eighth() {
        assert_eq!(pow10(U64(0)), Ok(U128(1u128.to_le_bytes())));
        assert_eq!(pow10(U64(18)), Ok(U128(1_000_000_000_000_000_000u128.to_le_bytes())));
        assert_eq!(pow10(U64(38)), Ok(U128(10u128.pow(38).to_le_bytes())));
        assert_eq!(pow10(U64(39)), Err(err(BallistaError::ArithmeticOverflow)));
        assert_eq!(pow10(U64(u64::MAX)), Err(err(BallistaError::ArithmeticOverflow)));
        assert_eq!(pow10(I64(2)), Err(err(BallistaError::TypeMismatch)));
    }
}
