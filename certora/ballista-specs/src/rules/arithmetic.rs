//! Checked arithmetic, comparisons, and casts behave exactly as specified for every input.

use ballista::error::BallistaError;
use ballista::processor::execute::{arithmetic, cast, compare, RunError, RunResult, RuntimeValue};
use ballista::processor::math::{integer, mul_div};
use ballista_common::template::*;
use cvlr::prelude::*;

use super::util::pick;

/// One of the six arithmetic opcodes.
fn arithmetic_opcode() -> u8 {
    pick!(OP_ADD, OP_SUB, OP_MUL, OP_DIV, OP_MIN, OP_MAX)
}

/// The arithmetic opcodes other than division. Signed division compiles to a compiler-rt routine
/// the prover treats as an uninterpreted function, so its quotient is checked separately by the
/// unit tests and only its error conditions are stated here.
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

/// What a `u128` arithmetic call returned.
#[derive(Clone, Copy, PartialEq, Eq)]
enum U128Outcome {
    Value(u128),
    DivisionByZero,
    Overflow,
    Other,
}

/// One `u128` opcode on nondeterministic operands: Rust's checked result, the call's outcome, and
/// the divisor. Shared by each opcode's rule, its reachability rules and its twin.
#[inline(always)]
fn u128_case(op: u8) -> (Option<u128>, U128Outcome, u128) {
    let a: u128 = nondet();
    let b: u128 = nondet();
    let expected = checked_result!(op, a, b);
    clog!(a, b);
    let outcome = match arithmetic(op, RuntimeValue::U128(a.to_le_bytes()), RuntimeValue::U128(b.to_le_bytes())) {
        Ok(RuntimeValue::U128(result)) => U128Outcome::Value(u128::from_le_bytes(result)),
        Err(RunError::Vm(BallistaError::DivisionByZero)) => U128Outcome::DivisionByZero,
        Err(RunError::Vm(BallistaError::ArithmeticOverflow)) => U128Outcome::Overflow,
        _ => U128Outcome::Other,
    };
    (expected, outcome, b)
}

/// The checked-arithmetic property for `u128`, one rule per opcode: the result is Rust's checked
/// result, division by zero is reported as such, and every other failure is an overflow Rust's
/// checked operator also reports. A single rule over all six asked the solver for six nonlinear
/// 128-bit products on every path; with the opcode a constant, `arithmetic` (always inlined)
/// compiles to one straight-line case per rule, and the values stay in registers and on the stack
/// as whole words. Products and quotients go through compiler-rt's `__multi3` and `__udivti3`,
/// which the prover summarizes with exact mathematical integers under `-solanaTACMathInt`.
///
/// Each opcode also gets a reachability rule per outcome it can reach, and a twin that must fail:
/// it claims every result is one more than it is.
#[inline(always)]
fn check_u128(op: u8) {
    let (expected, outcome, divisor) = u128_case(op);
    match outcome {
        U128Outcome::Value(result) => cvlr_assert!(expected == Some(result)),
        U128Outcome::DivisionByZero => cvlr_assert!(op == OP_DIV && divisor == 0),
        U128Outcome::Overflow => cvlr_assert!(expected.is_none() && !(op == OP_DIV && divisor == 0)),
        U128Outcome::Other => cvlr_assert!(false),
    }
}

macro_rules! u128_rules {
    ($op:expr, $rule:ident, $twin:ident, [$($reach:ident => $outcome:pat),* $(,)?]) => {
        #[rule]
        pub fn $rule() {
            check_u128($op);
        }

        $(
            #[rule]
            pub fn $reach() {
                let (_, outcome, _) = u128_case($op);
                cvlr_satisfy!(matches!(outcome, $outcome));
            }
        )*

        #[rule]
        pub fn $twin() {
            let (expected, outcome, _) = u128_case($op);
            if let U128Outcome::Value(result) = outcome {
                cvlr_assert!(expected == Some(result.wrapping_add(1)));
            }
        }
    };
}

u128_rules!(OP_ADD, rule_u128_add_is_checked, rule_u128_add_twin_is_off_by_one, [
    rule_u128_add_reaches_a_sum => U128Outcome::Value(_),
    rule_u128_add_reaches_overflow => U128Outcome::Overflow,
]);
u128_rules!(OP_SUB, rule_u128_sub_is_checked, rule_u128_sub_twin_is_off_by_one, [
    rule_u128_sub_reaches_a_difference => U128Outcome::Value(_),
    rule_u128_sub_reaches_overflow => U128Outcome::Overflow,
]);
u128_rules!(OP_MUL, rule_u128_mul_is_checked, rule_u128_mul_twin_is_off_by_one, [
    rule_u128_mul_reaches_a_product => U128Outcome::Value(_),
    rule_u128_mul_reaches_overflow => U128Outcome::Overflow,
]);
// Unsigned division overflows never; only a zero divisor fails it.
u128_rules!(OP_DIV, rule_u128_div_is_checked, rule_u128_div_twin_is_off_by_one, [
    rule_u128_div_reaches_a_quotient => U128Outcome::Value(_),
    rule_u128_div_reaches_division_by_zero => U128Outcome::DivisionByZero,
]);
u128_rules!(OP_MIN, rule_u128_min_is_exact, rule_u128_min_twin_is_off_by_one, [
    rule_u128_min_reaches_a_result => U128Outcome::Value(_),
]);
u128_rules!(OP_MAX, rule_u128_max_is_exact, rule_u128_max_twin_is_off_by_one, [
    rule_u128_max_reaches_a_result => U128Outcome::Value(_),
]);

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

/// Five of the six conversions between `i64`, `u64` and `u128` (`i64` to `u64`, `u64` to `i64`,
/// `u128` to `u64`, `i64` to `u128`, `u64` to `u128`) succeed exactly when the value fits, and a
/// `bool` is refused. `rule_u128_to_i64_cast_succeeds_exactly_when_the_value_fits` has the sixth.
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

/// What converting a `u128` to an `i64` returned.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CastOutcome {
    Value(i64),
    Overflow,
    Other,
}

/// `CAST_I64` of a nondeterministic `u128`: the value and the outcome.
#[inline(always)]
fn u128_to_i64_case() -> (u128, CastOutcome) {
    let value: u128 = nondet();
    clog!(value);
    let outcome = match cast(OP_CAST_I64, RuntimeValue::U128(value.to_le_bytes())) {
        Ok(RuntimeValue::I64(result)) => CastOutcome::Value(result),
        Err(RunError::Vm(BallistaError::ArithmeticOverflow)) => CastOutcome::Overflow,
        _ => CastOutcome::Other,
    };
    (value, outcome)
}

/// The sixth conversion: `u128` to `i64` succeeds exactly when the value is at most `i64::MAX`,
/// and then keeps it.
#[rule]
pub fn rule_u128_to_i64_cast_succeeds_exactly_when_the_value_fits() {
    let (value, outcome) = u128_to_i64_case();
    match outcome {
        CastOutcome::Value(result) => cvlr_assert!(value <= i64::MAX as u128 && result as u128 == value),
        CastOutcome::Overflow => cvlr_assert!(value > i64::MAX as u128),
        CastOutcome::Other => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_u128_to_i64_cast_reaches_a_value() {
    let (_, outcome) = u128_to_i64_case();
    cvlr_satisfy!(matches!(outcome, CastOutcome::Value(_)));
}

#[rule]
pub fn rule_u128_to_i64_cast_reaches_overflow() {
    let (_, outcome) = u128_to_i64_case();
    cvlr_satisfy!(outcome == CastOutcome::Overflow);
}

/// Twin that must fail: it puts the boundary one too low, so `i64::MAX` itself refutes it.
#[rule]
pub fn rule_u128_to_i64_cast_twin_misses_the_boundary() {
    let (value, outcome) = u128_to_i64_case();
    if let CastOutcome::Value(_) = outcome {
        cvlr_assert!(value < i64::MAX as u128);
    }
}

/// Remainder, shifts and bitwise operations over `u64` match Rust's checked operators exactly.
#[rule]
pub fn rule_u64_integer_operations_match_rust() {
    let op = pick!(OP_REM, OP_SHL, OP_SHR, OP_BIT_AND, OP_BIT_OR, OP_BIT_XOR);
    let a: u64 = nondet();
    let b: u64 = nondet();
    clog!(op, a, b);
    let expected: Option<u64> = match op {
        OP_REM => a.checked_rem(b),
        OP_SHL => {
            if b >= 64 {
                if a == 0 { Some(0) } else { None }
            } else {
                let shifted = a << b;
                if shifted >> b == a { Some(shifted) } else { None }
            }
        }
        OP_SHR => Some(if b >= 64 { 0 } else { a >> b }),
        OP_BIT_AND => Some(a & b),
        OP_BIT_OR => Some(a | b),
        _ => Some(a ^ b),
    };
    match integer(op, RuntimeValue::U64(a), RuntimeValue::U64(b)) {
        Ok(RuntimeValue::U64(result)) => cvlr_assert!(expected == Some(result)),
        Err(RunError::Vm(BallistaError::DivisionByZero)) => cvlr_assert!(op == OP_REM && b == 0),
        Err(RunError::Vm(BallistaError::ArithmeticOverflow)) => {
            cvlr_assert!(expected.is_none() && !(op == OP_REM && b == 0))
        }
        _ => cvlr_assert!(false),
    }
}

/// What a `u64` `mul_div` returned, next to the exact rounded quotient.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MulDivOutcome {
    Value(u64),
    DivisionByZero,
    Overflow,
    Other,
}

/// `mul_div` on nondeterministic `u64` operands: the divisor, the exact rounded quotient (zero for
/// a zero divisor) and the outcome.
#[inline(always)]
fn mul_div_case() -> (u64, u128, MulDivOutcome) {
    let a: u64 = nondet();
    let b: u64 = nondet();
    let c: u64 = nondet();
    let round_up: bool = nondet();
    let product = a as u128 * b as u128;
    // The exact rounded quotient, for a nonzero divisor. `floor * c` is at most `product`, so the
    // wrapping product is exact.
    let rounded = if c == 0 {
        0
    } else {
        let floor = product / c as u128;
        let exact = floor.wrapping_mul(c as u128) == product;
        if round_up && !exact { floor + 1 } else { floor }
    };
    clog!(a, b, c, round_up);
    let outcome = match mul_div(round_up, RuntimeValue::U64(a), RuntimeValue::U64(b), RuntimeValue::U64(c)) {
        Ok(RuntimeValue::U64(result)) => MulDivOutcome::Value(result),
        Err(RunError::Vm(BallistaError::DivisionByZero)) => MulDivOutcome::DivisionByZero,
        Err(RunError::Vm(BallistaError::ArithmeticOverflow)) => MulDivOutcome::Overflow,
        _ => MulDivOutcome::Other,
    };
    (c, rounded, outcome)
}

/// `mul_div` over `u64` is the rounded quotient of the exact product, failing only for a zero
/// divisor or a quotient past `u64::MAX`. The `u64` path is `mul_div_u64`, which never divides a
/// `u128`: it widens the product with `mul64`'s native 32×32→64 multiplies, then estimates the
/// quotient's digits with `divlu`, corrected at most twice per digit loop.
///
/// The rule computes the expected quotient with one `__udivti3` and tests the remainder with one
/// `__multi3` (`floor * c != product`), both of which the prover summarizes exactly; it used
/// `product % c`, which compiles to `__umodti3`, a routine the prover leaves as an opaque call
/// that never writes its result slot. Its overflow case is now exact (the rounded quotient exceeds
/// `u64::MAX`) where it was a necessary condition (the floor reaches `u64::MAX`).
///
/// Blocked, suspected: `mul_div` is out of line and builds its result in a stack temporary whose
/// `RuntimeValue` tag is one byte, then copies it out with eight-byte moves, and its errors as
/// two-byte tags and four-byte kinds. The prover rebuilds a stack word only from two four-byte
/// halves, so it may lose the kind, or the tag.
#[rule]
pub fn rule_u64_mul_div_is_exact() {
    let (c, rounded, outcome) = mul_div_case();
    match outcome {
        MulDivOutcome::Value(result) => cvlr_assert!(c != 0 && result as u128 == rounded),
        MulDivOutcome::DivisionByZero => cvlr_assert!(c == 0),
        MulDivOutcome::Overflow => cvlr_assert!(c != 0 && rounded > u64::MAX as u128),
        MulDivOutcome::Other => cvlr_assert!(false),
    }
}

#[rule]
pub fn rule_u64_mul_div_reaches_a_quotient() {
    let (_, _, outcome) = mul_div_case();
    cvlr_satisfy!(matches!(outcome, MulDivOutcome::Value(_)));
}

#[rule]
pub fn rule_u64_mul_div_reaches_division_by_zero() {
    let (_, _, outcome) = mul_div_case();
    cvlr_satisfy!(outcome == MulDivOutcome::DivisionByZero);
}

#[rule]
pub fn rule_u64_mul_div_reaches_overflow() {
    let (_, _, outcome) = mul_div_case();
    cvlr_satisfy!(outcome == MulDivOutcome::Overflow);
}

/// Twin that must fail: it claims every quotient is one more than the exact rounded one.
#[rule]
pub fn rule_u64_mul_div_twin_is_off_by_one() {
    let (_, rounded, outcome) = mul_div_case();
    if let MulDivOutcome::Value(result) = outcome {
        cvlr_assert!(result as u128 == rounded + 1);
    }
}
