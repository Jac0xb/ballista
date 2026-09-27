# Runtime math: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to carry out this plan task by task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add ten math opcodes (`MUL_DIV`, `MUL_DIV_CEIL`, `REM`, `SHL`, `SHR`, `BIT_AND`,
`BIT_OR`, `BIT_XOR`, `POW10`, `READ_I32`) to the Ballista VM. Each is verified at finalize,
executed on chain, authorable from TypeScript and from the Rust `ProgramBuilder`, and covered end
to end under Mollusk. Then upgrade the oracle-checked swap example to use them.

**Architecture:**
- Each new operation is a new opcode, numbered 51 to 60. The run path never re-verifies, so new
  meaning must never ride on fields the current verifier ignores.
- The pure arithmetic lives in a new executor module,
  `programs/ballista/src/processor/math.rs`. `execute.rs` only dispatches to it.
- The verifier types each opcode exactly as the spec says:
  `docs/superpowers/specs/2026-09-26-runtime-extensions-design.md`, section 1.
- The TypeScript compiler gains matching expression kinds, plus a parity test that reads the
  Rust opcode constants so the two tables cannot drift.

**Tech stack:**
- Rust (no_std SBF program on pinocchio 0.11, shared `ballista-common` crate).
- TypeScript SDK (Zod, vitest).
- Mollusk 0.14 integration suite; Certora specs.

**Base:**
- Branch `claude/runtime-extensions`, created from `cu/integrated`. Line numbers below refer to
  that base.
- All paths are relative to the worktree root.

**Conventions:**
- Keep the surrounding style: doc comments that explain why, `#[inline(never)]` for heavy
  helpers reached from the dispatch loop, and `RunResult`/`BallistaError` for failures.
- Commit after each task with a descriptive message ending in
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

---

## File map

| File | Change |
| --- | --- |
| `common/src/template/wire.rs` | Add `OP_MUL_DIV` … `OP_READ_I32` (51–60) |
| `common/src/template/verify.rs` | Type rules for the ten opcodes; `read_width` for `READ_I32`; tests |
| `programs/ballista/src/processor/math.rs` | **New.** `mul_div`, `integer` (rem, shifts, bitwise), `pow10`, and 256-bit helpers, with unit tests |
| `programs/ballista/src/processor/mod.rs` | Declare `math` (public under `spec-api`, like `execute`) |
| `programs/ballista/src/processor/execute.rs` | Dispatch arms; `READ_I32` in the read arm and `decode_value` |
| `common/src/template/builder.rs` | `mul_div`, `mul_div_ceil`, `pow10` helpers |
| `common/src/template/generate.rs` | Generator arms for the new pure ops |
| `clients/js/src/schema.ts` | Read type `i32`; binary ops; `multiplyDivide`, `powerOfTen` expressions |
| `clients/js/src/compiler.ts` | Opcode table entries; type rules and lowering |
| `clients/js/src/opcodes.test.ts` | **New.** Rust ↔ TypeScript opcode parity |
| `clients/js/src/compiler.test.ts` | Compiler tests for the new expressions |
| `clients/js/src/fixtures.test.ts` | New `math-ops` fixture |
| `tests/ballista/src/lib.rs` | `fixture()` arm, and an end-to-end math test |
| `certora/ballista-specs/src/rules/arithmetic.rs`, `run.conf`, `run-blocked.conf` | Rules for the new ops |
| `clients/js/examples/protocols/jupiter-oracle-checked-swap.ts` | Read decimals and the `i32` exponent; compute the floor on chain |
| `clients/js/src/protocol-semantics.test.ts` | Follow the new expression kinds; assert decimals are read |

---

### Task 0: Worktree and baseline

- [ ] **Step 1: Create the worktree.** Use superpowers:using-git-worktrees. Create branch
  `claude/runtime-extensions` from `cu/integrated`, at
  `.claude/worktrees/runtime-extensions`.
- [ ] **Step 2: Install and build.**

```bash
cd .claude/worktrees/runtime-extensions
pnpm install --frozen-lockfile
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
```

- [ ] **Step 3: Baseline, all green before any change.**

```bash
pnpm fixtures && git diff --exit-code fixtures
pnpm check
pnpm test
cargo test --manifest-path tests/ballista/Cargo.toml
cargo check -p ballista-specs --features rt --manifest-path certora/Cargo.toml
```

Expected: every command exits 0. If any fails on the untouched base, stop and report it; do not
fix unrelated failures silently.

- [ ] **Step 4: Commit the spec and this plan.**

```bash
mkdir -p docs/superpowers/specs docs/superpowers/plans
cp /private/tmp/claude-501/-Users-jacob-Documents-ballista/06470e69-8486-4adc-b478-7e3c20bf99aa/scratchpad/2026-09-26-runtime-extensions-design.md docs/superpowers/specs/
cp /private/tmp/claude-501/-Users-jacob-Documents-ballista/06470e69-8486-4adc-b478-7e3c20bf99aa/scratchpad/2026-09-26-runtime-math.md docs/superpowers/plans/
git add docs/superpowers
git commit -m "Specify the runtime math, loop, output and introspection additions"
```

---

### Task 1: Opcode constants and verifier rules

**Files:**
- Modify: `common/src/template/wire.rs`, after `OP_CREATE_PDA` (around line 139).
- Modify: `common/src/template/verify.rs`:
  - the READ arm (351–370);
  - the arithmetic arm (the `OP_ADD | OP_SUB …` arm);
  - the RETURN_DATA arm (444–465);
  - `read_width` (808–818);
  - tests.

- [ ] **Step 1: Write the failing verifier tests.** In `verify.rs`, add these cases to the
  `cases` array in `every_opcode_rejects_uninitialized_or_mistyped_operands`, before the
  `(39, …)` line:

```rust
            (OP_REM, Some(VALUE_I64), Some(VALUE_I64), Ok(())),
            (OP_REM, Some(VALUE_U128), Some(VALUE_U128), Ok(())),
            (OP_REM, Some(VALUE_U64), Some(VALUE_I64), Err(TemplateError::TypeMismatch)),
            (OP_REM, Some(VALUE_BOOL), Some(VALUE_BOOL), Err(TemplateError::TypeMismatch)),
            (OP_SHL, Some(VALUE_U64), Some(VALUE_U64), Ok(())),
            (OP_SHR, Some(VALUE_U128), Some(VALUE_U64), Ok(())),
            (OP_SHL, Some(VALUE_I64), Some(VALUE_U64), Err(TemplateError::TypeMismatch)),
            (OP_SHR, Some(VALUE_U64), Some(VALUE_U128), Err(TemplateError::TypeMismatch)),
            (OP_SHL, Some(VALUE_U64), None, Err(TemplateError::RegisterNotInitialized(1))),
            (OP_BIT_AND, Some(VALUE_U64), Some(VALUE_U64), Ok(())),
            (OP_BIT_OR, Some(VALUE_U128), Some(VALUE_U128), Ok(())),
            (OP_BIT_XOR, Some(VALUE_I64), Some(VALUE_I64), Err(TemplateError::TypeMismatch)),
            (OP_BIT_AND, Some(VALUE_U64), Some(VALUE_U128), Err(TemplateError::TypeMismatch)),
            (OP_BIT_OR, Some(VALUE_BOOL), Some(VALUE_BOOL), Err(TemplateError::TypeMismatch)),
            (OP_POW10, Some(VALUE_U64), None, Ok(())),
            (OP_POW10, Some(VALUE_U128), None, Err(TemplateError::TypeMismatch)),
            (OP_POW10, None, None, Err(TemplateError::RegisterNotInitialized(0))),
```

Then add a new test after that function:

```rust
    #[test]
    fn multiply_divide_takes_three_matching_unsigned_operands() {
        let cases: &[(u8, [Option<u8>; 3], Result<(), TemplateError>)] = &[
            (OP_MUL_DIV, [Some(VALUE_U64); 3], Ok(())),
            (OP_MUL_DIV_CEIL, [Some(VALUE_U128); 3], Ok(())),
            (OP_MUL_DIV, [Some(VALUE_I64); 3], Err(TemplateError::TypeMismatch)),
            (
                OP_MUL_DIV,
                [Some(VALUE_U64), Some(VALUE_U64), Some(VALUE_U128)],
                Err(TemplateError::TypeMismatch),
            ),
            (
                OP_MUL_DIV_CEIL,
                [Some(VALUE_U128), Some(VALUE_U64), Some(VALUE_U128)],
                Err(TemplateError::TypeMismatch),
            ),
            (
                OP_MUL_DIV,
                [Some(VALUE_U64), Some(VALUE_U64), None],
                Err(TemplateError::RegisterNotInitialized(2)),
            ),
        ];
        for (opcode, types, expected) in cases {
            let mut builder = ProgramBuilder::new();
            let a = typed_register(&mut builder, types[0]);
            let b = typed_register(&mut builder, types[1]);
            let c = typed_register(&mut builder, types[2]);
            let result = builder.op(*opcode, a, b, c, 0);
            // Use the result so a mistyped destination would also show up.
            let _ = result;
            let outcome = verify_builder(&builder).map(|_| ());
            assert_eq!(&outcome, expected, "opcode {opcode} with {types:?}");
        }
    }

    #[test]
    fn i32_reads_sign_extend_into_i64() {
        // A fixed-offset i32 read is typed i64: it adds to an i64 and not to a u64.
        let mut builder = ProgramBuilder::new();
        let feed = builder.account(0, None, Some([7; 32]), 8);
        let exponent = builder.read(OP_READ_I32, feed, 4);
        let one = builder.const_i64(1);
        builder.binary(OP_ADD, exponent, one);
        assert_eq!(verify_builder(&builder).map(|_| ()), Ok(()));

        let mut builder = ProgramBuilder::new();
        let feed = builder.account(0, None, Some([7; 32]), 8);
        let exponent = builder.read(OP_READ_I32, feed, 4);
        let one = builder.const_u64(1);
        builder.binary(OP_ADD, exponent, one);
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(TemplateError::TypeMismatch));

        // The four bytes must fit the declared minimum data length, as for every fixed read.
        let mut builder = ProgramBuilder::new();
        let feed = builder.account(0, None, Some([7; 32]), 8);
        builder.read(OP_READ_I32, feed, 5);
        assert!(matches!(verify_builder(&builder), Err(TemplateError::ReadOutOfBounds(_))));

        // As a return-data width selector it is typed i64 too.
        assert_eq!(read_width(OP_READ_I32), 4);
    }
```

Check the `account(...)` signature in `builder.rs` before relying on it. The owner pin is what
makes the read legal; the last argument is the minimum data length. If the helper's argument
order differs, adapt the calls, not the assertions.

- [ ] **Step 2: Run the tests and confirm they fail.**

Run: `cargo test -p ballista-common --lib verify`
Expected: compile errors. `OP_REM`, `OP_SHL`, `OP_SHR`, `OP_BIT_AND`, `OP_BIT_OR`, `OP_BIT_XOR`,
`OP_POW10`, `OP_MUL_DIV`, `OP_MUL_DIV_CEIL` and `OP_READ_I32` are not defined.

- [ ] **Step 3: Add the constants.** In `wire.rs`, directly after the `OP_CREATE_PDA`
  definition:

```rust
/// `a × b ÷ c` for three `u64`s or three `u128`s, rounded down, with the product computed exactly.
pub const OP_MUL_DIV: u8 = 51;
/// As `OP_MUL_DIV`, rounded up.
pub const OP_MUL_DIV_CEIL: u8 = 52;
/// `a mod b` for matching `u64`, `i64` or `u128`; the result takes the dividend's sign.
pub const OP_REM: u8 = 53;
/// `a << b` for a `u64` or `u128` `a` and a `u64` `b`; fails rather than drop a set bit.
pub const OP_SHL: u8 = 54;
/// `a >> b`, rounding down; a shift of the full width or more gives zero.
pub const OP_SHR: u8 = 55;
pub const OP_BIT_AND: u8 = 56;
pub const OP_BIT_OR: u8 = 57;
pub const OP_BIT_XOR: u8 = 58;
/// `10^a` for a `u64` `a` of at most 38, as a `u128`.
pub const OP_POW10: u8 = 59;
/// A four-byte signed read, sign-extended into an `i64` register.
pub const OP_READ_I32: u8 = 60;
```

- [ ] **Step 4: Add the verifier rules.** In `verify.rs`:

  (a) In `read_width`, change `OP_READ_U32 => 4,` to:

```rust
        OP_READ_U32 | OP_READ_I32 => 4,
```

  (b) In `verify_instruction`, add `| OP_READ_I32` to the READ arm's pattern. In that arm's
  `value_type` match, change `OP_READ_I64 => VALUE_I64,` to
  `OP_READ_I64 | OP_READ_I32 => VALUE_I64,`. Make the same change in the RETURN_DATA arm's
  `value_type` match; there the scrutinee is `instruction.a`.

  (c) Directly after the `OP_ADD | OP_SUB | OP_MUL | OP_DIV | OP_MIN | OP_MAX` arm, add:

```rust
            OP_REM => {
                let left = self.read_register(registers, instruction.a)?;
                let right = self.read_register(registers, instruction.b)?;
                if left != right || !left.is_numeric() {
                    return Err(TemplateError::TypeMismatch);
                }
                self.write_register(registers, instruction.dst, left)?;
            }
            OP_BIT_AND | OP_BIT_OR | OP_BIT_XOR => {
                let left = self.read_register(registers, instruction.a)?;
                let right = self.read_register(registers, instruction.b)?;
                if left != right || !matches!(left.value_type, VALUE_U64 | VALUE_U128) {
                    return Err(TemplateError::TypeMismatch);
                }
                self.write_register(registers, instruction.dst, left)?;
            }
            OP_SHL | OP_SHR => {
                let value = self.read_register(registers, instruction.a)?;
                self.require_type(registers, instruction.b, VALUE_U64)?;
                if !matches!(value.value_type, VALUE_U64 | VALUE_U128) {
                    return Err(TemplateError::TypeMismatch);
                }
                self.write_register(registers, instruction.dst, value)?;
            }
            OP_MUL_DIV | OP_MUL_DIV_CEIL => {
                let a = self.read_register(registers, instruction.a)?;
                let b = self.read_register(registers, instruction.b)?;
                let c = self.read_register(registers, instruction.c)?;
                if a != b || a != c || !matches!(a.value_type, VALUE_U64 | VALUE_U128) {
                    return Err(TemplateError::TypeMismatch);
                }
                self.write_register(registers, instruction.dst, a)?;
            }
            OP_POW10 => {
                self.require_type(registers, instruction.a, VALUE_U64)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_U128))?;
            }
```

In the `(OP_SHL, Some(VALUE_U64), None, …)` case, the uninitialized `b` must report
`RegisterNotInitialized(1)`. That holds because `require_type` reads the register first. Confirm
by running the test, and if the order differs, match the arithmetic arm's order (read both
registers first).

- [ ] **Step 5: Run the tests and confirm they pass.**

Run: `cargo test -p ballista-common --lib verify`
Expected: PASS, including `every_opcode_rejects_uninitialized_or_mistyped_operands`,
`multiply_divide_takes_three_matching_unsigned_operands` and `i32_reads_sign_extend_into_i64`.

- [ ] **Step 6: Commit.**

```bash
git add common/src/template/wire.rs common/src/template/verify.rs
git commit -m "Verify ten math opcodes: multiply-divide, remainder, shifts, bitwise, powers of ten, i32 reads"
```

---

### Task 2: The math module

**Files:**
- Create: `programs/ballista/src/processor/math.rs`
- Modify: `programs/ballista/src/processor/mod.rs`

- [ ] **Step 1: Declare the module.** Replace the body of `processor/mod.rs` with:

```rust
#[cfg(not(feature = "spec-api"))]
mod execute;
#[cfg(feature = "spec-api")]
pub mod execute;

#[cfg(not(feature = "spec-api"))]
mod math;
#[cfg(feature = "spec-api")]
pub mod math;

pub use execute::run;
```

- [ ] **Step 2: Write the module with its tests first.** Create
  `programs/ballista/src/processor/math.rs`. The tests below define the behaviour; the
  functions follow in Step 4.

```rust
//! Integer operations beyond checked add, subtract, multiply and divide: multiply-then-divide
//! with an exact 256-bit product, remainder, shifts, bitwise operations and powers of ten. Each
//! returns the exact result or fails; none wraps, saturates or truncates silently.

use ballista_common::template::{OP_BIT_AND, OP_BIT_OR, OP_REM, OP_SHL, OP_SHR};

use super::execute::{RunError, RunResult, RuntimeValue};
use crate::error::BallistaError;

// (functions go here, Step 4)

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
```

The `let _ = OP_MUL_DIV;` line only keeps the import used if you later trim the test. Delete the
import and that line together if clippy flags them.

- [ ] **Step 3: Run the tests and confirm they fail.**

Run: `cargo test -p ballista --lib math`
Expected: compile errors. `widening_mul`, `mul_div_u128`, `mul_div`, `integer` and `pow10` are
not defined.

- [ ] **Step 4: Implement.** Replace `// (functions go here, Step 4)` with:

```rust
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
```

`integer`'s `_` arm sends every other opcode to `bitwise`. The dispatch arm (Task 3) only calls
`integer` for `OP_REM | OP_SHL | OP_SHR | OP_BIT_AND | OP_BIT_OR | OP_BIT_XOR`, and `bitwise`
treats anything that is not AND or OR as XOR. That is sound only because of that guarded call
site; the doc comment on `integer` says so.

`RunError`, `RunResult` and `RuntimeValue` must be `pub` in `execute.rs`. They already are,
because the Certora specs import them. `RunError: From<BallistaError>` exists (`execute.rs`,
around line 40).

- [ ] **Step 5: Run the tests and confirm they pass.**

Run: `cargo test -p ballista --lib math`
Expected: PASS, all eight tests.

- [ ] **Step 6: Commit.**

```bash
git add programs/ballista/src/processor/math.rs programs/ballista/src/processor/mod.rs
git commit -m "Add exact multiply-divide, remainder, shifts, bitwise and powers of ten to the executor"
```

---

### Task 3: Dispatch and the i32 read

**Files:** modify `programs/ballista/src/processor/execute.rs`:
- the imports at the top;
- the read arm (around line 849);
- the arithmetic arm (around line 875);
- `decode_value` (around line 1104);
- the tests module.

- [ ] **Step 1: Write the failing test.** In the `#[cfg(test)] mod tests` of `execute.rs`, add:

```rust
    #[test]
    fn i32_reads_sign_extend() {
        let data = [0xf8, 0xff, 0xff, 0xff, 0x2a, 0x00, 0x00, 0x00];
        assert_eq!(read_value(OP_READ_I32, &data, 0), Ok(RuntimeValue::I64(-8)));
        assert_eq!(read_value(OP_READ_I32, &data, 4), Ok(RuntimeValue::I64(42)));
        assert!(read_value(OP_READ_I32, &data, 5).is_err());
    }
```

- [ ] **Step 2: Run it and confirm it fails.**

Run: `cargo test -p ballista --lib i32_reads_sign_extend`
Expected: FAIL, because `decode_value` returns `InvalidTemplateProgram` for opcode 60.

- [ ] **Step 3: Implement.**

  (a) In `decode_value`, after the `OP_READ_I64` arm:

```rust
        OP_READ_I32 => sink(RuntimeValue::I64(
            i32::from_le_bytes(*read_array(data, offset)?) as i64,
        )),
```

  (b) Add `| OP_READ_I32` to the pattern of the `OP_READ_U8 | OP_READ_U16 | …` arm in
  `execute_instruction`.

  (c) After the `OP_ADD | OP_SUB | OP_MUL | OP_DIV | OP_MIN | OP_MAX` arm in
  `execute_instruction`, add:

```rust
        OP_REM | OP_SHL | OP_SHR | OP_BIT_AND | OP_BIT_OR | OP_BIT_XOR => {
            let left = operand(registers, instruction.a)?;
            let right = operand(registers, instruction.b)?;
            let value = math::integer(instruction.opcode, *left, *right)
                .map_err(|error| unset_first(error, left, right))?;
            set(registers, dst, value)?;
        }
        OP_MUL_DIV | OP_MUL_DIV_CEIL => {
            let a = operand(registers, instruction.a)?;
            let b = operand(registers, instruction.b)?;
            let c = operand(registers, instruction.c)?;
            let value = math::mul_div(instruction.opcode == OP_MUL_DIV_CEIL, *a, *b, *c)
                .map_err(|error| unset_first(unset_first(error, a, b), c, c))?;
            set(registers, dst, value)?;
        }
        OP_POW10 => {
            let exponent = operand(registers, instruction.a)?;
            let value = math::pow10(*exponent)
                .map_err(|error| unset_first(error, exponent, exponent))?;
            set(registers, dst, value)?;
        }
```

  (d) Add `use super::math;` next to the other `use` lines at the top of `execute.rs`. If
  `ballista_common::template::*` is not glob-imported there, also import the new `OP_*`
  constants.

- [ ] **Step 4: Run the tests.**

Run: `cargo test -p ballista --lib`
Expected: PASS, including `i32_reads_sign_extend` and every existing executor test.

- [ ] **Step 5: Build for SBF and check the frame size.**

Run: `cargo build-sbf --manifest-path programs/ballista/Cargo.toml 2>&1 | grep -i -E "stack|frame|error" || true`
Expected: no new "Stack offset … exceeded" warnings. `mul_div` is `#[inline(never)]` so its
locals get their own frame.

- [ ] **Step 6: Commit.**

```bash
git add programs/ballista/src/processor/execute.rs
git commit -m "Dispatch the math opcodes and read sign-extended i32 fields"
```

---

### Task 4: Builder helpers and the program generator

**Files:**
- Modify: `common/src/template/builder.rs`, after `cast` (around line 230).
- Modify: `common/src/template/generate.rs`: `emit_operation` (247–388).

- [ ] **Step 1: Add the builder helpers.**

```rust
    /// `a × b ÷ c` with the product computed exactly, rounded down. All three share a type,
    /// `u64` or `u128`.
    pub fn mul_div(&mut self, a: u8, b: u8, c: u8) -> u8 {
        self.op(OP_MUL_DIV, a, b, c, 0)
    }

    /// `a × b ÷ c`, rounded up.
    pub fn mul_div_ceil(&mut self, a: u8, b: u8, c: u8) -> u8 {
        self.op(OP_MUL_DIV_CEIL, a, b, c, 0)
    }

    /// `10^exponent` as a `u128`, from a `u64` register.
    pub fn pow10(&mut self, exponent: u8) -> u8 {
        self.op(OP_POW10, exponent, NO_INDEX, NO_INDEX, 0)
    }
```

Update the doc comment on `binary` to read:
`/// Emits a two-operand instruction: arithmetic, remainder, shift, bitwise, comparison, or boolean.`

- [ ] **Step 2: Extend the generator.** In `emit_operation`, change `match choices.below(12)`
  to `match choices.below(16)`. Insert these arms before the final `_ =>` arm:

```rust
        11 => {
            // Remainder on any numeric type; bitwise operations on unsigned ones.
            let numeric = registers.numeric();
            if let Some(left) = choices.pick(&numeric) {
                let kind = registers.type_of(left);
                let same = registers.of_type(kind);
                let right = same[choices.below(same.len())];
                let opcode = if kind == VALUE_I64 {
                    OP_REM
                } else {
                    [OP_REM, OP_BIT_AND, OP_BIT_OR, OP_BIT_XOR][choices.below(4)]
                };
                let register = builder.binary(opcode, left, right);
                registers.push(register, kind);
            }
        }
        12 => {
            let unsigned: Vec<u8> = registers
                .numeric()
                .into_iter()
                .filter(|register| registers.type_of(*register) != VALUE_I64)
                .collect();
            if let Some(value) = choices.pick(&unsigned) {
                let kind = registers.type_of(value);
                // Amounts past the width exercise the zero and overflow rules too.
                let bits = builder.const_u64(choices.below(140) as u64);
                registers.push(bits, VALUE_U64);
                let opcode = if choices.below(2) == 0 { OP_SHL } else { OP_SHR };
                let register = builder.binary(opcode, value, bits);
                registers.push(register, kind);
            }
        }
        13 => {
            let unsigned: Vec<u8> = registers
                .numeric()
                .into_iter()
                .filter(|register| registers.type_of(*register) != VALUE_I64)
                .collect();
            if let Some(a) = choices.pick(&unsigned) {
                let kind = registers.type_of(a);
                let same = registers.of_type(kind);
                let b = same[choices.below(same.len())];
                let c = same[choices.below(same.len())];
                let register = if choices.below(2) == 0 {
                    builder.mul_div(a, b, c)
                } else {
                    builder.mul_div_ceil(a, b, c)
                };
                registers.push(register, kind);
            }
        }
        14 => {
            // Exponents past 38 overflow, which is an allowed value-dependent failure.
            let exponent = builder.const_u64(choices.below(45) as u64);
            registers.push(exponent, VALUE_U64);
            let register = builder.pow10(exponent);
            registers.push(register, VALUE_U128);
        }
```

Arm 12 pushes two registers. The `registers.len() >= 56` guard at the top of `emit_operation`
leaves headroom for that; keep it.

- [ ] **Step 3: Run the generator properties.**

Run: `cargo test -p ballista-common --features proptest --test generated`
Expected: PASS. Generated programs parse, verify, and report the stats the generator recorded.

Run: `cargo build-sbf --manifest-path programs/ballista/Cargo.toml && cargo test --manifest-path tests/ballista/Cargo.toml generated_programs_never_hit_structural_errors`
Expected: PASS. The new ops fail only with `ArithmeticOverflow` (6013) or `DivisionByZero`
(6014), which `ALLOWED_RUNTIME_ERRORS` already permits.

- [ ] **Step 4: Commit.**

```bash
git add common/src/template/builder.rs common/src/template/generate.rs
git commit -m "Build and generate programs with the new math opcodes"
```

---

### Task 5: Rust ↔ TypeScript opcode parity

**Files:** create `clients/js/src/opcodes.test.ts`.

- [ ] **Step 1: Write the test.** It fails now: Rust has 51–60 and TypeScript does not.

```ts
/**
 * The compiler's opcode table is a copy of the constants in `common/src/template/wire.rs`. This
 * pairs every Rust opcode with its TypeScript name and checks the numbers agree, so a change on
 * either side that the other does not follow fails here rather than as a verifier rejection.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { expect, test } from 'vitest';

import { opcode } from './compiler.js';

const wire = readFileSync(
  fileURLToPath(new URL('../../../common/src/template/wire.rs', import.meta.url)),
  'utf8',
);
const rust = new Map(
  [...wire.matchAll(/pub const (OP_[A-Z0-9_]+): u8 = (\d+);/g)].map(([, name, value]) => [
    name!,
    Number(value),
  ]),
);

const rustName: Record<keyof typeof opcode, string> = {
  loadInput: 'OP_LOAD_INPUT',
  constBool: 'OP_CONST_BOOL',
  constU64: 'OP_CONST_U64',
  constI64: 'OP_CONST_I64',
  constU128: 'OP_CONST_U128',
  constPubkey: 'OP_CONST_PUBKEY',
  constBytes: 'OP_CONST_BYTES',
  accountKey: 'OP_ACCOUNT_KEY',
  accountOwner: 'OP_ACCOUNT_OWNER',
  accountLamports: 'OP_ACCOUNT_LAMPORTS',
  accountDataLength: 'OP_ACCOUNT_DATA_LEN',
  accountIsEmpty: 'OP_ACCOUNT_IS_EMPTY',
  readU64: 'OP_READ_U64',
  readI64: 'OP_READ_I64',
  readU128: 'OP_READ_U128',
  readPubkey: 'OP_READ_PUBKEY',
  clockSlot: 'OP_CLOCK_SLOT',
  clockTimestamp: 'OP_CLOCK_TIMESTAMP',
  add: 'OP_ADD',
  subtract: 'OP_SUB',
  multiply: 'OP_MUL',
  divide: 'OP_DIV',
  equal: 'OP_EQ',
  notEqual: 'OP_NE',
  lessThan: 'OP_LT',
  lessThanOrEqual: 'OP_LTE',
  greaterThan: 'OP_GT',
  greaterThanOrEqual: 'OP_GTE',
  and: 'OP_AND',
  or: 'OP_OR',
  not: 'OP_NOT',
  min: 'OP_MIN',
  max: 'OP_MAX',
  select: 'OP_SELECT',
  castU64: 'OP_CAST_U64',
  castI64: 'OP_CAST_I64',
  castU128: 'OP_CAST_U128',
  loopIndex: 'OP_LOOP_INDEX',
  require: 'OP_REQUIRE',
  invoke: 'OP_INVOKE',
  forEach: 'OP_FOREACH',
  readU8: 'OP_READ_U8',
  readU16: 'OP_READ_U16',
  readU32: 'OP_READ_U32',
  readBool: 'OP_READ_BOOL',
  derivePda: 'OP_DERIVE_PDA',
  returnData: 'OP_RETURN_DATA',
  move: 'OP_MOVE',
  createPda: 'OP_CREATE_PDA',
  mulDiv: 'OP_MUL_DIV',
  mulDivCeil: 'OP_MUL_DIV_CEIL',
  remainder: 'OP_REM',
  shiftLeft: 'OP_SHL',
  shiftRight: 'OP_SHR',
  bitAnd: 'OP_BIT_AND',
  bitOr: 'OP_BIT_OR',
  bitXor: 'OP_BIT_XOR',
  powerOfTen: 'OP_POW10',
  readI32: 'OP_READ_I32',
};

test('every opcode has the same number in Rust and TypeScript', () => {
  for (const [key, name] of Object.entries(rustName) as [keyof typeof opcode, string][]) {
    expect(rust.get(name), `${key} ↔ ${name}`).toBe(opcode[key]);
  }
  expect([...rust.keys()].sort()).toEqual(Object.values(rustName).sort());
});
```

- [ ] **Step 2: Run it and confirm it fails.**

Run: `pnpm --dir clients/js exec vitest run src/opcodes.test.ts`
Expected: a type error or failure, because `mulDiv` and the other new keys are missing from
`opcode`.

- [ ] **Step 3: Add the entries** to `export const opcode` in `clients/js/src/compiler.ts`, after
  `createPda: 50,`:

```ts
  mulDiv: 51,
  mulDivCeil: 52,
  remainder: 53,
  shiftLeft: 54,
  shiftRight: 55,
  bitAnd: 56,
  bitOr: 57,
  bitXor: 58,
  powerOfTen: 59,
  readI32: 60,
```

- [ ] **Step 4: Run it again.**

Run: `pnpm --dir clients/js exec vitest run src/opcodes.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add clients/js/src/opcodes.test.ts clients/js/src/compiler.ts
git commit -m "Check the TypeScript opcode table against the Rust constants"
```

---

### Task 6: TypeScript expressions

**Files:**
- Modify: `clients/js/src/schema.ts`:
  - `ReadTypeSchema` (21);
  - the `Expression` type (81–130);
  - `ExpressionSchema` (132–210);
  - the `expression` constructors (405–453).
- Modify: `clients/js/src/compiler.ts`:
  - `readOpcode`, `readWidth` and `readResultType` (150–181);
  - `compileExpression` (634–776).
- Test: `clients/js/src/compiler.test.ts`.

- [ ] **Step 1: Write the failing tests.** `compiler.test.ts` already defines `HEADER_LENGTH`,
  `ACCOUNT_RECORD_LENGTH` (8), `INSTRUCTION_LENGTH` (16) and `instructionOffset(compiled, pc)`.
  Add `opcode`, `type Step` and `type TemplateInput` to its import from `./index.js`, then append:

```ts
/** Every 16-byte instruction record of a compiled payload. */
function records(compiled: CompiledTemplate): Uint8Array[] {
  return Array.from({ length: compiled.stats.instructions }, (_, pc) => {
    const offset = instructionOffset(compiled, pc);
    return compiled.bytes.slice(offset, offset + INSTRUCTION_LENGTH);
  });
}

/** The minimum data length recorded for fixed account `index`: a u32 at byte 4 of its record. */
function minDataLength(compiled: CompiledTemplate, index: number): number {
  const view = new DataView(compiled.bytes.buffer, compiled.bytes.byteOffset);
  return view.getUint32(HEADER_LENGTH + index * ACCOUNT_RECORD_LENGTH + 4, true);
}

describe('math expressions', () => {
  const compileSteps = (inputs: TemplateInput['inputs'], steps: Step[]) =>
    compileTemplate(defineTemplate({ inputs, accounts: {}, steps }));

  test('multiplyDivide lowers to one three-operand record, rounding by opcode', () => {
    const compiled = compileSteps(
      { a: { type: 'u64' }, b: { type: 'u64' }, c: { type: 'u64' } },
      [
        step.let('down', expression.multiplyDivide(expression.input('a'), expression.input('b'), expression.input('c'))),
        step.let('up', expression.multiplyDivide(expression.input('a'), expression.input('b'), expression.input('c'), 'up')),
        step.require(expression.lessThanOrEqual(expression.variable('down'), expression.variable('up'))),
      ],
    );
    const mulDivs = records(compiled).filter(
      (record) => record[0] === opcode.mulDiv || record[0] === opcode.mulDivCeil,
    );
    expect(mulDivs.map((record) => record[0])).toEqual([opcode.mulDiv, opcode.mulDivCeil]);
    // Operands a, b, c are the three hoisted input loads, registers 0 to 2.
    expect([...mulDivs[0]!.slice(2, 5)]).toEqual([0, 1, 2]);
  });

  test('multiplyDivide rejects mixed or signed operands', () => {
    expect(() =>
      compileSteps({ a: { type: 'u64' }, b: { type: 'u128' } }, [
        step.let('x', expression.multiplyDivide(expression.input('a'), expression.input('a'), expression.input('b'))),
      ]),
    ).toThrow(/multiplyDivide/);
    expect(() =>
      compileSteps({ a: { type: 'i64' } }, [
        step.let('x', expression.multiplyDivide(expression.input('a'), expression.input('a'), expression.input('a'))),
      ]),
    ).toThrow(/multiplyDivide/);
  });

  test('shifts take an unsigned value and a u64 amount; bitwise ops need matching unsigned operands', () => {
    expect(() =>
      compileSteps({ a: { type: 'u128' }, n: { type: 'u64' } }, [
        step.let('x', expression.shiftLeft(expression.input('a'), expression.input('n'))),
        step.let('y', expression.shiftRight(expression.input('a'), expression.input('n'))),
        step.let('z', expression.bitXor(expression.input('a'), expression.variable('x'))),
      ]),
    ).not.toThrow();
    expect(() =>
      compileSteps({ a: { type: 'i64' }, n: { type: 'u64' } }, [
        step.let('x', expression.shiftLeft(expression.input('a'), expression.input('n'))),
      ]),
    ).toThrow(/shiftLeft/);
    expect(() =>
      compileSteps({ a: { type: 'u64' }, b: { type: 'u128' } }, [
        step.let('x', expression.bitAnd(expression.input('a'), expression.input('b'))),
      ]),
    ).toThrow(/bitAnd/);
  });

  test('remainder works on every numeric type; powerOfTen takes a u64 and yields a u128', () => {
    expect(() =>
      compileSteps({ a: { type: 'i64' }, e: { type: 'u64' } }, [
        step.let('r', expression.remainder(expression.input('a'), expression.input('a'))),
        step.require(expression.equal(expression.powerOfTen(expression.input('e')), expression.u128(1_000n))),
      ]),
    ).not.toThrow();
    expect(() =>
      compileSteps({ a: { type: 'u128' } }, [step.let('x', expression.powerOfTen(expression.input('a')))]),
    ).toThrow(/powerOfTen/);
  });

  test('an i32 read is typed i64 and raises the data floor to cover its four bytes', () => {
    const compiled = compileTemplate(
      defineTemplate({
        accounts: { feed: { owner: address(7) } },
        steps: [
          step.require(
            expression.lessThan(expression.accountData(account.fixed('feed'), 89, 'i32'), expression.i64(0)),
          ),
        ],
      }),
    );
    expect(records(compiled).some((record) => record[0] === opcode.readI32)).toBe(true);
    expect(minDataLength(compiled, 0)).toBe(93);
  });
});
```

- [ ] **Step 2: Run them and confirm they fail.**

Run: `pnpm --dir clients/js exec vitest run src/compiler.test.ts -t "math expressions"`
Expected: FAIL, because `expression.multiplyDivide` and the other new constructors do not exist.

- [ ] **Step 3: Extend the schema** in `schema.ts`.

  - `ReadTypeSchema`:

    ```ts
    export const ReadTypeSchema = z.enum(['bool', 'u8', 'u16', 'u32', 'i32', 'u64', 'i64', 'u128', 'pubkey']);
    ```

  - In the `Expression` type, extend the binary `op` union with
    `| 'remainder' | 'shiftLeft' | 'shiftRight' | 'bitAnd' | 'bitOr' | 'bitXor'`, and add two
    variants before the `not` variant:

    ```ts
      | {
          /** `left × right ÷ divisor` with the product computed exactly. */
          kind: 'multiplyDivide';
          left: Expression;
          right: Expression;
          divisor: Expression;
          rounding: 'down' | 'up';
        }
      | { kind: 'powerOfTen'; exponent: Expression }
    ```

  - In `ExpressionSchema`, add `'remainder', 'shiftLeft', 'shiftRight', 'bitAnd', 'bitOr',
    'bitXor'` to the binary `z.enum`, and add:

    ```ts
        z
          .object({
            kind: z.literal('multiplyDivide'),
            left: ExpressionSchema,
            right: ExpressionSchema,
            divisor: ExpressionSchema,
            rounding: z.enum(['down', 'up']),
          })
          .strict(),
        z.object({ kind: z.literal('powerOfTen'), exponent: ExpressionSchema }).strict(),
    ```

  - In `expression`, after `max: binary('max'),`:

    ```ts
      remainder: binary('remainder'),
      shiftLeft: binary('shiftLeft'),
      shiftRight: binary('shiftRight'),
      bitAnd: binary('bitAnd'),
      bitOr: binary('bitOr'),
      bitXor: binary('bitXor'),
      multiplyDivide: (
        left: Expression,
        right: Expression,
        divisor: Expression,
        rounding: 'down' | 'up' = 'down',
      ): Expression => ({ kind: 'multiplyDivide', left, right, divisor, rounding }),
      powerOfTen: (exponent: Expression): Expression => ({ kind: 'powerOfTen', exponent }),
    ```

- [ ] **Step 4: Extend the compiler** in `compiler.ts`.

  - Add `i32: opcode.readI32` to `readOpcode`, `i32: 4` to `readWidth`, and `i32: 'i64'` to
    `readResultType`.

  - In `compileExpression`, before the `cast` branch:

    ```ts
        if (current.kind === 'multiplyDivide') {
          const left = this.compileExpression(current.left, inLoop, bindings);
          const right = this.compileExpression(current.right, inLoop, bindings);
          const divisor = this.compileExpression(current.divisor, inLoop, bindings);
          if (
            left.type !== right.type ||
            left.type !== divisor.type ||
            (left.type !== 'u64' && left.type !== 'u128')
          ) {
            throw new TypeError('multiplyDivide requires three u64 or three u128 operands');
          }
          const operation = current.rounding === 'up' ? opcode.mulDivCeil : opcode.mulDiv;
          return this.emit(operation, left.type, 0, left.register, right.register, divisor.register);
        }
        if (current.kind === 'powerOfTen') {
          const exponent = this.compileExpression(current.exponent, inLoop, bindings);
          requireType(exponent, 'u64', 'powerOfTen');
          return this.emit(opcode.powerOfTen, 'u128', 0, exponent.register);
        }
    ```

  - In the binary section, directly after `const operation = opcode[current.op];`:

    ```ts
        if (current.op === 'remainder') {
          if (left.type !== right.type || !isNumeric(left.type)) {
            throw new TypeError('remainder requires matching numeric types');
          }
          return this.emit(operation, left.type, 0, left.register, right.register);
        }
        if (current.op === 'shiftLeft' || current.op === 'shiftRight') {
          if (left.type !== 'u64' && left.type !== 'u128') {
            throw new TypeError(`${current.op} requires a u64 or u128 value`);
          }
          requireType(right, 'u64', `${current.op} amount`);
          return this.emit(operation, left.type, 0, left.register, right.register);
        }
        if (current.op === 'bitAnd' || current.op === 'bitOr' || current.op === 'bitXor') {
          if (left.type !== right.type || (left.type !== 'u64' && left.type !== 'u128')) {
            throw new TypeError(`${current.op} requires matching u64 or u128 operands`);
          }
          return this.emit(operation, left.type, 0, left.register, right.register);
        }
    ```

    Check `requireType`'s message format (`compiler.ts:942-944`). If it does not include the
    label, the `/shiftLeft/` assertion in the test still holds only through the explicit
    `throw` above. Adjust the test's regex rather than the message if needed.

- [ ] **Step 5: Run the tests.**

Run: `pnpm --dir clients/js check`
Expected: PASS. That covers tsc for source and examples, plus every vitest file, including
`opcodes.test.ts` and "math expressions".

- [ ] **Step 6: Commit.**

```bash
git add clients/js/src/schema.ts clients/js/src/compiler.ts clients/js/src/compiler.test.ts
git commit -m "Author the math opcodes from TypeScript"
```

---

### Task 7: The `math-ops` fixture, end to end

**Files:**
- `clients/js/src/fixtures.test.ts`: the `fixtures` map.
- `common/src/template/verify.rs`: `every_shared_fixture_parses_and_verifies`.
- `tests/ballista/src/lib.rs`: `fixture()`, plus a new test.

- [ ] **Step 1: Add the fixture.** In `fixtures.test.ts`, add this entry to `fixtures` after
  `'dynamic-read'`. Every requirement compares one result with a constant worked out by hand.

```ts
  'math-ops': () =>
    defineTemplate({
      inputs: {
        amount: { type: 'u64' },
        price: { type: 'u64' },
        divisor: { type: 'u64' },
        flags: { type: 'u64' },
        exponent: { type: 'u64' },
      },
      accounts: { feed: { owner: TOKEN_PROGRAM_ADDRESS_BYTES } },
      steps: [
        // 1,000,003 × 7 = 7,000,021, and ÷ 3 is 2,333,340⅓.
        step.require(
          expression.equal(
            expression.multiplyDivide(expression.input('amount'), expression.input('price'), expression.input('divisor')),
            expression.u64(2_333_340),
          ),
          'floor',
        ),
        step.require(
          expression.equal(
            expression.multiplyDivide(expression.input('amount'), expression.input('price'), expression.input('divisor'), 'up'),
            expression.u64(2_333_341),
          ),
          'ceiling',
        ),
        // (2^100 + 1)^2 ÷ 2^90 needs the 256-bit path.
        step.require(
          expression.equal(
            expression.multiplyDivide(
              expression.u128((1n << 100n) + 1n),
              expression.u128((1n << 100n) + 1n),
              expression.u128(1n << 90n),
            ),
            expression.u128((1n << 110n) + 2048n),
          ),
          'wideFloor',
        ),
        step.require(
          expression.equal(expression.remainder(expression.input('amount'), expression.input('divisor')), expression.u64(1)),
          'remainder',
        ),
        step.require(
          expression.equal(
            expression.shiftRight(expression.bitAnd(expression.input('flags'), expression.u64(0xff00)), expression.u64(8)),
            expression.u64(0xab),
          ),
          'highByte',
        ),
        step.require(
          expression.equal(expression.shiftLeft(expression.u64(1), expression.u64(63)), expression.u64(1n << 63n)),
          'topBit',
        ),
        step.require(
          expression.equal(
            expression.bitOr(expression.bitXor(expression.input('flags'), expression.u64(0xffff)), expression.u64(0x000f)),
            expression.u64(0x543f),
          ),
          'xorThenOr',
        ),
        step.require(
          expression.equal(expression.powerOfTen(expression.input('exponent')), expression.u128(10n ** 18n)),
          'powerOfTen',
        ),
        // The feed's amount is 0xfffffff8, whose low four bytes are the i32 −8.
        step.require(
          expression.equal(expression.accountData(account.fixed('feed'), 64, 'i32'), expression.i64(-8)),
          'signedRead',
        ),
      ],
    }),
```

Check the arithmetic before running it. `0xabcd ^ 0xffff = 0x5432`, and `0x5432 | 0x000f = 0x543f`.

- [ ] **Step 2: Regenerate the fixtures and confirm the payload verifies.**

Run: `pnpm fixtures`
Expected: `fixtures/math-ops.hex` is created and `manifest.json` gains an entry.

In `verify.rs`, change `[(&str, &str); 14]` to `[(&str, &str); 15]` and add
`("math-ops", include_str!("../../../fixtures/math-ops.hex")),` to the array.

Run: `cargo test -p ballista-common every_shared_fixture_parses_and_verifies`
Expected: PASS.

- [ ] **Step 3: Write the end-to-end test.** In `tests/ballista/src/lib.rs`, add
  `"math-ops" => include_str!("../../../fixtures/math-ops.hex"),` to `fixture()`. Then add this
  test to `mod tests`:

```rust
    /// The math opcodes as the TypeScript SDK compiles them, run on chain. Each requirement in the
    /// fixture compares one result with a constant worked out by hand, so a run that succeeds has
    /// computed all of them exactly; the failing runs show each failure is the documented one.
    #[test]
    fn typescript_math_fixture_computes_exact_results() {
        let creator = Pubkey::new_unique();
        let authority = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let feed = Pubkey::new_unique();
        let mut accounts = funded_accounts([creator], 10_000_000_000);
        accounts.insert(
            feed,
            token::create_account_for_token_account(token_account_state(mint, authority, 0xffff_fff8)),
        );
        let mut context = context(accounts);
        let payload = fixture("math-ops");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 90, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 90);
        let mut run = |amount: u64, price: u64, divisor: u64, flags: u64, exponent: u64| {
            let mut inputs = Vec::new();
            for value in [amount, price, divisor, flags, exponent] {
                inputs.extend_from_slice(&value.to_le_bytes());
            }
            context.process_instruction(&run_instruction(
                template,
                vec![AccountMeta::new_readonly(feed, false)],
                &inputs,
            ))
        };

        let exact = run(1_000_003, 7, 3, 0xabcd, 18);
        assert!(exact.program_result.is_ok(), "{exact:#?}");

        // A wrong power of ten fails its requirement, not the arithmetic.
        let wrong = run(1_000_003, 7, 3, 0xabcd, 17);
        assert_eq!(decode_kind(&wrong), Some(BallistaError::RequirementFailed as u32));
        // Division by zero and a power of ten past 10^38 fail as themselves.
        let zero = run(1_000_003, 7, 0, 0xabcd, 18);
        assert_eq!(decode_kind(&zero), Some(BallistaError::DivisionByZero as u32));
        let huge = run(1_000_003, 7, 3, 0xabcd, 39);
        assert_eq!(decode_kind(&huge), Some(BallistaError::ArithmeticOverflow as u32));
    }
```

Check how the file refers to error kinds. If `BallistaError` is not imported in `mod tests`, use
the literal codes 6015, 6014 and 6013, which is the file's style elsewhere. `token_account_state`,
`funded_accounts`, `context`, `create_template_instruction`, `find_template_pda`,
`run_instruction` and `decode_kind` already exist in that module.

- [ ] **Step 4: Build and run it.**

Run: `cargo build-sbf --manifest-path programs/ballista/Cargo.toml && cargo test --manifest-path tests/ballista/Cargo.toml typescript_math_fixture_computes_exact_results`
Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add clients/js/src/fixtures.test.ts fixtures/ common/src/template/verify.rs tests/ballista/src/lib.rs
git commit -m "Run a TypeScript-compiled math fixture end to end"
```

---

### Task 8: Formal specifications

**Files:**
- `certora/ballista-specs/src/rules/arithmetic.rs`
- `certora/ballista-specs/run.conf`
- `certora/ballista-specs/run-blocked.conf`

- [ ] **Step 1: Add the rules.** Append to `arithmetic.rs`:

```rust
use ballista::processor::math::{integer, mul_div};

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

/// `mul_div` over `u64` is the rounded quotient of the exact product, failing only for a zero
/// divisor or a quotient past `u64::MAX`. It divides a `u128`, which compiles to a compiler-rt
/// routine the prover does not interpret, so this rule is in the blocked configuration.
#[rule]
pub fn rule_u64_mul_div_is_exact() {
    let a: u64 = nondet();
    let b: u64 = nondet();
    let c: u64 = nondet();
    let round_up: bool = nondet();
    let product = a as u128 * b as u128;
    match mul_div(round_up, RuntimeValue::U64(a), RuntimeValue::U64(b), RuntimeValue::U64(c)) {
        Ok(RuntimeValue::U64(result)) => {
            cvlr_assert!(c != 0);
            let floor = product / c as u128;
            let rounded = if round_up && product % c as u128 != 0 { floor + 1 } else { floor };
            cvlr_assert!(result as u128 == rounded);
        }
        Err(RunError::Vm(BallistaError::DivisionByZero)) => cvlr_assert!(c == 0),
        Err(RunError::Vm(BallistaError::ArithmeticOverflow)) => {
            cvlr_assert!(c != 0);
            cvlr_assert!(product / c as u128 >= u64::MAX as u128);
        }
        _ => cvlr_assert!(false),
    }
}
```

Place the `use` line with the file's other imports at the top rather than mid-file. It compiles
because `processor::math` is `pub` under `spec-api` (Task 2). If the specs crate cannot see it,
check the `spec-api` feature on the `ballista` dependency in `certora/ballista-specs/Cargo.toml`.

- [ ] **Step 2: Register the rules.** In `run.conf`, add
  `"rule_u64_integer_operations_match_rust"` to `"rule"`. In `run-blocked.conf`, add
  `"rule_u64_mul_div_is_exact"`.

- [ ] **Step 3: Typecheck the specs.** This is the CI job; the prover itself needs a key and is
  not run here.

Run: `cargo check -p ballista-specs --features rt --manifest-path certora/Cargo.toml`
Expected: PASS.

If the `cargo-certora-sbf` tool is installed, also check frames:
`cargo certora-sbf --tools-version v1.53 --no-build 2>&1 | grep -i "frame"`. Nothing may exceed
4 KiB. `mul_div`'s temporaries are a few `u128`s and well under that.

- [ ] **Step 4: Commit.**

```bash
git add certora/ballista-specs
git commit -m "Specify the u64 integer operations and multiply-divide for the prover"
```

---

### Task 9: Compute-unit ceilings

- [ ] **Step 1: Run the full Mollusk suite**, which includes the ratchets.

Run: `cargo build-sbf --manifest-path programs/ballista/Cargo.toml && cargo test --manifest-path tests/ballista/Cargo.toml`
Expected: PASS. If `ceilings` or `measure_every_example` fails, the new dispatch arms changed the
cost of existing paths.

- [ ] **Step 2: If a ceiling fails**, find out why before changing anything:
  - Compare the reported units with `fixtures/cu-ceilings.json` or
    `fixtures/example-ceilings.json`.
  - If the rise is real and small (under about 1% of the case), raise that entry by hand to the
    measured value plus the file's 64-unit slack, in this task's commit. Put the before and after
    numbers in the commit message.
  - If it is larger, first check whether a new handler got inlined into `dispatch` (look for
    missing `#[inline(never)]`).
  - Never run a script that raises every ceiling at once.

- [ ] **Step 3: Commit if anything changed.**

```bash
git add fixtures/cu-ceilings.json fixtures/example-ceilings.json
git commit -m "Raise the ceilings the new dispatch arms cost: <case>: <old> → <new>"
```

---

### Task 10: The oracle example computes its own scale

**Files:**
- `clients/js/examples/protocols/jupiter-oracle-checked-swap.ts`
- `clients/js/examples/protocols/shared.ts`
- `clients/js/src/protocol-semantics.test.ts`
- `fixtures/protocol-examples.json`

- [ ] **Step 1: Write the failing test.** In `protocol-semantics.test.ts`:
  - Add `'multiplyDivide'` and `'powerOfTen'` cases to the `dependsOn` switch.
  - Add a test to the `describe('the oracle-checked swap', …)` block.

```ts
    case 'multiplyDivide':
      return recurse(expression.left) || recurse(expression.right) || recurse(expression.divisor);
    case 'powerOfTen':
      return recurse(expression.exponent);
```

```ts
  test('scales by both mints’ decimals, read on chain rather than supplied', () => {
    expect(dependsOn(check.condition, bindings, reads('sourceMint', SPL_MINT.decimals))).toBe(true);
    expect(dependsOn(check.condition, bindings, reads('destinationMint', SPL_MINT.decimals))).toBe(true);
    expect(Object.keys(jupiterOracleCheckedSwap.inputs ?? {})).not.toContain('scaleDivisor');
  });
```

Import `SPL_MINT` from `../examples/protocols/shared.js`.

Run: `pnpm --dir clients/js exec vitest run src/protocol-semantics.test.ts`
Expected: FAIL, because `SPL_MINT` does not exist and the template does not read decimals.

- [ ] **Step 2: Add the mint layout** to `shared.ts`, after `TOKEN_ACCOUNT_LENGTH`:

```ts
/** SPL Token account: the mint is the first field. */
export const TOKEN_ACCOUNT_MINT_OFFSET = 0;

/** SPL Token `Mint`: `decimals` is the u8 at offset 44 of the 82-byte layout. */
export const SPL_MINT = { length: 82, decimals: 44 } as const;
```

- [ ] **Step 3: Rewrite the template's scaling.**
  - Remove the `priceExponent` and `scaleDivisor` inputs.
  - Add `sourceMint` and `destinationMint` accounts, each
    `{ owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: SPL_MINT.length }`.
  - Replace the exponent requirement and the `fairOut` binding with:

```ts
    // Each token account must hold the mint whose decimals scale it.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('sourceAta'), TOKEN_ACCOUNT_MINT_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('sourceMint'), 'key'),
      ),
      'sourceHoldsTheSourceMint',
    ),
    step.require(
      expression.equal(
        expression.accountData(account.fixed('destinationAta'), TOKEN_ACCOUNT_MINT_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('destinationMint'), 'key'),
      ),
      'destinationHoldsTheDestinationMint',
    ),

    // price × 10^exponent is per whole token. In base units the fill is worth
    // sold × price × 10^(destinationDecimals + exponent − sourceDecimals). The exponent is an
    // i32 and usually negative, so split the power into a multiplier and a divisor, each ≥ 0,
    // and let multiplyDivide apply both exactly.
    step.let(
      'scale',
      expression.subtract(
        expression.add(
          expression.cast('i64', expression.accountData(account.fixed('destinationMint'), SPL_MINT.decimals, 'u8')),
          expression.accountData(account.fixed('priceUpdate'), PYTH.exponent, 'i32'),
        ),
        expression.cast('i64', expression.accountData(account.fixed('sourceMint'), SPL_MINT.decimals, 'u8')),
      ),
      'computeDecimalScale',
    ),
```

then, after `sold` is measured:

```ts
    step.let(
      'fairOut',
      expression.cast(
        'u64',
        expression.multiplyDivide(
          expression.multiplyDivide(
            expression.multiply(
              expression.cast('u128', expression.variable('sold')),
              expression.cast('u128', expression.variable('oraclePrice')),
            ),
            expression.powerOfTen(expression.cast('u64', expression.max(expression.variable('scale'), expression.i64(0)))),
            expression.powerOfTen(
              expression.cast('u64', expression.max(expression.subtract(expression.i64(0), expression.variable('scale')), expression.i64(0))),
            ),
          ),
          expression.cast('u128', expression.subtract(expression.u64(10_000), expression.input('toleranceBps'))),
          expression.u128(10_000),
        ),
      ),
      'computeOracleFloor',
    ),
```

`sold × price` fits `u128` because both factors are below 2^64. Using `max` with zero means
exactly one of the two powers is 1, so no `select` is needed. That matters because a `select`
evaluates both branches, and the unused one would fail its cast.

Update the header comment: the template now reads the feed's exponent and both mints' decimals,
and the caller supplies only the route and the tolerance. Update the `jupiterCalls` table in
`protocol-semantics.test.ts` only if the Jupiter account list changed; it has not.

- [ ] **Step 4: Run the tests, then regenerate and verify.**

Run: `pnpm --dir clients/js exec vitest run src/protocol-semantics.test.ts`
Expected: PASS.

Run: `pnpm fixtures && cargo test -p ballista-common every_shared_fixture_parses_and_verifies`
Expected: PASS. Only `jupiterOracleCheckedSwap` changes in `protocol-examples.json`.

- [ ] **Step 5: Commit.**

```bash
git add clients/js/examples/protocols clients/js/src/protocol-semantics.test.ts fixtures/protocol-examples.json
git commit -m "Let the oracle-checked swap read the exponent and decimals it scales by"
```

---

### Task 11: Phase verification

- [ ] **Step 1: Run everything CI runs.**

```bash
pnpm install --frozen-lockfile
pnpm fixtures && git diff --exit-code fixtures
pnpm check
pnpm test
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
cargo test --manifest-path tests/ballista/Cargo.toml
cargo check -p ballista-specs --features rt --manifest-path certora/Cargo.toml
cargo clippy -p ballista -p ballista-common --all-targets 2>&1 | grep -E "^(warning|error)" | sort | uniq -c
git diff --check
```

Expected: every command exits 0, and clippy shows no warnings beyond those on the base (Task 0
records the base count).

- [ ] **Step 2: Report.** List the commits, the test counts, any ceiling that moved and why, and
  what remains for the next phase (loops).
