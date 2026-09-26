//! Checked arithmetic, comparisons, and casts behave exactly as specified for every input.

use ballista::error::BallistaError;
use ballista::processor::execute::{arithmetic, cast, compare, RunError, RunResult, RuntimeValue};
use ballista_common::template::*;
use cvlr::prelude::*;

use super::util::pick;

/// One of the six arithmetic opcodes.
fn arithmetic_opcode() -> u8 {
    pick!(OP_ADD, OP_SUB, OP_MUL, OP_DIV, OP_MIN, OP_MAX)
}

/// The arithmetic opcodes other than division. Signed division compiles to a compiler-rt routine
/// the prover treats as an uninterpreted function, so its quotient is checked separately by the
/// unit tests and only its error conditions are proved here.
fn non_division_opcode() -> u8 {
    pick!(OP_ADD, OP_SUB, OP_MUL, OP_MIN, OP_MAX)
}

/// One of the six comparison opcodes.
fn comparison_opcode() -> u8 {
    pick!(OP_EQ, OP_NE, OP_LT, OP_LTE, OP_GT, OP_GTE)
}

/// Records which way an arithmetic call went, so a counterexample shows the outcome next to the
/// inputs: 1 = numeric result, 2 = result of another type, 3 = division by zero, 4 = overflow,
/// 5 = any other error. The low 64 bits of the returned value follow when there is one.
fn log_outcome(outcome: &RunResult<RuntimeValue<'_>>, expected_some: bool, expected_low: u64) {
    let (outcome_tag, result_low): (u64, u64) = match outcome {
        Ok(RuntimeValue::U64(value)) => (1, *value),
        Ok(RuntimeValue::I64(value)) => (1, *value as u64),
        Ok(RuntimeValue::U128(bytes)) => (1, u128::from_le_bytes(*bytes) as u64),
        Ok(_) => (2, 0),
        Err(RunError::Vm(BallistaError::DivisionByZero)) => (3, 0),
        Err(RunError::Vm(BallistaError::ArithmeticOverflow)) => (4, 0),
        Err(_) => (5, 0),
    };
    clog!(outcome_tag, result_low, expected_some, expected_low);
}

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
    let op = arithmetic_opcode();
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
    let op = non_division_opcode();
    let a: i64 = nondet();
    let b: i64 = nondet();
    let expected = checked_result!(op, a, b);
    clog!(op, a, b);
    let outcome = arithmetic(op, RuntimeValue::I64(a), RuntimeValue::I64(b));
    log_outcome(&outcome, expected.is_some(), expected.unwrap_or(0) as u64);
    match outcome {
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

/// Signed division reports division by zero and the one overflowing quotient, and otherwise
/// returns an `i64`. The quotient's value is outside what the prover models.
#[rule]
pub fn rule_i64_division_reports_zero_and_overflow() {
    let a: i64 = nondet();
    let b: i64 = nondet();
    clog!(a, b);
    let outcome = arithmetic(OP_DIV, RuntimeValue::I64(a), RuntimeValue::I64(b));
    log_outcome(&outcome, b != 0 && !(a == i64::MIN && b == -1), 0);
    match outcome {
        Ok(RuntimeValue::I64(_)) => cvlr_assert!(b != 0 && !(a == i64::MIN && b == -1)),
        Ok(_) => cvlr_assert!(false),
        Err(RunError::Vm(BallistaError::DivisionByZero)) => cvlr_assert!(b == 0),
        Err(RunError::Vm(BallistaError::ArithmeticOverflow)) => cvlr_assert!(a == i64::MIN && b == -1),
        Err(_) => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_u128_arithmetic_is_checked() {
    let op = arithmetic_opcode();
    let a: u128 = nondet();
    let b: u128 = nondet();
    let expected = checked_result!(op, a, b);
    clog!(op, a, b);
    let outcome = arithmetic(
        op,
        RuntimeValue::U128(a.to_le_bytes()),
        RuntimeValue::U128(b.to_le_bytes()),
    );
    log_outcome(&outcome, expected.is_some(), expected.unwrap_or(0) as u64);
    match outcome {
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
    let op = arithmetic_opcode();
    let left = pick!(
        RuntimeValue::U64(1),
        RuntimeValue::I64(1),
        RuntimeValue::Bool(true),
        RuntimeValue::Pubkey([1; 32]),
        RuntimeValue::Bytes(&[1]),
    );
    let right = pick!(
        RuntimeValue::U128([0; 16]),
        RuntimeValue::Bool(false),
        RuntimeValue::Pubkey([2; 32]),
        RuntimeValue::Unset,
    );
    cvlr_assert!(arithmetic(op, left, right) == Err(RunError::Vm(BallistaError::TypeMismatch)));
}

#[rule]
pub fn rule_u64_comparisons_match_rust() {
    let op = comparison_opcode();
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
    let op = comparison_opcode();
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
    let op = pick!(OP_LT, OP_LTE, OP_GT, OP_GTE);
    let pair = pick!(
        (RuntimeValue::Bool(nondet()), RuntimeValue::Bool(nondet())),
        (RuntimeValue::Pubkey([1; 32]), RuntimeValue::Pubkey([1; 32])),
        (RuntimeValue::Bytes(&[1, 2]), RuntimeValue::Bytes(&[1, 2])),
        (RuntimeValue::U64(1), RuntimeValue::I64(1)),
    );
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
