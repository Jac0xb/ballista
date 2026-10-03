//! `MUL_DIV` and `MUL_DIV_CEIL`: `math::mul_div`, `mul_div_u64`, `mul_div_u128` and
//! `full_product`.
//!
//! The documented contract (docs/reference/language.md, "Computation"): `a × b ÷ c` with the
//! product held exactly, rounded down or up; `DivisionByZero` when `c` is zero; otherwise
//! `ArithmeticOverflow` exactly when the rounded result does not fit the operands' type.
//!
//! Every correctness spec here is relational, `q·c ≤ a·b < q·c + c` for the floor and
//! `a·b ≤ q·c < a·b + c` for the ceiling. Proving it at full width asks the solver to show two
//! different 64- or 128-bit multipliers equal, which bit-level model checking does not do in useful
//! time (full-width runs timed out at 15 minutes). So these proofs bound their operands with
//! `windowed_*` values: three narrow windows of symbolic bits, at the bottom, straddling the
//! middle, and at the top, with zeros between. Every path stays reachable, and the covers below
//! show it: both fast paths, the 128-bit and 256-bit products, the wide division, overflow, and
//! the rounding carry.

use ballista::error::BallistaError;
use ballista::processor::execute::{RunError, RuntimeValue};
use ballista::processor::math::{full_product, mul_div, mul_div_u128, mul_div_u64};

use crate::util::{any_runtime_value, windowed_u128, windowed_u64, U128Parts};

fn err(kind: BallistaError) -> RunError {
    RunError::Vm(kind)
}

/// A 256-bit value as `(high, low)` 128-bit halves.
type Wide = (u128, u128);

fn wide_less_than(a: Wide, b: Wide) -> bool {
    a.0 < b.0 || (a.0 == b.0 && a.1 < b.1)
}

fn wide_add(a: Wide, small: u128) -> Wide {
    let (low, carry) = a.1.overflowing_add(small);
    (a.0 + carry as u128, low)
}

/// Checks `outcome` against the contract for `a × b ÷ c` over a type whose largest value is
/// `max`, given the exact products `ab = a·b` and `qc(q) = q·c`, `max_c = max·c`. `Ok(q)` must be the
/// floor or the ceiling; an error must be the documented one, exactly when it is due.
fn check_contract(
    outcome: Result<u128, RunError>,
    c: u128,
    round_up: bool,
    ab: Wide,
    qc: impl Fn(u128) -> Wide,
    max_c: Wide,
) {
    if c == 0 {
        assert_eq!(outcome, Err(err(BallistaError::DivisionByZero)));
        return;
    }
    // The rounded quotient exceeds max: for the floor, a·b ≥ (max + 1)·c; for the ceiling,
    // a·b > max·c.
    let overflows = if round_up {
        wide_less_than(max_c, ab)
    } else {
        !wide_less_than(ab, wide_add(max_c, c))
    };
    if overflows {
        assert_eq!(outcome, Err(err(BallistaError::ArithmeticOverflow)));
        kani::cover!(round_up && wide_less_than(ab, wide_add(max_c, c)), "only the rounding overflows");
    } else {
        let q = outcome.expect("an in-range quotient");
        let q_c = qc(q);
        if round_up {
            // a·b ≤ q·c < a·b + c
            assert!(!wide_less_than(q_c, ab) && wide_less_than(q_c, wide_add(ab, c)));
            kani::cover!(q_c != ab, "rounding up adds one");
        } else {
            // q·c ≤ a·b < q·c + c
            assert!(!wide_less_than(ab, q_c) && wide_less_than(ab, wide_add(q_c, c)));
        }
    }
}

/// `mul_div_u64` meets the documented contract. The spec multiplies in `u128`, which holds every
/// `u64` product exactly. Bound: each operand has 12 symbolic bits, three 4-bit windows (bits 0–3,
/// 30–33, 60–63); the rounding mode is symbolic.
#[kani::proof]
#[kani::unwind(4)]
fn mul_div_u64_meets_the_contract() {
    let (a, b, c) = (windowed_u64(4), windowed_u64(4), windowed_u64(4));
    let round_up: bool = kani::any();
    let outcome = mul_div_u64(a, b, c, round_up).map(|q| q as u128);
    let ab = (0, a as u128 * b as u128);
    let max_c = (0, u64::MAX as u128 * c as u128);
    let ok = outcome.is_ok();
    check_contract(outcome, c as u128, round_up, ab, |q| (0, q * c as u128), max_c);
    kani::cover!(ab.1 >> 64 != 0 && ok, "the wide path (a product past 64 bits) succeeds");
    kani::cover!(ab.1 >> 64 == 0 && ok && c > 1, "the native path succeeds");
}

/// `mul_div_u64` never panics, overflows an intermediate, divides by zero or exceeds its loop
/// bound. `divlu`'s two correction loops run at most twice each; the unwind bound of 4 proves it.
/// Bound: each operand has 30 symbolic bits, three 10-bit windows (a full-width run did not finish
/// in 15 minutes); the rounding mode is symbolic.
#[kani::proof]
#[kani::unwind(4)]
fn mul_div_u64_never_panics() {
    let (a, b, c) = (windowed_u64(10), windowed_u64(10), windowed_u64(10));
    let outcome = mul_div_u64(a, b, c, kani::any());
    kani::cover!(outcome.is_ok() && (a as u128 * b as u128) >> 64 != 0, "the wide division runs");
}

/// `mul_div_u128` meets the documented contract, the 256-bit product included, against products
/// taken by the 32-bit-digit schoolbook in `util::U128Parts::widening_mul`, which shares nothing
/// with `full_product`. Bound: each operand has 12 symbolic bits, three 4-bit windows (bits 0–3,
/// 62–65, 124–127); the rounding mode is symbolic.
#[kani::proof]
#[kani::unwind(8)]
fn mul_div_u128_meets_the_contract() {
    let (a, b, c) = (windowed_u128(4), windowed_u128(4), windowed_u128(4));
    let round_up: bool = kani::any();
    let product = |x: u128, y: u128| U128Parts::of(x).widening_mul(U128Parts::of(y));
    let outcome = mul_div_u128(a, b, c, round_up);
    let ab = product(a, b);
    let ok = outcome.is_ok();
    check_contract(outcome, c, round_up, ab, |q| product(q, c), product(u128::MAX, c));
    kani::cover!(ab.0 != 0 && ok, "the 256-bit path succeeds");
    kani::cover!(ab.0 == 0 && (a | b) >> 64 == 0 && ok, "the one-multiply path succeeds");
    kani::cover!(ab.0 == 0 && (ab.1 | c) >> 64 != 0 && ok, "the u128 division path succeeds");
}

/// `mul_div_u128` never panics, overflows an intermediate, divides by zero or exceeds its loop
/// bounds (`divide_3by2`'s correction loop runs at most twice). Bound: each operand has 18 symbolic
/// bits, three 6-bit windows; the rounding mode is symbolic.
#[kani::proof]
#[kani::unwind(4)]
fn mul_div_u128_never_panics() {
    let (a, b, c) = (windowed_u128(6), windowed_u128(6), windowed_u128(6));
    let outcome = mul_div_u128(a, b, c, kani::any());
    kani::cover!(outcome.is_ok() && c >> 64 != 0 && (a >> 64) * (b >> 64) != 0, "a two-digit divisor in the wide division");
}

/// `full_product` is the exact 256-bit product, against the 32-bit-digit schoolbook product.
/// Bound: each operand has 12 symbolic bits, three 4-bit windows.
#[kani::proof]
#[kani::unwind(8)]
fn full_product_is_the_exact_product() {
    let (a, b) = (windowed_u128(4), windowed_u128(4));
    let exact = U128Parts::of(a).widening_mul(U128Parts::of(b));
    assert_eq!(full_product(a, b), exact);
    kani::cover!(exact.0 >> 64 != 0, "a product reaching the top limb");
}

/// `full_product` never overflows an intermediate, for every pair of `u128`s. Bound: none.
#[kani::proof]
fn full_product_never_overflows() {
    let (a, b): (u128, u128) = (kani::any(), kani::any());
    let (high, _) = full_product(a, b);
    kani::cover!(high == u128::MAX - 1, "the largest product");
}

/// `mul_div` on registers: three `u64`s go to `mul_div_u64`, three `u128`s to `mul_div_u128`,
/// with the result in the operands' type, and any other combination is a `TypeMismatch`. Bound:
/// every variant combination; numeric operands below 16 (the arithmetic itself is the other
/// harnesses' subject); `bytes` up to 2 bytes.
#[kani::proof]
#[kani::unwind(5)]
fn mul_div_dispatches_on_operand_types() {
    let round_up: bool = kani::any();
    let (x, y, z): ([u8; 2], [u8; 2], [u8; 2]) = (kani::any(), kani::any(), kani::any());
    let (a, b, c) = (any_runtime_value(&x), any_runtime_value(&y), any_runtime_value(&z));
    let small = |value: RuntimeValue<'_>| match value {
        RuntimeValue::U64(v) => v < 16,
        RuntimeValue::U128(v) => u128::from_le_bytes(v) < 16,
        _ => true,
    };
    kani::assume(small(a) && small(b) && small(c));
    let outcome = mul_div(round_up, a, b, c);
    match (a, b, c) {
        (RuntimeValue::U64(a), RuntimeValue::U64(b), RuntimeValue::U64(c)) => {
            assert_eq!(outcome, mul_div_u64(a, b, c, round_up).map(RuntimeValue::U64));
            kani::cover!(outcome.is_ok(), "a u64 multiply-divide");
        }
        (RuntimeValue::U128(a), RuntimeValue::U128(b), RuntimeValue::U128(c)) => {
            let expected = mul_div_u128(
                u128::from_le_bytes(a),
                u128::from_le_bytes(b),
                u128::from_le_bytes(c),
                round_up,
            );
            assert_eq!(outcome, expected.map(|q| RuntimeValue::U128(q.to_le_bytes())));
            kani::cover!(outcome.is_ok(), "a u128 multiply-divide");
        }
        _ => {
            assert_eq!(outcome, Err(err(BallistaError::TypeMismatch)));
            kani::cover!(matches!((a, b, c), (RuntimeValue::U64(_), RuntimeValue::U64(_), RuntimeValue::U128(_))), "mixed widths");
        }
    }
}
