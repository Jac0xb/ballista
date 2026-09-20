//! Checked arithmetic, comparisons, and casts behave exactly as specified for every input.

use ballista::error::BallistaError;
use ballista::processor::execute::{arithmetic, cast, compare, RunError, RuntimeValue};
use ballista_common::template::*;
use cvlr::prelude::*;

use super::util::pick;

const ARITHMETIC: [u8; 6] = [OP_ADD, OP_SUB, OP_MUL, OP_DIV, OP_MIN, OP_MAX];
const ORDERED: [u8; 6] = [OP_EQ, OP_NE, OP_LT, OP_LTE, OP_GT, OP_GTE];

/// The result Rust's checked operators give, or `None` on overflow or division by zero.
macro_rules! checked_result {
    ($op:expr, $a:expr, $b:expr) => {
        match $op {
            OP_ADD => $a.checked_add($b),
            OP_SUB => $a.checked_sub($b),
            OP_MUL => $a.checked_mul($b),
            OP_DIV => $a.checked_div($b),
            OP_MIN => Some($a.min($b)),
            _ => Some($a.max($b)),
        }
    };
}

#[rule]
pub fn rule_u64_arithmetic_is_checked() {
    let op = pick(&ARITHMETIC);
    let a: u64 = nondet();
    let b: u64 = nondet();
    let expected = checked_result!(op, a, b);
    clog!(op, a, b);
    match arithmetic(op, RuntimeValue::U64(a), RuntimeValue::U64(b)) {
        Ok(RuntimeValue::U64(result)) => cvlr_assert!(expected == Some(result)),
        Ok(_) => cvlr_assert!(false),
        Err(RunError::Vm(BallistaError::DivisionByZero)) => cvlr_assert!(op == OP_DIV && b == 0),
        Err(RunError::Vm(BallistaError::ArithmeticOverflow)) => {
            cvlr_assert!(expected.is_none() && !(op == OP_DIV && b == 0))
        }
        Err(_) => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_i64_arithmetic_is_checked() {
    let op = pick(&ARITHMETIC);
    let a: i64 = nondet();
    let b: i64 = nondet();
    let expected = checked_result!(op, a, b);
    clog!(op, a, b);
    match arithmetic(op, RuntimeValue::I64(a), RuntimeValue::I64(b)) {
        Ok(RuntimeValue::I64(result)) => cvlr_assert!(expected == Some(result)),
        Ok(_) => cvlr_assert!(false),
        Err(RunError::Vm(BallistaError::DivisionByZero)) => cvlr_assert!(op == OP_DIV && b == 0),
        Err(RunError::Vm(BallistaError::ArithmeticOverflow)) => {
            // Includes i64::MIN / -1, which Rust reports as overflow rather than division by zero.
            cvlr_assert!(expected.is_none() && !(op == OP_DIV && b == 0))
        }
        Err(_) => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_u128_arithmetic_is_checked() {
    let op = pick(&ARITHMETIC);
    let a: u128 = nondet();
    let b: u128 = nondet();
    let expected = checked_result!(op, a, b);
    match arithmetic(
        op,
        RuntimeValue::U128(a.to_le_bytes()),
        RuntimeValue::U128(b.to_le_bytes()),
    ) {
        Ok(RuntimeValue::U128(result)) => cvlr_assert!(expected == Some(u128::from_le_bytes(result))),
        Ok(_) => cvlr_assert!(false),
        Err(RunError::Vm(BallistaError::DivisionByZero)) => cvlr_assert!(op == OP_DIV && b == 0),
        Err(RunError::Vm(BallistaError::ArithmeticOverflow)) => {
            cvlr_assert!(expected.is_none() && !(op == OP_DIV && b == 0))
        }
        Err(_) => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_arithmetic_rejects_mixed_and_non_numeric_operands() {
    let op = pick(&ARITHMETIC);
    let left = pick(&[
        RuntimeValue::U64(1),
        RuntimeValue::I64(1),
        RuntimeValue::Bool(true),
        RuntimeValue::Pubkey([1; 32]),
        RuntimeValue::Bytes(&[1]),
    ]);
    let right = pick(&[
        RuntimeValue::U128([0; 16]),
        RuntimeValue::Bool(false),
        RuntimeValue::Pubkey([2; 32]),
        RuntimeValue::Unset,
    ]);
    cvlr_assert!(arithmetic(op, left, right) == Err(RunError::Vm(BallistaError::TypeMismatch)));
}

#[rule]
pub fn rule_u64_comparisons_match_rust() {
    let op = pick(&ORDERED);
    let a: u64 = nondet();
    let b: u64 = nondet();
    let expected = match op {
        OP_EQ => a == b,
        OP_NE => a != b,
        OP_LT => a < b,
        OP_LTE => a <= b,
        OP_GT => a > b,
        _ => a >= b,
    };
    cvlr_assert!(compare(op, RuntimeValue::U64(a), RuntimeValue::U64(b)) == Ok(expected));
}

#[rule]
pub fn rule_i64_comparisons_match_rust() {
    let op = pick(&ORDERED);
    let a: i64 = nondet();
    let b: i64 = nondet();
    let expected = match op {
        OP_EQ => a == b,
        OP_NE => a != b,
        OP_LT => a < b,
        OP_LTE => a <= b,
        OP_GT => a > b,
        _ => a >= b,
    };
    cvlr_assert!(compare(op, RuntimeValue::I64(a), RuntimeValue::I64(b)) == Ok(expected));
}

#[rule]
pub fn rule_ordering_is_numeric_only() {
    let op = pick(&[OP_LT, OP_LTE, OP_GT, OP_GTE]);
    let pair = pick(&[
        (RuntimeValue::Bool(nondet()), RuntimeValue::Bool(nondet())),
        (RuntimeValue::Pubkey([1; 32]), RuntimeValue::Pubkey([1; 32])),
        (RuntimeValue::Bytes(&[1, 2]), RuntimeValue::Bytes(&[1, 2])),
        (RuntimeValue::U64(1), RuntimeValue::I64(1)),
    ]);
    cvlr_assert!(compare(op, pair.0, pair.1) == Err(RunError::Vm(BallistaError::TypeMismatch)));
}

#[rule]
pub fn rule_casts_succeed_exactly_when_the_value_fits() {
    let value: i64 = nondet();
    match cast(OP_CAST_U64, RuntimeValue::I64(value)) {
        Ok(RuntimeValue::U64(result)) => cvlr_assert!(value >= 0 && result == value as u64),
        Err(RunError::Vm(BallistaError::ArithmeticOverflow)) => cvlr_assert!(value < 0),
        _ => cvlr_assert!(false),
    }

    let value: u64 = nondet();
    match cast(OP_CAST_I64, RuntimeValue::U64(value)) {
        Ok(RuntimeValue::I64(result)) => cvlr_assert!(value <= i64::MAX as u64 && result == value as i64),
        Err(RunError::Vm(BallistaError::ArithmeticOverflow)) => cvlr_assert!(value > i64::MAX as u64),
        _ => cvlr_assert!(false),
    }

    let value: u128 = nondet();
    match cast(OP_CAST_U64, RuntimeValue::U128(value.to_le_bytes())) {
        Ok(RuntimeValue::U64(result)) => cvlr_assert!(value <= u64::MAX as u128 && result == value as u64),
        Err(RunError::Vm(BallistaError::ArithmeticOverflow)) => cvlr_assert!(value > u64::MAX as u128),
        _ => cvlr_assert!(false),
    }

    let value: i64 = nondet();
    match cast(OP_CAST_U128, RuntimeValue::I64(value)) {
        Ok(RuntimeValue::U128(result)) => {
            cvlr_assert!(value >= 0 && u128::from_le_bytes(result) == value as u128)
        }
        Err(RunError::Vm(BallistaError::ArithmeticOverflow)) => cvlr_assert!(value < 0),
        _ => cvlr_assert!(false),
    }

    let value: u64 = nondet();
    cvlr_assert!(cast(OP_CAST_U128, RuntimeValue::U64(value)) == Ok(RuntimeValue::U128((value as u128).to_le_bytes())));
    cvlr_assert!(cast(OP_CAST_U64, RuntimeValue::Bool(true)) == Err(RunError::Vm(BallistaError::TypeMismatch)));
}
