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
//! time (full-width runs timed out at 15 minutes), and a symbolic divisor makes the division's
//! normalization shift symbolic, which no windowing helps. So these proofs fix the divisor, one
//! call per normalization class, and bound the factors with `windowed_*` values: three narrow
//! windows of symbolic bits, at the bottom, straddling the middle, and at the top, with zeros
//! between. The covers show every path is reached: the fast paths, the wide division, overflow,
//! and the rounding carry.

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

/// `mul_div_u64(a, b, c, round_up)` meets the contract for this `c`, any rounding, and `a`, `b`
/// with 18 symbolic bits each (three 6-bit windows: bits 0–5, 29–34, 58–63). Kani also checks
/// every panic, overflow and loop bound on the way, so this proves those absent too. Returns
/// whether it succeeded.
fn u64_contract(c: u64) -> bool {
    let (a, b) = (windowed_u64(6), windowed_u64(6));
    let round_up: bool = kani::any();
    let outcome = mul_div_u64(a, b, c, round_up).map(|q| q as u128);
    let ok = outcome.is_ok();
    let ab = (0, a as u128 * b as u128);
    let max_c = (0, u64::MAX as u128 * c as u128);
    check_contract(outcome, c as u128, round_up, ab, |q| (0, q * c as u128), max_c);
    kani::cover!(ok && ab.1 >> 64 != 0, "the wide division succeeds");
    ok
}

/// One harness per divisor, so each stays small and they run in parallel: a symbolic divisor makes
/// `divlu`'s normalization shift (the divisor's leading zeros) symbolic, and every later step a
/// full divider, which does not finish.
macro_rules! u64_contracts {
    ($($name:ident: $divisor:expr => $class:literal),* $(,)?) => {$(
        #[doc = concat!("`mul_div_u64` meets the documented contract for the divisor ", $class, ": the floor or ceiling of the exact product, or `ArithmeticOverflow` exactly when it does not fit; no panic. Bound: that divisor; `a` and `b` with 18 symbolic bits each; either rounding.")]
        #[kani::proof]
        #[kani::unwind(4)]
        fn $name() {
            let ok = u64_contract($divisor);
            kani::cover!(ok, "a quotient");
        }
    )*};
}

u64_contracts!(
    mul_div_u64_contract_divisor_1: 1 => "1 (normalization shift 63)",
    mul_div_u64_contract_divisor_3: 3 => "3 (shift 62)",
    mul_div_u64_contract_divisor_10: 10 => "10 (shift 60)",
    mul_div_u64_contract_divisor_prime: 1_000_000_007 => "1,000,000,007 (shift 34)",
    mul_div_u64_contract_divisor_2_32_minus_1: (1 << 32) - 1 => "2^32 − 1 (shift 32)",
    mul_div_u64_contract_divisor_2_32_plus_1: (1 << 32) + 1 => "2^32 + 1 (shift 31)",
    mul_div_u64_contract_divisor_63_bits: (1 << 62) + 12_345 => "2^62 + 12,345 (shift 1)",
    mul_div_u64_contract_divisor_2_63: 1 << 63 => "2^63 (shift 0)",
    mul_div_u64_contract_divisor_max: u64::MAX => "u64::MAX (shift 0)",
);

/// A zero divisor is `DivisionByZero`, for every `a`, `b` and rounding, in both widths. Bound: none.
#[kani::proof]
fn mul_div_rejects_a_zero_divisor() {
    assert_eq!(mul_div_u64(kani::any(), kani::any(), 0, kani::any()), Err(err(BallistaError::DivisionByZero)));
    assert_eq!(mul_div_u128(kani::any(), kani::any(), 0, kani::any()), Err(err(BallistaError::DivisionByZero)));
}

/// `mul_div_u128(a, b, c, round_up)` meets the contract for this `c`, any rounding, and `a`, `b`
/// with 18 symbolic bits each (three 6-bit windows: bits 0–5, 61–66, 122–127), against products
/// from the 32-bit-digit schoolbook in `util::U128Parts::widening_mul`, which shares nothing with
/// `full_product`. Kani checks every panic, overflow and loop bound on the way. Returns whether it
/// succeeded.
fn u128_contract(c: u128) -> bool {
    let (a, b) = (windowed_u128(6), windowed_u128(6));
    let round_up: bool = kani::any();
    let product = |x: u128, y: u128| U128Parts::of(x).widening_mul(U128Parts::of(y));
    let outcome = mul_div_u128(a, b, c, round_up);
    let ok = outcome.is_ok();
    let ab = product(a, b);
    check_contract(outcome, c, round_up, ab, |q| product(q, c), product(u128::MAX, c));
    kani::cover!(ok && ab.0 != 0, "the 256-bit division succeeds");
    ok
}

/// As `u64_contracts!`, for `mul_div_u128`: one-digit divisors take two `divlu` steps, two-digit
/// ones Knuth D's normalized path.
macro_rules! u128_contracts {
    ($($name:ident: $divisor:expr => $class:literal),* $(,)?) => {$(
        #[doc = concat!("`mul_div_u128` meets the documented contract for the divisor ", $class, ", the 256-bit product included; no panic. Bound: that divisor; `a` and `b` with 18 symbolic bits each; either rounding.")]
        #[kani::proof]
        #[kani::unwind(8)]
        fn $name() {
            let ok = u128_contract($divisor);
            kani::cover!(ok, "a quotient");
        }
    )*};
}

u128_contracts!(
    mul_div_u128_contract_divisor_1: 1 => "1 (one digit)",
    mul_div_u128_contract_divisor_3: 3 => "3 (one digit)",
    mul_div_u128_contract_divisor_10_18: 1_000_000_000_000_000_000 => "10^18 (one digit)",
    mul_div_u128_contract_divisor_2_64_minus_1: u64::MAX as u128 => "2^64 − 1 (one digit)",
    mul_div_u128_contract_divisor_2_64_plus_1: (1u128 << 64) + 1 => "2^64 + 1 (two digits, shift 63)",
    mul_div_u128_contract_divisor_101_bits: (1u128 << 100) + 12_345 => "2^100 + 12,345 (two digits, shift 27)",
    mul_div_u128_contract_divisor_127_bits: (1u128 << 126) + 1 => "2^126 + 1 (two digits, shift 1)",
    mul_div_u128_contract_divisor_max: u128::MAX => "u128::MAX (two digits, shift 0)",
);

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
    // Compared without `==` on the values, whose byte arrays would need a larger unwind bound.
    let same = |expected: Result<RuntimeValue<'_>, RunError>| match (&outcome, &expected) {
        (Ok(x), Ok(y)) => crate::util::same_value(x, y),
        (Err(x), Err(y)) => x == y,
        _ => false,
    };
    match (a, b, c) {
        (RuntimeValue::U64(a), RuntimeValue::U64(b), RuntimeValue::U64(c)) => {
            assert!(same(mul_div_u64(a, b, c, round_up).map(RuntimeValue::U64)));
            kani::cover!(outcome.is_ok(), "a u64 multiply-divide");
        }
        (RuntimeValue::U128(a), RuntimeValue::U128(b), RuntimeValue::U128(c)) => {
            let expected = mul_div_u128(
                u128::from_le_bytes(a),
                u128::from_le_bytes(b),
                u128::from_le_bytes(c),
                round_up,
            );
            assert!(same(expected.map(|q| RuntimeValue::U128(q.to_le_bytes()))));
            kani::cover!(outcome.is_ok(), "a u128 multiply-divide");
        }
        _ => {
            assert!(same(Err(err(BallistaError::TypeMismatch))));
            kani::cover!(
                matches!((a, b, c), (RuntimeValue::U64(_), RuntimeValue::U64(_), RuntimeValue::U128(_))),
                "mixed widths"
            );
        }
    }
}
