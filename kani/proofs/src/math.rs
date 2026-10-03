//! Arithmetic: `execute::arithmetic`, `compare`, `cast`, and `math::{integer, pow10}`
//! (`mul_div` is in `muldiv.rs`).
//!
//! Each spec is written independently of the code under test: in a wider integer type where one
//! exists (`i128` for `u64` and `i64`), or limb by limb for `u128`. Where a proof needs two
//! different wide multipliers or dividers, one on each side, the operands are bounded: their
//! symbolic bits sit in three windows (see `util::windowed_u64`/`windowed_u128`), and the harness
//! says how many bits that is.

use ballista::error::BallistaError;
use ballista::processor::execute::{arithmetic, cast, compare, RunError, RuntimeValue};
use ballista::processor::math::{integer, pow10};
use ballista_common::template::*;

use crate::util::{any_runtime_value, one_of, windowed_i64, windowed_u128, windowed_u64, U128Parts};

fn err(kind: BallistaError) -> RunError {
    RunError::Vm(kind)
}

fn u128_value(value: u128) -> RuntimeValue<'static> {
    RuntimeValue::U128(value.to_le_bytes())
}

fn as_u64(value: &RuntimeValue<'_>) -> Option<u64> {
    match value {
        RuntimeValue::U64(value) => Some(*value),
        _ => None,
    }
}

fn as_i64(value: &RuntimeValue<'_>) -> Option<i64> {
    match value {
        RuntimeValue::I64(value) => Some(*value),
        _ => None,
    }
}

fn as_u128(value: &RuntimeValue<'_>) -> Option<u128> {
    match value {
        RuntimeValue::U128(bytes) => Some(u128::from_le_bytes(*bytes)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// pow10
// ---------------------------------------------------------------------------------------------

/// `POW10` returns exactly `10^e` for every `u64` exponent up to 38 and `ArithmeticOverflow` for
/// every larger one; `10^38` is the largest power of ten a `u128` holds, so the table stops where
/// it must. Any operand that is not a `u64` is a `TypeMismatch`. Bound: none (every `u64`
/// exponent, every operand variant).
#[kani::proof]
#[kani::unwind(8)]
fn pow10_is_exact_up_to_38_and_overflows_above() {
    let exponent: u64 = kani::any();
    let result = pow10(RuntimeValue::U64(exponent));
    if exponent <= 38 {
        let value = result.as_ref().ok().and_then(as_u128);
        assert_eq!(value, 10u128.checked_pow(exponent as u32));
        kani::cover!(exponent == 38, "10^38");
    } else {
        assert_eq!(result, Err(err(BallistaError::ArithmeticOverflow)));
        kani::cover!(exponent == 39, "10^39 overflows");
    }
    assert!(10u128.checked_pow(38).is_some() && 10u128.checked_pow(39).is_none());

    let bytes: [u8; 4] = kani::any();
    let other = any_runtime_value(&bytes);
    if !matches!(other, RuntimeValue::U64(_)) {
        assert_eq!(pow10(other), Err(err(BallistaError::TypeMismatch)));
        kani::cover!(matches!(other, RuntimeValue::U128(_)), "a u128 exponent is refused");
    }
}

// ---------------------------------------------------------------------------------------------
// Checked add, subtract, multiply, divide, min, max
// ---------------------------------------------------------------------------------------------

/// `ADD`, `SUB`, `MIN`, `MAX` over `u64` return the exact result, computed in `i128`, or
/// `ArithmeticOverflow` exactly when it falls outside `u64`. Bound: none (every `u64` pair).
#[kani::proof]
fn u64_add_sub_min_max_are_exact() {
    let op = one_of(&[OP_ADD, OP_SUB, OP_MIN, OP_MAX]);
    let (a, b): (u64, u64) = (kani::any(), kani::any());
    let (wide_a, wide_b) = (a as i128, b as i128);
    let exact = match op {
        OP_ADD => wide_a + wide_b,
        OP_SUB => wide_a - wide_b,
        OP_MIN => wide_a.min(wide_b),
        _ => wide_a.max(wide_b),
    };
    let fits = 0 <= exact && exact <= u64::MAX as i128;
    let result = arithmetic(op, RuntimeValue::U64(a), RuntimeValue::U64(b));
    if fits {
        assert_eq!(result.as_ref().ok().and_then(as_u64).map(|r| r as i128), Some(exact));
        kani::cover!(op == OP_ADD && exact == u64::MAX as i128, "an add landing on u64::MAX");
    } else {
        assert_eq!(result, Err(err(BallistaError::ArithmeticOverflow)));
        kani::cover!(op == OP_SUB, "a subtraction below zero");
        kani::cover!(op == OP_ADD, "an addition past u64::MAX");
    }
}

/// `ADD`, `SUB`, `MIN`, `MAX` over `i64`, as for `u64`, against the exact `i128` result. Bound:
/// none (every `i64` pair).
#[kani::proof]
fn i64_add_sub_min_max_are_exact() {
    let op = one_of(&[OP_ADD, OP_SUB, OP_MIN, OP_MAX]);
    let (a, b): (i64, i64) = (kani::any(), kani::any());
    let (wide_a, wide_b) = (a as i128, b as i128);
    let exact = match op {
        OP_ADD => wide_a + wide_b,
        OP_SUB => wide_a - wide_b,
        OP_MIN => wide_a.min(wide_b),
        _ => wide_a.max(wide_b),
    };
    let fits = i64::MIN as i128 <= exact && exact <= i64::MAX as i128;
    let result = arithmetic(op, RuntimeValue::I64(a), RuntimeValue::I64(b));
    if fits {
        assert_eq!(result.as_ref().ok().and_then(as_i64).map(|r| r as i128), Some(exact));
        kani::cover!(op == OP_SUB && exact == i64::MIN as i128, "a subtraction landing on i64::MIN");
    } else {
        assert_eq!(result, Err(err(BallistaError::ArithmeticOverflow)));
        kani::cover!(exact < i64::MIN as i128, "an underflow below i64::MIN");
        kani::cover!(exact > i64::MAX as i128, "an overflow past i64::MAX");
    }
}

/// `ADD`, `SUB`, `MIN`, `MAX` over `u128`, against a spec computed on 64-bit limbs with explicit
/// carries, so it shares no 128-bit operator with the code under test. Bound: none (every `u128`
/// pair).
#[kani::proof]
fn u128_add_sub_min_max_are_exact() {
    let op = one_of(&[OP_ADD, OP_SUB, OP_MIN, OP_MAX]);
    let (a, b): (u128, u128) = (kani::any(), kani::any());
    let (pa, pb) = (U128Parts::of(a), U128Parts::of(b));
    // `Some(value)` when the exact result fits 128 bits.
    let exact: Option<u128> = match op {
        OP_ADD => pa.add(pb),
        OP_SUB => pa.sub(pb),
        OP_MIN => Some(if pa.less_than(pb) { a } else { b }),
        _ => Some(if pa.less_than(pb) { b } else { a }),
    };
    let result = arithmetic(op, u128_value(a), u128_value(b));
    match exact {
        Some(value) => {
            assert_eq!(result.as_ref().ok().and_then(as_u128), Some(value));
            kani::cover!(op == OP_ADD && pa.low.checked_add(pb.low).is_none(), "an add carrying across the limbs");
        }
        None => {
            assert_eq!(result, Err(err(BallistaError::ArithmeticOverflow)));
            kani::cover!(op == OP_SUB, "a u128 subtraction below zero");
        }
    }
}

/// `DIV` and `REM` over `u64`, `i64` and `u128` fail exactly on the documented inputs and with the
/// documented error: `DivisionByZero` exactly when the divisor is zero, `ArithmeticOverflow`
/// exactly for the `i64` minimum over −1, and success, in the operands' type, on every other pair.
/// (The values they return are the subject of the `*_div_*` and `*_remainder_*` harnesses.) Bound:
/// none (every pair of every type).
#[kani::proof]
fn division_and_remainder_fail_exactly_on_the_documented_inputs() {
    let op = one_of(&[OP_DIV, OP_REM]);
    let run = |left: RuntimeValue<'static>, right: RuntimeValue<'static>| {
        if op == OP_REM { integer(op, left, right) } else { arithmetic(op, left, right) }
    };
    let kind: u8 = kani::any_where(|kind: &u8| *kind < 3);
    let (outcome, zero, overflow, typed) = match kind {
        0 => {
            let (a, b): (u64, u64) = (kani::any(), kani::any());
            let outcome = run(RuntimeValue::U64(a), RuntimeValue::U64(b));
            let typed = outcome.as_ref().map_or(true, |value| as_u64(value).is_some());
            (outcome, b == 0, false, typed)
        }
        1 => {
            let (a, b): (i64, i64) = (kani::any(), kani::any());
            let outcome = run(RuntimeValue::I64(a), RuntimeValue::I64(b));
            let typed = outcome.as_ref().map_or(true, |value| as_i64(value).is_some());
            (outcome, b == 0, a == i64::MIN && b == -1, typed)
        }
        _ => {
            let (a, b): (u128, u128) = (kani::any(), kani::any());
            let outcome = run(u128_value(a), u128_value(b));
            let typed = outcome.as_ref().map_or(true, |value| as_u128(value).is_some());
            (outcome, b == 0, false, typed)
        }
    };
    assert!(typed);
    if zero {
        assert_eq!(outcome, Err(err(BallistaError::DivisionByZero)));
        kani::cover!(kind == 2 && op == OP_REM, "a u128 remainder by zero");
    } else if overflow {
        assert_eq!(outcome, Err(err(BallistaError::ArithmeticOverflow)));
        kani::cover!(op == OP_REM, "i64::MIN % -1");
    } else {
        assert!(outcome.is_ok());
        kani::cover!(kind == 1 && op == OP_DIV, "a signed division");
    }
}

/// Width of each symbolic window in a bounded `u64` or `i64` operand (see `util::windowed_u64`).
const U64_BITS: u32 = 8;

/// Width of each symbolic window in a bounded `u64` or `i64` division or remainder (see
/// `util::windowed_u64`).
const U64_DIVISION_BITS: u32 = 4;

/// `MUL` over `u64` returns the exact product, computed in `u128`, or `ArithmeticOverflow` exactly
/// when it exceeds `u64::MAX`. Bound: each operand has 24 symbolic bits, three 8-bit windows (bits
/// 0–7, 28–35, 56–63) with zeros between, so products still reach every bit and overflow; a
/// full-width run compares two 64-bit multipliers and did not finish in 15 minutes.
#[kani::proof]
fn u64_mul_is_exact() {
    let (a, b) = (windowed_u64(U64_BITS), windowed_u64(U64_BITS));
    let exact = a as u128 * b as u128;
    let result = arithmetic(OP_MUL, RuntimeValue::U64(a), RuntimeValue::U64(b));
    if exact <= u64::MAX as u128 {
        assert_eq!(result.as_ref().ok().and_then(as_u64).map(|r| r as u128), Some(exact));
        kani::cover!(exact >> 56 != 0, "a product reaching the top byte");
    } else {
        assert_eq!(result, Err(err(BallistaError::ArithmeticOverflow)));
        kani::cover!(exact >> 64 == 1, "a product just past u64");
    }
}

/// `MUL` over `i64` returns the exact product, computed in `i128`, or `ArithmeticOverflow` exactly
/// when it falls outside `i64`. Bound: each operand is a windowed `u64` (24 symbolic bits, as in
/// `u64_mul_is_exact`) or its bitwise complement, as an `i64`, so both signs, values near zero on
/// either side, `i64::MIN` and `-1` are all reachable.
#[kani::proof]
fn i64_mul_is_exact() {
    let (a, b) = (windowed_i64(U64_BITS), windowed_i64(U64_BITS));
    let exact = a as i128 * b as i128;
    let result = arithmetic(OP_MUL, RuntimeValue::I64(a), RuntimeValue::I64(b));
    if i64::MIN as i128 <= exact && exact <= i64::MAX as i128 {
        assert_eq!(result.as_ref().ok().and_then(as_i64).map(|r| r as i128), Some(exact));
        kani::cover!(exact < 0, "a negative product");
    } else {
        assert_eq!(result, Err(err(BallistaError::ArithmeticOverflow)));
        kani::cover!(a == i64::MIN && b == -1, "i64::MIN * -1");
    }
}

/// `DIV` over `u64`: `DivisionByZero` exactly when the divisor is zero, and otherwise the floor
/// quotient, checked by the defining inequality `q·b ≤ a < (q + 1)·b` in `u128`. Bound: each
/// operand has 12 symbolic bits, three 4-bit windows (bits 0–3, 30–33, 60–63) with zeros between.
/// The check compares the code's divider with a multiplier: at full width it did not finish in 20
/// minutes, nor with 24 symbolic bits in 15. Which inputs fail, and how, is proved at full width by
/// `division_and_remainder_fail_exactly_on_the_documented_inputs`.
#[kani::proof]
fn u64_div_is_the_floor_quotient() {
    let (a, b) = (windowed_u64(U64_DIVISION_BITS), windowed_u64(U64_DIVISION_BITS));
    let result = arithmetic(OP_DIV, RuntimeValue::U64(a), RuntimeValue::U64(b));
    assert!(matches!(result, Ok(RuntimeValue::U64(_)) | Err(_)));
    match result {
        Ok(RuntimeValue::U64(q)) => {
            assert!(b != 0);
            let (q, a, b) = (q as u128, a as u128, b as u128);
            assert!(q * b <= a && a < q * b + b);
            kani::cover!(q > 1 && a != q * b, "an inexact quotient");
        }
        Err(error) => {
            assert_eq!(error, err(BallistaError::DivisionByZero));
            assert!(b == 0);
        }
        Ok(_) => {}
    }
}

/// `DIV` over `i64`: `DivisionByZero` exactly when the divisor is zero, `ArithmeticOverflow`
/// exactly for `i64::MIN / -1`, and otherwise the quotient truncated toward zero: its magnitude is
/// `|a| / |b|` in `u64` (whose floor property `u64_div_is_the_floor_quotient` proves), and it is
/// negative exactly when it is nonzero and the signs differ. Bound: each operand is a windowed
/// `u64` (12 symbolic bits, as in `u64_div_is_the_floor_quotient`) or its bitwise complement, as an
/// `i64`, so both signs, `i64::MIN` and `-1` are reachable.
#[kani::proof]
fn i64_div_truncates_toward_zero() {
    let (a, b) = (windowed_i64(U64_DIVISION_BITS), windowed_i64(U64_DIVISION_BITS));
    let result = arithmetic(OP_DIV, RuntimeValue::I64(a), RuntimeValue::I64(b));
    assert!(matches!(result, Ok(RuntimeValue::I64(_)) | Err(_)));
    match result {
        Ok(RuntimeValue::I64(q)) => {
            assert!(b != 0 && !(a == i64::MIN && b == -1));
            assert_eq!(q.unsigned_abs(), a.unsigned_abs() / b.unsigned_abs());
            assert_eq!(q < 0, q != 0 && ((a < 0) != (b < 0)));
            kani::cover!(q < 0, "a negative quotient");
        }
        Err(error) if b == 0 => assert_eq!(error, err(BallistaError::DivisionByZero)),
        Err(error) => {
            assert_eq!(error, err(BallistaError::ArithmeticOverflow));
            assert!(a == i64::MIN && b == -1);
        }
        Ok(_) => {}
    }
}

/// Width of each symbolic window in a bounded `u128` multiply (see `util::windowed_u128`).
const U128_BITS: u32 = 8;

/// `MUL` over `u128`: the exact product or `ArithmeticOverflow`, against a schoolbook product on
/// 32-bit digits. Bound: each operand has 24 symbolic bits, three 8-bit windows (bits 0–7, 60–67,
/// 120–127) with zeros between.
#[kani::proof]
#[kani::unwind(8)]
fn u128_mul_is_exact() {
    let (a, b) = (windowed_u128(U128_BITS), windowed_u128(U128_BITS));
    let (high, low) = U128Parts::of(a).widening_mul(U128Parts::of(b));
    let result = arithmetic(OP_MUL, u128_value(a), u128_value(b));
    if high == 0 {
        assert_eq!(result.as_ref().ok().and_then(as_u128), Some(low));
        kani::cover!(low >> 64 != 0, "a product past 64 bits");
    } else {
        assert_eq!(result, Err(err(BallistaError::ArithmeticOverflow)));
        kani::cover!(high == 1, "a product just past u128");
    }
}

/// Window width for the `u128` divide and remainder proofs: their quotient is fully symbolic, so
/// the solver's divider is larger than a multiplier, and the windows are narrower.
const U128_DIVISION_BITS: u32 = 4;

/// `DIV` over `u128`: `DivisionByZero` exactly when the divisor is zero, and otherwise the floor
/// quotient, checked by `q·b ≤ a < (q + 1)·b` with limb-wise products. Bound: each operand has 12
/// symbolic bits, three 4-bit windows (bits 0–3, 62–65, 124–127).
#[kani::proof]
#[kani::unwind(8)]
fn u128_div_is_the_floor_quotient() {
    let (a, b) = (windowed_u128(U128_DIVISION_BITS), windowed_u128(U128_DIVISION_BITS));
    let result = arithmetic(OP_DIV, u128_value(a), u128_value(b));
    if b == 0 {
        assert_eq!(result, Err(err(BallistaError::DivisionByZero)));
    } else {
        let q = result.as_ref().ok().and_then(as_u128).expect("a u128 quotient");
        let (high, qb) = U128Parts::of(q).widening_mul(U128Parts::of(b));
        assert!(high == 0 && qb <= a && a - qb < b);
        kani::cover!(q >> 64 != 0 && a != qb, "an inexact quotient past 64 bits");
    }
}

/// Arithmetic on two values that are not both `u64`, both `i64` or both `u128` is a
/// `TypeMismatch`, for every arithmetic opcode; an opcode outside the six falls through as
/// `ArithmeticOverflow` (the executor never routes one here). Bound: none over variants and
/// payloads (`bytes` values up to 4 bytes).
#[kani::proof]
#[kani::unwind(5)]
fn arithmetic_rejects_mismatched_operands() {
    let op = one_of(&[OP_ADD, OP_SUB, OP_MUL, OP_DIV, OP_MIN, OP_MAX]);
    let (left_bytes, right_bytes): ([u8; 4], [u8; 4]) = (kani::any(), kani::any());
    let left = any_runtime_value(&left_bytes);
    let right = any_runtime_value(&right_bytes);
    let same_numeric = matches!(
        (left, right),
        (RuntimeValue::U64(_), RuntimeValue::U64(_))
            | (RuntimeValue::I64(_), RuntimeValue::I64(_))
            | (RuntimeValue::U128(_), RuntimeValue::U128(_))
    );
    if !same_numeric {
        assert_eq!(arithmetic(op, left, right), Err(err(BallistaError::TypeMismatch)));
        kani::cover!(matches!((left, right), (RuntimeValue::U64(_), RuntimeValue::I64(_))), "u64 with i64");
        kani::cover!(matches!(left, RuntimeValue::Unset), "an unset operand");
    }
    let foreign: u8 = kani::any();
    kani::assume(!matches!(foreign, OP_ADD | OP_SUB | OP_MUL | OP_DIV | OP_MIN | OP_MAX));
    let (a, b): (u64, u64) = (kani::any(), kani::any());
    assert_eq!(
        arithmetic(foreign, RuntimeValue::U64(a), RuntimeValue::U64(b)),
        Err(err(BallistaError::ArithmeticOverflow))
    );
}

// ---------------------------------------------------------------------------------------------
// Comparisons and casts
// ---------------------------------------------------------------------------------------------

/// The six comparisons order `u64`, `i64` and `u128` numerically; `u128`s are compared limb by
/// limb in the spec, not through `u128`'s own ordering. Bound: none (every pair of every type).
#[kani::proof]
fn numeric_comparisons_match_the_numbers() {
    let op = one_of(&[OP_EQ, OP_NE, OP_LT, OP_LTE, OP_GT, OP_GTE]);
    let expect = |less: bool, equal: bool| match op {
        OP_EQ => equal,
        OP_NE => !equal,
        OP_LT => less,
        OP_LTE => less || equal,
        OP_GT => !less && !equal,
        _ => !less,
    };
    let kind: u8 = kani::any_where(|kind: &u8| *kind < 3);
    let result = match kind {
        0 => {
            let (a, b): (u64, u64) = (kani::any(), kani::any());
            let expected = expect((a as i128) < (b as i128), a == b);
            assert_eq!(compare(op, RuntimeValue::U64(a), RuntimeValue::U64(b)), Ok(expected));
            expected
        }
        1 => {
            let (a, b): (i64, i64) = (kani::any(), kani::any());
            let expected = expect((a as i128) < (b as i128), a == b);
            assert_eq!(compare(op, RuntimeValue::I64(a), RuntimeValue::I64(b)), Ok(expected));
            kani::cover!(a < 0 && b >= 0 && expected, "a negative compared with a non-negative");
            expected
        }
        _ => {
            let (a, b): (u128, u128) = (kani::any(), kani::any());
            let (pa, pb) = (U128Parts::of(a), U128Parts::of(b));
            let expected = expect(pa.less_than(pb), pa == pb);
            assert_eq!(compare(op, u128_value(a), u128_value(b)), Ok(expected));
            kani::cover!(pa.high == pb.high && pa.low != pb.low, "u128s differing only in the low limb");
            expected
        }
    };
    kani::cover!(result, "a comparison that holds");
    kani::cover!(!result, "a comparison that fails");
}

/// Equality on `bool`, `pubkey` and `bytes` is value equality (`bytes` compare by length and
/// content), the four orderings on them are a `TypeMismatch`, so is any comparison of two
/// different types, and an opcode outside the six fails on every pair. Bound: `bytes` values up to
/// 3 bytes; every other payload unbounded.
#[kani::proof]
#[kani::unwind(33)]
fn equality_only_comparisons_and_type_rules() {
    let op = one_of(&[OP_EQ, OP_NE, OP_LT, OP_LTE, OP_GT, OP_GTE]);
    let (left_bytes, right_bytes): ([u8; 3], [u8; 3]) = (kani::any(), kani::any());
    let left = any_runtime_value(&left_bytes);
    let right = any_runtime_value(&right_bytes);
    let foreign: u8 = kani::any();
    kani::assume(!matches!(foreign, OP_EQ | OP_NE | OP_LT | OP_LTE | OP_GT | OP_GTE));
    assert!(compare(foreign, left, right).is_err());

    let result = compare(op, left, right);
    let ordering = matches!(op, OP_LT | OP_LTE | OP_GT | OP_GTE);
    let equal = match (left, right) {
        (RuntimeValue::Bool(a), RuntimeValue::Bool(b)) => Some(a == b),
        (RuntimeValue::Pubkey(a), RuntimeValue::Pubkey(b)) => Some(a == b),
        (RuntimeValue::Bytes(a), RuntimeValue::Bytes(b)) => {
            Some(a.len() == b.len() && (0..a.len()).all(|i| a[i] == b[i]))
        }
        _ => None,
    };
    let numeric_pair = matches!(
        (left, right),
        (RuntimeValue::U64(_), RuntimeValue::U64(_))
            | (RuntimeValue::I64(_), RuntimeValue::I64(_))
            | (RuntimeValue::U128(_), RuntimeValue::U128(_))
    );
    match equal {
        Some(_) if ordering => {
            assert_eq!(result, Err(err(BallistaError::TypeMismatch)));
            kani::cover!(matches!(left, RuntimeValue::Pubkey(_)), "ordering pubkeys is refused");
        }
        Some(equal) => {
            assert_eq!(result, Ok(if op == OP_EQ { equal } else { !equal }));
            kani::cover!(matches!(left, RuntimeValue::Bytes(a) if a.len() == 3) && equal, "equal 3-byte values");
            kani::cover!(
                matches!((left, right), (RuntimeValue::Bytes(a), RuntimeValue::Bytes(b)) if a.len() != b.len()),
                "bytes of different lengths"
            );
        }
        None if !numeric_pair => {
            assert_eq!(result, Err(err(BallistaError::TypeMismatch)));
            kani::cover!(matches!((left, right), (RuntimeValue::Bool(_), RuntimeValue::U64(_))), "bool with u64");
        }
        None => {}
    }
}

/// `CAST_U64`, `CAST_I64`, `CAST_U128` succeed exactly when the value fits the target, keep the
/// value unchanged (compared through a sign and two 64-bit limbs), report `ArithmeticOverflow`
/// otherwise, and reject non-numeric sources with `TypeMismatch`. Bound: none (every value of
/// every numeric type; other variants with any payload).
#[kani::proof]
#[kani::unwind(3)]
fn casts_preserve_the_value_or_fail() {
    let op = one_of(&[OP_CAST_U64, OP_CAST_I64, OP_CAST_U128]);
    let kind: u8 = kani::any_where(|kind: &u8| *kind < 3);
    // The source as (sign, high limb, low limb): one representation for all three types.
    let (source, negative, high, low) = match kind {
        0 => {
            let value: u64 = kani::any();
            (RuntimeValue::U64(value), false, 0u64, value)
        }
        1 => {
            let value: i64 = kani::any();
            (RuntimeValue::I64(value), value < 0, 0, value.unsigned_abs())
        }
        _ => {
            let value: u128 = kani::any();
            let parts = U128Parts::of(value);
            (u128_value(value), false, parts.high, parts.low)
        }
    };
    let result = cast(op, source);
    match op {
        OP_CAST_U64 => {
            if !negative && high == 0 {
                assert_eq!(result, Ok(RuntimeValue::U64(low)));
                kani::cover!(kind == 2 && low == u64::MAX, "u64::MAX from a u128");
            } else {
                assert_eq!(result, Err(err(BallistaError::ArithmeticOverflow)));
                kani::cover!(negative, "a negative cast to u64");
            }
        }
        OP_CAST_I64 => {
            let fits = high == 0 && (low <= i64::MAX as u64 || (negative && low == 1 << 63));
            if fits {
                let value = if negative { (low as i128).wrapping_neg() } else { low as i128 };
                assert_eq!(result, Ok(RuntimeValue::I64(value as i64)));
                assert_eq!(value as i64 as i128, value);
                kani::cover!(kind == 2 && low == i64::MAX as u64, "i64::MAX from a u128");
            } else {
                assert_eq!(result, Err(err(BallistaError::ArithmeticOverflow)));
                kani::cover!(kind == 0, "a u64 past i64::MAX");
            }
        }
        _ => {
            if negative {
                assert_eq!(result, Err(err(BallistaError::ArithmeticOverflow)));
                kani::cover!(low == 1, "-1 cast to u128");
            } else {
                let value = result.as_ref().ok().and_then(as_u128).expect("a u128");
                assert_eq!(U128Parts::of(value), U128Parts { high, low });
            }
        }
    }
    let bytes: [u8; 2] = kani::any();
    let other = any_runtime_value(&bytes);
    if !matches!(other, RuntimeValue::U64(_) | RuntimeValue::I64(_) | RuntimeValue::U128(_)) {
        assert_eq!(cast(op, other), Err(err(BallistaError::TypeMismatch)));
    }
}

// ---------------------------------------------------------------------------------------------
// Remainder, shifts, bitwise
// ---------------------------------------------------------------------------------------------

/// `REM` over `u64`: `DivisionByZero` exactly for a zero divisor, otherwise the remainder `r`:
/// `r < b`, `r ≤ a`, and `a − r` a multiple of `b`. Bound: each operand has 12 symbolic bits, as
/// in `u64_div_is_the_floor_quotient`.
#[kani::proof]
fn u64_remainder_is_exact() {
    let (a, b) = (windowed_u64(U64_DIVISION_BITS), windowed_u64(U64_DIVISION_BITS));
    let result = integer(OP_REM, RuntimeValue::U64(a), RuntimeValue::U64(b));
    assert!(matches!(result, Ok(RuntimeValue::U64(_)) | Err(_)));
    match result {
        Ok(RuntimeValue::U64(r)) => {
            assert!(b != 0 && r < b && r <= a);
            assert_eq!((a - r) % b, 0);
            kani::cover!(r != 0, "a nonzero remainder");
        }
        Err(error) => {
            assert_eq!(error, err(BallistaError::DivisionByZero));
            assert!(b == 0);
        }
        Ok(_) => {}
    }
}

/// `REM` over `i64`: `DivisionByZero` for a zero divisor, `ArithmeticOverflow` exactly for
/// `i64::MIN % -1` (documented in docs/reference/language.md), and otherwise the remainder that
/// takes the dividend's sign: `|r| < |b|`, `r` zero or signed as `a`, and `b | a − r`, in `i128`.
/// Bound: each operand as in `i64_div_truncates_toward_zero`.
#[kani::proof]
fn i64_remainder_takes_the_dividends_sign() {
    let (a, b) = (windowed_i64(U64_DIVISION_BITS), windowed_i64(U64_DIVISION_BITS));
    let result = integer(OP_REM, RuntimeValue::I64(a), RuntimeValue::I64(b));
    assert!(matches!(result, Ok(RuntimeValue::I64(_)) | Err(_)));
    match result {
        Ok(RuntimeValue::I64(r)) => {
            assert!(b != 0 && !(a == i64::MIN && b == -1));
            let (a, b, r) = (a as i128, b as i128, r as i128);
            assert!(r.abs() < b.abs());
            assert!(r == 0 || (r < 0) == (a < 0));
            assert_eq!((a - r) % b, 0);
            kani::cover!(r < 0, "a negative remainder");
        }
        Err(error) if b == 0 => assert_eq!(error, err(BallistaError::DivisionByZero)),
        Err(error) => {
            assert_eq!(error, err(BallistaError::ArithmeticOverflow));
            assert!(a == i64::MIN && b == -1);
        }
        Ok(_) => {}
    }
}

/// `REM` over `u128`, as for `u64`: `r < b`, `r ≤ a`, and `(a / b)·b + r = a` with the product
/// taken by limbs. Bound: each operand has 12 symbolic bits, as in
/// `u128_div_is_the_floor_quotient`.
#[kani::proof]
#[kani::unwind(8)]
fn u128_remainder_is_exact() {
    let (a, b) = (windowed_u128(U128_DIVISION_BITS), windowed_u128(U128_DIVISION_BITS));
    let result = integer(OP_REM, u128_value(a), u128_value(b));
    if b == 0 {
        assert_eq!(result, Err(err(BallistaError::DivisionByZero)));
    } else {
        let r = result.as_ref().ok().and_then(as_u128).expect("a u128 remainder");
        assert!(r < b && r <= a);
        let (high, qb) = U128Parts::of(a / b).widening_mul(U128Parts::of(b));
        assert!(high == 0 && qb + r == a);
        kani::cover!(r >> 64 != 0, "a remainder past 64 bits");
    }
}

/// `SHL` over `u64` and `u128` fails with `ArithmeticOverflow` exactly when a set bit would be
/// shifted out (for any shift amount, including the full width and beyond), and otherwise returns
/// `a · 2^n`; `SHR` never fails and returns `⌊a / 2^n⌋`, 0 for a shift of the full width or more.
/// The spec states both through masks (`a < 2^(w − n)`, the low `n` bits of the result clear)
/// rather than by shifting back as the code does. Bound: none (every value, every `u64` shift).
#[kani::proof]
fn shifts_never_drop_a_set_bit_silently() {
    let left = kani::any::<bool>();
    let opcode = if left { OP_SHL } else { OP_SHR };
    let bits: u64 = kani::any();
    if kani::any() {
        let value: u64 = kani::any();
        let result = integer(opcode, RuntimeValue::U64(value), RuntimeValue::U64(bits));
        // value · 2^n fits when value < 2^(64 − n), i.e. nothing above bit 63 − n is set.
        let fits = value == 0 || (bits < 64 && (bits == 0 || value >> (64 - bits) == 0));
        let low_mask = if bits < 64 { (1u64 << bits) - 1 } else { u64::MAX };
        if left && !fits {
            assert_eq!(result, Err(err(BallistaError::ArithmeticOverflow)));
            kani::cover!(bits < 64, "a shift that drops a set bit");
        } else {
            let r = result.as_ref().ok().and_then(as_u64).expect("a u64 result");
            if left {
                // r = value · 2^n: its low n bits are clear and the rest is value.
                assert_eq!(r & low_mask, 0);
                assert_eq!(if bits < 64 { r >> bits } else { 0 }, if bits < 64 { value } else { 0 });
                kani::cover!(bits == 63 && r != 0, "a shift to the top bit");
            } else if bits < 64 {
                // value = r · 2^n + (value mod 2^n), with r below 2^(64 − n): r is the floor.
                assert_eq!((r << bits) | (value & low_mask), value);
                assert!(bits == 0 || r >> (64 - bits) == 0);
            } else {
                assert_eq!(r, 0);
                kani::cover!(value != 0, "a full-width right shift clears a value");
            }
        }
    } else {
        let value: u128 = kani::any();
        let result = integer(opcode, u128_value(value), RuntimeValue::U64(bits));
        let fits = value == 0 || (bits < 128 && (bits == 0 || value >> (128 - bits) == 0));
        let low_mask = if bits < 128 { (1u128 << bits) - 1 } else { u128::MAX };
        if left && !fits {
            assert_eq!(result, Err(err(BallistaError::ArithmeticOverflow)));
            kani::cover!(bits >= 128, "a u128 shift past the width");
        } else {
            let r = result.as_ref().ok().and_then(as_u128).expect("a u128 result");
            if left {
                assert_eq!(r & low_mask, 0);
                assert_eq!(if bits < 128 { r >> bits } else { 0 }, if bits < 128 { value } else { 0 });
                kani::cover!(bits > 64 && r != 0, "a u128 shift across the limbs");
            } else if bits < 128 {
                assert_eq!((r << bits) | (value & low_mask), value);
                assert!(bits == 0 || r >> (128 - bits) == 0);
            } else {
                assert_eq!(r, 0);
            }
        }
    }
}

/// `BIT_AND`, `BIT_OR`, `BIT_XOR` over matching `u64` or `u128` are the bitwise operations, limb
/// by limb for `u128`; `REM`, the shifts and the bitwise opcodes reject every other operand pair
/// with `TypeMismatch` (a shift amount must be a `u64`, its value a `u64` or `u128`); any opcode
/// outside the six is `InvalidTemplateProgram`. Bound: none (every value; other variants with any
/// payload, `bytes` up to 2 bytes).
#[kani::proof]
#[kani::unwind(3)]
fn bitwise_operations_and_integer_type_rules() {
    let op = one_of(&[OP_BIT_AND, OP_BIT_OR, OP_BIT_XOR]);
    let apply = |a: u64, b: u64| match op {
        OP_BIT_AND => a & b,
        OP_BIT_OR => a | b,
        _ => a ^ b,
    };
    let (a, b): (u128, u128) = (kani::any(), kani::any());
    let (pa, pb) = (U128Parts::of(a), U128Parts::of(b));
    let wide = integer(op, u128_value(a), u128_value(b));
    let value = wide.as_ref().ok().and_then(as_u128).expect("a u128 result");
    assert_eq!(U128Parts::of(value), U128Parts { high: apply(pa.high, pb.high), low: apply(pa.low, pb.low) });
    let narrow = integer(op, RuntimeValue::U64(pa.low), RuntimeValue::U64(pb.low));
    assert_eq!(narrow.as_ref().ok().and_then(as_u64), Some(apply(pa.low, pb.low)));

    let any_op = one_of(&[OP_REM, OP_SHL, OP_SHR, OP_BIT_AND, OP_BIT_OR, OP_BIT_XOR]);
    let (left_bytes, right_bytes): ([u8; 2], [u8; 2]) = (kani::any(), kani::any());
    let left = any_runtime_value(&left_bytes);
    let right = any_runtime_value(&right_bytes);
    let accepted = match any_op {
        OP_SHL | OP_SHR => {
            matches!(left, RuntimeValue::U64(_) | RuntimeValue::U128(_))
                && matches!(right, RuntimeValue::U64(_))
        }
        OP_REM => matches!(
            (left, right),
            (RuntimeValue::U64(_), RuntimeValue::U64(_))
                | (RuntimeValue::I64(_), RuntimeValue::I64(_))
                | (RuntimeValue::U128(_), RuntimeValue::U128(_))
        ),
        _ => matches!(
            (left, right),
            (RuntimeValue::U64(_), RuntimeValue::U64(_))
                | (RuntimeValue::U128(_), RuntimeValue::U128(_))
        ),
    };
    if !accepted {
        assert_eq!(integer(any_op, left, right), Err(err(BallistaError::TypeMismatch)));
        kani::cover!(any_op == OP_SHL && matches!(right, RuntimeValue::U128(_)), "a u128 shift amount");
        kani::cover!(any_op == OP_BIT_AND && matches!(left, RuntimeValue::I64(_)), "bitwise on i64");
    }
    let foreign: u8 = kani::any();
    kani::assume(!matches!(foreign, OP_REM | OP_SHL | OP_SHR | OP_BIT_AND | OP_BIT_OR | OP_BIT_XOR));
    assert_eq!(integer(foreign, left, right), Err(err(BallistaError::InvalidTemplateProgram)));
}
