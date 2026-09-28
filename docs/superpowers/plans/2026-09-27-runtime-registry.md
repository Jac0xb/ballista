# Runtime registry: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to carry out this plan task by task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give a template state that outlives a run and that only its own runs can change:
registry entries, accounts Ballista owns, one per template, registry and key. Three opcodes open,
read and write them; they are verified at finalize, executed on chain, authorable from TypeScript
and from the Rust `ProgramBuilder`, covered end to end under Mollusk, and used by a `rateLimit`
SDK helper. The last task runs a per-caller daily cap on a real Jupiter swap.

**Spec:** `docs/superpowers/specs/2026-09-27-template-registry-design.md` (Task 0 commits it).
Every rule it states is implemented here; where this plan decides something the spec leaves
open, or measured a reason to depart from its wording, the decision is listed below and marked
**(user decision)** when the user should confirm it.

**Tech stack:**
- Rust: the no_std SBF program on pinocchio 0.11.2 and pinocchio-system, the shared
  `ballista-common` crate.
- TypeScript SDK (Zod, vitest).
- Mollusk 0.14 integration suite, the proptest generator, Certora specs.
- The last task: the LiteSVM protocol suite in `tests/protocols` on `claude/protocol-tests`.

---

## Numbers

| What | Value |
| --- | --- |
| Opcodes | 75 `OPEN_REGISTRY`, 76 `READ_REGISTRY`, 77 `WRITE_REGISTRY` |
| Runtime errors | 6025 `InvalidRegistryEntry`, 6026 `RegistryReentry`; first unused becomes 6027 |
| Verifier error | 6132 `InvalidRegistry(pc)`; first unused becomes 6133 |
| Unknown-opcode samples | the sweeps' `OP_BYTES_LEN + 1` (75) becomes `OP_WRITE_REGISTRY + 1` (78); `0xfe` stays |
| Entry header | 72 bytes: `BREG` (0..4), version 1 (4), registry index (5), zero (6..8), template address (8..40), key (40..72) |
| Limits | at most 8 opens per template, registry index below 8, 1 to 512 field bytes per registry |
| Entry size | 72 + the registry's field size, at most 584 bytes; rent for 88 bytes is 1,503,360 lamports |

## Encoding

Every account reference sits in an operand field, as for every other opcode; numbers and the
System program account sit in the immediate. Spare operands must be `NO_INDEX`, spare immediate
bytes zero and `flags` zero: a stored template is never verified again, so a field accepted with
any value now could never take a meaning later.

| Opcode | `dst` | `a` | `b` | `c` | Immediate, little-endian bytes |
| --- | --- | --- | --- | --- | --- |
| 75 `OPEN_REGISTRY` | `NO_INDEX` | entry account | key register, or `NO_INDEX` for the zero key | payer account | 0: registry index · 1–2: field size (u16) · 3: System program account · 4–7: zero |
| 76 `READ_REGISTRY` | destination | entry account | `NO_INDEX` | `NO_INDEX` | 0–1: field offset past the header (u16) · 2: a read opcode, the width selector · 3–7: zero |
| 77 `WRITE_REGISTRY` | `NO_INDEX` | value register | entry account | `NO_INDEX` | as `READ_REGISTRY` |

`common/src/template/wire.rs` gains `RegistryOpen { index, size, system_program }` and
`RegistryField { offset, selector }` with `encode` and `decode`; `decode` returns `None` when a
spare byte is set. The verifier, the executor, the builder and the TypeScript compiler all use
this one layout.

## Decisions this plan makes

1. **A slot is its entry account's fixed index. (user decision)** The spec's "slot" never has to
   exist at run time: `READ_REGISTRY` and `WRITE_REGISTRY` name the entry account itself, and the
   verifier requires an `OPEN_REGISTRY` of that account at a lower pc. The spec's rules keep their
   meaning: "each slot opened once" is each entry account opened once; "at most 8 slots" is at
   most 8 opens. The immediate carries no separate slot byte, which would only repeat `a`.
   Measured reason, on `65c9b45` with every fixed case and all 32 cookbook examples:
   - A slot table in `Scratch` (`open: u8` plus `[MaybeUninit<&AccountView>; 8]`) costs every
     run 2 compute units (CU) on top of the template address below; the array alone costs 0,
     the mask byte 2, from register allocation in `run`, although it merged two stores into one.
   - The same table in `Machine` costs a 30-pass count loop 426 CU and the math case 102.
   - A lazily allocated table behind one pointer costs 3 to 4 CU a run.
   - Naming the account costs nothing: `READ_REGISTRY` and `WRITE_REGISTRY` resolve it as every
     account opcode does.
2. **The template address reaches the run for free.** `processor::run` takes the instruction's
   whole account list, template first, instead of the runtime accounts and the template address
   as a sixth argument that SBF passes on the stack. `Machine` gains `template:
   Option<&'data AccountView>`: `Some` in a run, `None` from `execute_instruction`, which the
   unit tests and Certora call. Measured: 1 CU less on the 9 fixed cases without a CPI, no change
   on any case or example with one. Passing the address to `Scratch` or `Machine` the old way
   cost 2 CU a run.
3. **An open entry is marked in its own borrow flag, and that is the reentry check. (user
   decision)** Opening an entry leaves its data marked exclusively borrowed for the rest of the
   run (`try_borrow_mut`, then `forget`). `bounded_invoke` already calls `check_borrow_mut` on
   every account a CPI passes writable, so a CPI that passes an open entry writable fails before
   it is made; `invoke_cpi` maps that failure to `RegistryReentry` in its cold error path.
   - It keeps the spec's invariant, no lost updates: the runtime refuses indirect reentrancy
     (A → B → A), so a nested Ballista run can only be a direct CPI from the template, and it can
     write an entry only if that CPI passes the entry writable.
   - It is narrower than the spec's wording, "a CPI to Ballista fails while an entry is open": a
     CPI to Ballista that does not pass an open entry writable still runs, and a CPI to any other
     program that passes one writable fails.
   - Measured: 0 CU on every case and example. The literal check (`open != 0 && program ==
     Ballista`) before `bounded_invoke` cost 3 CU per CPI: `run, payroll 30 rows` +89, the
     cookbook +653 across all 32 examples. Placed in the uncached program-resolution branch it
     cost 14 per CPI, `run, payroll 30 rows` +101.
   - `READ_REGISTRY` and `WRITE_REGISTRY` also check the flag before touching the entry: only an
     open sets it, so it is a free guard that the account was opened in this run.
4. **An existing entry is not re-derived.** Only Ballista can write a Ballista-owned account's
   data, and the only code that writes a `BREG` header writes it at the address derived from that
   header's template, index and key. So the header check (owner, size, magic, version, index,
   template, key) proves the address, and the canonical-bump search (about 300 CU per bump tried)
   runs only when an entry is created.
5. **Creation lives in one function, `registry::create_entry`, the only place a run signs.**
   - With no lamports at the address: `CreateAccount::with_minimum_balance` from the payer,
     signed with the entry's seeds `["registry", template, [index], key, [bump]]`.
   - With lamports already there: a `Transfer` of the rent still missing, signed by the payer
     alone (the transaction carries that signature; the entry does not sign a transfer into
     itself), then `Allocate` and `Assign`, each signed with the entry's seeds. This is
     `create_template_account` in `lib.rs`, which already handles a pre-funded template address.
   - Then the header is written. The template's own CPIs go through `bounded_invoke`, which passes
     no signer seeds, so a template never gets a signature.
6. **CPI budget.** An open counts 3 toward `MAX_EXPANDED_CPIS` in the verifier and in the
   TypeScript compiler's `worstCaseCpis`, since the pre-funded path makes three calls. The run's
   `expanded` counter and `executed` bits, which the run event reports, keep counting `INVOKE`
   instructions only.
7. **Verifier errors.** Every registry rule fails with `InvalidRegistry(pc)`, as the spec says
   ("unless noted"), including the key register (unset, out of range or not a `pubkey`) and a
   written value whose type does not match its width. `READ_REGISTRY`'s destination keeps the
   usual `InvalidRegister(dst)`.
8. **Widths.** `READ_REGISTRY` takes any of the nine read opcodes. `WRITE_REGISTRY` takes only
   the five whose width holds every value of their type: `bool`, `u64`, `i64`, `u128`, `pubkey`.
   A `u8` selector would need a range check at run time; the spec's "its type must match the
   width" rules it out. TypeScript registry fields take the same five types.
9. **The spec's error table says a read-only entry fails with `InvalidRegistryEntry`.** The open
   checks `is_writable()` and does fail that way, but in a whole run the account-validation pass
   gets there first: the verifier requires the entry's constraint to be writable, so a read-only
   entry fails with `AccountConstraintFailed` (6020, the account's index as context) before the
   first instruction. The executor's unit test covers the open's own check; the Mollusk test
   asserts 6020. The docs should say so. **(user decision: accept, or drop the sentence from the spec)**
10. **TypeScript.**
    - `registries: { name: { field: type, ... } }` in `defineTemplate`. The registry index is
      the declaration order; field offsets pack in declaration order with no padding.
    - `account.registry(registry, { key?, payer })` is an account constraint, `{ writable: true,
      registry: {...} }`. With no `key` the entry is the template-wide zero key.
    - `expression.registry(account, field)` and `step.setRegistry(account, field, value)` name
      the **registry account**, the `accounts` entry, not the registry: a template can open two
      entries of one registry (an allowlist keyed by the caller and by an input), and the account
      says which. In the spec's example both are called `limits`.
    - The compiler emits every open after the hoisted inputs and constants and before the first
      step, in account declaration order, so each key expression (an input, an account key, a
      constant) is ready. Opens therefore precede every `setReturnData`.
    - It refuses a template with registry accounts and no fixed account pinned to the System
      program, a payer that is not a declared signer and writable, a registry account passed
      writable to an `invoke` (that run could only fail with `RegistryReentry`), and an unknown
      registry, account or field.
    - The spec's example uses two sugars the SDK lacks, `account.systemProgram()` and
      `expression.accountKey(name)`. Both are added, so the example compiles as written.
11. **`rateLimit`'s arithmetic.** `now = max(clock, lastSpend)` in `i64`: `now` never reads
    earlier than `lastSpend`, so `now − lastSpend` is never negative without a separate clamp, and
    the `lastSpend` written back, being `now`, never moves back either — so a clock that steps
    back and later recovers never refills the same seconds twice. Then `refill = u128(now −
    lastSpend) × u128(refillPerSecond)`, below `2^127` since both factors stay under `2^63` and
    `2^64`; `total = u128(spent) − min(u128(spent), refill) + u128(amount)`; require `total <=
    u128(cap)` as `withinRateLimit`; write `spent = u64(total)` and `lastSpend = now`. A fresh
    entry's `lastSpend` of 0 makes `now` the clock and the elapsed time the whole Unix time, a full
    refill.
12. **Certora covers the opcodes vacuously, as it does introspection.** Neither spec program
    declares an account pinned to the System program, so the verifier rejects every open there,
    and a read or write at pc 0 has no open before it. The two runtime errors join
    `value_dependent`.
13. **The generator's registry** is always the last three fixed accounts (System program, payer,
    entry), with a fixed five-field layout, so its harness can supply them.

## Measured on the prototype

A throwaway prototype of decisions 1 to 5 on `65c9b45`, under Mollusk, with 3 fixed accounts
and an 88-byte entry:

| Run | CU |
| --- | --- |
| Open an existing entry, read two fields, read the clock, add, write two fields (Task 11's case, with this plan's code) | 1,632 |
| The same run creating the entry (no lamports at the address) | about +2,070 |
| The same run creating a pre-funded entry (transfer, allocate, assign) | about +5,310 |
| One `WRITE_REGISTRY` / one `READ_REGISTRY` / `Clock::get` | about 70 / 15 / 140 |

This plan's Tasks 1 to 4 and Task 11's case were applied to a copy of `65c9b45` and measured:
the 9 fixed cases without a CPI came out 1 CU cheaper, every case and example with a CPI and all
32 cookbook examples unchanged, `create template, payroll 30 rows` 4,483 → 4,485, and the new case
1,632. Their host and Mollusk tests passed as written. Task 11 measures again on the real branch.

## Base and order

- **Branch.** `claude/runtime-registry`, cut from `claude/runtime-extensions` at `65c9b45`
  (phases 1 to 4 merged: math, loops, output, introspection).
- **Paths** are relative to the worktree root. `execute.rs` and `verify.rs` are named by
  function, not line.
- **Task 13** runs on `claude/protocol-tests` after this branch is merged into it. Everything else
  runs here.

## Rules for every task

- Keep the surrounding style: doc comments that explain why; `#[inline(never)]` for heavy
  helpers reached from the dispatch loop, taking at most four words; `RunResult` and
  `BallistaError` for failures.
- The dispatch loop's outer match (`step`) gains no arms, guards or `if`s. New opcodes reach
  `extended_instruction` through its fallback and get an inner arm there, before `_ =>
  write_output(...)`.
- The run path trusts finalized templates: every structural rule is the verifier's.
- **Never use `git stash`.** To set work aside, commit it.
- **Do not edit `package.json`, `pnpm-lock.yaml` or anything under `docs/` except
  `docs/superpowers/`.**
- **Compute-unit ceilings (`fixtures/cu-ceilings.json`, `fixtures/example-ceilings.json`)
  change only by hand, to exact measured values,** with a before/after table in the commit
  message and a `benches/CHANGELOG.md` entry. Only Task 11 touches them.
- End every commit message with:

```text
Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
```

## Docs (out of scope; the docs session updates them)

- `docs/guide/trust-model.md`: Ballista "keeps no state between runs" and "never signs". It now
  keeps state in registry entries only, and signs only to create an entry.
- `docs/reference/language.md`: the three opcodes, the `registries` schema, `account.registry`,
  `expression.registry`, `step.setRegistry`, `rateLimit`.
- `docs/reference/limits.md`: 8 opens, 8 registries, 512 field bytes, 3 CPIs per open.
- `docs/guide/errors-and-events.md`: 6025, 6026, 6132; decision 9's read-only entry.
- Also say "never signs" or "no state": `docs/guide/why-ballista.md`,
  `docs/guide/accounts-and-cpis.md`, `docs/guide/pda-assertions.md`. The opcode table is in
  `docs/reference/wire-format.md`; the SDKs in `docs/reference/typescript.md` and
  `docs/reference/rust.md`.

---

## File map

| File | Change |
| --- | --- |
| `common/src/template/wire.rs` | Opcodes 75–77; `REGISTRY_*` constants, `SYSTEM_PROGRAM_ADDRESS`; `RegistryOpen`, `RegistryField`; `InvalidRegistry`; error name tables |
| `common/src/template/verify.rs` | `verify_open_registry`, `registry_field`; three arms; tests; fixture list |
| `common/src/template/builder.rs` | `open_registry`, `read_registry`, `write_registry` |
| `common/src/template/generate.rs` | A registry in generated programs; `GeneratedRegistry` |
| `programs/ballista/src/error.rs` | `InvalidRegistryEntry`, `RegistryReentry` |
| `programs/ballista/src/lib.rs` | `run_template` passes the whole account list |
| `programs/ballista/src/utils/pda.rs` | `get_registry_address` |
| `programs/ballista/src/processor/registry.rs` | **New.** Header, open, create, field write; host tests |
| `programs/ballista/src/processor/mod.rs` | Declare `registry` (public under `spec-api`) |
| `programs/ballista/src/processor/execute.rs` | `run` signature; `Machine.template`; the router arm; `registry_instruction`; `reentry_or`; tests |
| `clients/rust/src/lib.rs` | Decode tests |
| `clients/js/src/errors.ts`, `errors.test.ts` | Names; decode tests |
| `clients/js/src/schema.ts` | `registries`; `account.registry`, `account.systemProgram`; `expression.registry`, `expression.accountKey`; `step.setRegistry` |
| `clients/js/src/compiler.ts` | Opcode table; opens; field lowering; rules; `worstCaseCpis` |
| `clients/js/src/helpers.ts` | `rateLimit` |
| `clients/js/src/opcodes.test.ts`, `compiler.test.ts` | Parity entries; schema, compiler and `rateLimit` tests (the SDK keeps helper tests in `compiler.test.ts`) |
| `clients/js/src/fixtures.test.ts` | `rate-limited-transfer` fixture |
| `tests/ballista/src/lib.rs` | Mollusk executor tests; the end-to-end test; the generator harness |
| `tests/ballista/src/cases.rs` | `run, registry open and update` |
| `tests/ballista/src/pda_equivalence.rs` | `get_registry_address` against `Pubkey::find_program_address` |
| `certora/ballista-specs/src/rules/errors.rs`, `typing.rs` | Error rules; `value_dependent` |
| `fixtures/*` | Regenerated names, `rate-limited-transfer.hex`, manifest; ceilings by hand in Task 11 |
| `benches/CHANGELOG.md` | Task 11's entry |

---

### Task 0: Worktree, baseline, spec and plan

- [ ] **Step 1: Create the worktree.** Use superpowers:using-git-worktrees. Create branch
  `claude/runtime-registry` from the tip of `claude/runtime-extensions`, at
  `.claude/worktrees/runtime-registry`.

```bash
git worktree add -b claude/runtime-registry .claude/worktrees/runtime-registry claude/runtime-extensions
cd .claude/worktrees/runtime-registry
git log --oneline -1
grep -n "pub const OP_BYTES_LEN\|pub const OP_OPEN_REGISTRY" common/src/template/wire.rs
grep -n "InvalidIntrospection\|WritableAccountBytesRead" fixtures/*-error-names.txt
```

Expected: the tip is `65c9b45` or a descendant; `OP_BYTES_LEN` is 74 and there is no
`OP_OPEN_REGISTRY`; the two names are the last lines of their files. If `OP_OPEN_REGISTRY`
exists, stop: someone started this phase.

- [ ] **Step 2: Install and build.**

```bash
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
cargo test --manifest-path certora/Cargo.toml -p ballista-specs --features rt 2>&1 | tail -3
cargo clippy -p ballista -p ballista-common --all-targets 2>&1 | grep -c -E "^(warning|error)"
```

Expected: every command exits 0. Record the clippy count; Task 12 compares against it. If
anything fails on the untouched base, stop and report it rather than fixing it silently.

- [ ] **Step 4: Record the compute-unit baseline.** Task 11 compares against these exact numbers.
  The ceiling tests pass with the committed ceilings, and on `65c9b45` every case and example
  measures exactly its ceiling, so the committed files are the baseline:

```bash
mkdir -p target/registry-cu
cp fixtures/cu-ceilings.json target/registry-cu/cu-before.json
cp fixtures/example-ceilings.json target/registry-cu/examples-before.json
```

- [ ] **Step 5: Commit the spec and this plan.**

```bash
mkdir -p docs/superpowers/specs docs/superpowers/plans
cp /private/tmp/claude-501/-Users-jacob-Documents-ballista/06470e69-8486-4adc-b478-7e3c20bf99aa/scratchpad/2026-09-27-template-registry-design.md docs/superpowers/specs/
cp /private/tmp/claude-501/-Users-jacob-Documents-ballista/06470e69-8486-4adc-b478-7e3c20bf99aa/scratchpad/plans/2026-09-27-runtime-registry.md docs/superpowers/plans/
git add docs/superpowers/specs/2026-09-27-template-registry-design.md docs/superpowers/plans/2026-09-27-runtime-registry.md
git commit -m "$(cat <<'MSG'
Specify the template registry and plan runtime phase 5

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 1: Numbers, layouts and error kinds

**Files:**
- `common/src/template/wire.rs`: opcodes, constants, `RegistryOpen`, `RegistryField`,
  `TemplateError::InvalidRegistry`, both name tables, the error-code test.
- `common/src/template/verify.rs`: the unknown-opcode sweep in
  `every_opcode_rejects_uninitialized_or_mistyped_operands`.
- `programs/ballista/src/error.rs`; `programs/ballista/src/processor/execute.rs`: the sweep in
  `opcodes_the_executor_does_not_run_fail_before_reading_operands`.
- `clients/js/src/errors.ts`, `clients/js/src/errors.test.ts`, `clients/rust/src/lib.rs`.
- `clients/js/src/compiler.ts` (the `opcode` table) and `clients/js/src/opcodes.test.ts`: the
  parity test reads every `OP_*` in `wire.rs`, so the TypeScript names land with the numbers.
- `certora/ballista-specs/src/rules/errors.rs`; `fixtures/*-error-names.txt` (regenerated).

- [ ] **Step 1: Write the failing tests.**
  - `wire.rs`, in `error_codes_are_unique_and_round_trip_context`: add
    `TemplateError::InvalidRegistry(15),` after `TemplateError::InvalidIntrospection(15),`;
    change `VERIFIER_ERROR_BASE + 31` to `+ 32` in the `codes.last()` assertion, and
    `decode_ballista_error(VERIFIER_ERROR_BASE + 32)` to `+ 33`.
  - `wire.rs`, a new test in the same `mod tests`:

```rust
    #[test]
    fn registry_immediates_round_trip_and_refuse_spare_bytes() {
        let open = RegistryOpen { index: 7, size: 512, system_program: 3 };
        assert_eq!(open.encode(), 0x0302_0007);
        assert_eq!(RegistryOpen::decode(open.encode()), Some(open));
        assert_eq!(RegistryOpen::decode(open.encode() | 1 << 32), None);
        assert_eq!(RegistryOpen::decode(open.encode() | 1 << 63), None);

        let field = RegistryField { offset: 0x01ff, selector: OP_READ_U128 };
        assert_eq!(field.encode(), 0x0f_01ff);
        assert_eq!(RegistryField::decode(field.encode()), Some(field));
        assert_eq!(RegistryField::decode(field.encode() | 1 << 24), None);

        assert_eq!(REGISTRY_ENTRY_HEADER_LEN, 4 + 1 + 1 + 2 + 32 + 32);
        assert_eq!(REGISTRY_ENTRY_HEADER_LEN + MAX_REGISTRY_SIZE, 584);
    }
```

  - `error.rs`, in the test's `variants` list: add `BallistaError::InvalidRegistryEntry,` and
    `BallistaError::RegistryReentry,` after `BallistaError::WritableAccountBytesRead,`.
  - `clients/rust/src/lib.rs`, in `error_codes_decode_with_context`: replace
    `assert!(decode_ballista_error(6025).is_none());` and
    `assert!(decode_ballista_error(6132).is_none());` with:

```rust
        assert_eq!(
            decode_ballista_error((3 << 16) | 6025).unwrap().name,
            "InvalidRegistryEntry"
        );
        assert_eq!(decode_ballista_error(6026).unwrap().name, "RegistryReentry");
        assert_eq!(decode_ballista_error(6132).unwrap().name, "InvalidRegistry");
        assert!(decode_ballista_error(6027).is_none());
        assert!(decode_ballista_error(6133).is_none());
```

  - `errors.test.ts`: replace `expect(decodeBallistaError(6025)).toBeUndefined();` with

```ts
    expect(decodeBallistaError((3 << 16) | 6025)).toMatchObject({ name: 'InvalidRegistryEntry', context: 3, source: 'runtime' });
    expect(decodeBallistaError(6026)).toMatchObject({ name: 'RegistryReentry', source: 'runtime' });
    expect(decodeBallistaError(6132)).toMatchObject({ name: 'InvalidRegistry', source: 'verifier' });
    expect(decodeBallistaError(6027)).toBeUndefined();
```

    and `expect(decodeBallistaError(6132)).toBeUndefined();` with
    `expect(decodeBallistaError(6133)).toBeUndefined();`.
  - The two unknown-opcode sweeps: in `verify.rs`, change the case
    `(OP_BYTES_LEN + 1, Some(VALUE_U64), None, Err(TemplateError::InvalidInstruction(1))),` to
    `(OP_WRITE_REGISTRY + 1, Some(VALUE_U64), None, Err(TemplateError::InvalidInstruction(1))),`.
    In `execute.rs`, change `OP_BYTES_LEN + 1` in the opcode list to `OP_WRITE_REGISTRY + 1`, and
    the comment above it to: `// 39 is unassigned, and so is every number after
    `OP_WRITE_REGISTRY`, 77, the last opcode the registry takes.` Keep `0xfe`: it is still
    unassigned.

- [ ] **Step 2: Run them and confirm they fail.**

Run: `cargo test -p ballista-common --lib wire && cargo test -p ballista --lib error`
Expected: compile errors: `TemplateError::InvalidRegistry`, `RegistryOpen`, `RegistryField`,
`REGISTRY_ENTRY_HEADER_LEN`, `MAX_REGISTRY_SIZE`, `OP_WRITE_REGISTRY` and the two
`BallistaError` variants are not defined.

- [ ] **Step 3: `wire.rs`: opcodes and constants.** After `pub const OP_BYTES_LEN: u8 = 74;` add:

```rust
/// Checks the registry entry in fixed account `a`, or creates it, and keeps it open for the rest
/// of the run. `b` is the `pubkey` register holding its key, or [`NO_INDEX`] for the zero key; `c`
/// is the account that pays for an entry this creates. The immediate is a [`RegistryOpen`].
/// Writes no register. Once per entry account, at the root, never after `SET_RETURN_DATA`.
pub const OP_OPEN_REGISTRY: u8 = 75;
/// Reads a field of the entry open in account `a` into `dst`, typed as the read opcode the
/// immediate's [`RegistryField`] names.
pub const OP_READ_REGISTRY: u8 = 76;
/// Writes register `a` into a field of the entry open in account `b`. The immediate is a
/// [`RegistryField`]; the value has the type of its read opcode, one of the five whose width holds
/// every value of that type.
pub const OP_WRITE_REGISTRY: u8 = 77;

/// The first seed of a registry entry's address: `["registry", template, [index], key]`.
pub const REGISTRY_SEED: &[u8] = b"registry";
/// The first four bytes of every registry entry.
pub const REGISTRY_ENTRY_MAGIC: [u8; 4] = *b"BREG";
pub const REGISTRY_ENTRY_VERSION: u8 = 1;
/// Magic, version, registry index, two zero bytes, the template's address and the key. The
/// registry's fields follow.
pub const REGISTRY_ENTRY_HEADER_LEN: usize = 72;
/// Registries a template can declare: an entry's registry index is below this.
pub const MAX_REGISTRIES: usize = 8;
/// Entries one template can open.
pub const MAX_REGISTRY_OPENS: usize = 8;
/// The most field bytes one registry holds, after the header.
pub const MAX_REGISTRY_SIZE: usize = 512;
/// The CPIs an open makes at most, creating a pre-funded entry: a transfer, an allocate and an
/// assign. The verifier counts each open as this many toward [`MAX_EXPANDED_CPIS`].
pub const REGISTRY_OPEN_CPIS: usize = 3;
/// The System program's address. Not `SYSTEM_PROGRAM_ID`: the Rust SDK exports a `Pubkey` of
/// that name, and files that glob-import this module beside it would see two.
pub const SYSTEM_PROGRAM_ADDRESS: [u8; 32] = [0; 32];

/// What `OP_OPEN_REGISTRY`'s immediate packs: the registry index in byte 0, the registry's field
/// size in bytes 1 and 2, and in byte 3 the fixed account pinned to the System program, which a
/// creation calls. The other bytes are zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistryOpen {
    pub index: u8,
    pub size: u16,
    pub system_program: u8,
}

impl RegistryOpen {
    pub const fn encode(self) -> u64 {
        self.index as u64 | (self.size as u64) << 8 | (self.system_program as u64) << 24
    }

    /// `None` when any byte past the three fields is set.
    pub const fn decode(immediate: u64) -> Option<Self> {
        if immediate >> 32 != 0 {
            return None;
        }
        Some(Self {
            index: immediate as u8,
            size: (immediate >> 8) as u16,
            system_program: (immediate >> 24) as u8,
        })
    }
}

/// What `OP_READ_REGISTRY`'s and `OP_WRITE_REGISTRY`'s immediates pack: the field's offset past
/// the entry header in bytes 0 and 1, and in byte 2 the `OP_READ_*` opcode whose width and type
/// the field has. The other bytes are zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistryField {
    pub offset: u16,
    pub selector: u8,
}

impl RegistryField {
    pub const fn encode(self) -> u64 {
        self.offset as u64 | (self.selector as u64) << 16
    }

    /// `None` when any byte past the two fields is set.
    pub const fn decode(immediate: u64) -> Option<Self> {
        if immediate >> 24 != 0 {
            return None;
        }
        Some(Self {
            offset: immediate as u16,
            selector: (immediate >> 16) as u8,
        })
    }
}
```

- [ ] **Step 4: `wire.rs`: the error kind and names.**
  - Change `RUNTIME_ERROR_NAMES: [&str; 25]` to `[&str; 27]` and add `"InvalidRegistryEntry",`
    and `"RegistryReentry",` after `"WritableAccountBytesRead",`.
  - After the `InvalidIntrospection(usize),` variant add:

```rust
    /// A registry opcode breaks a registry rule: an open outside the root, repeated, past the
    /// eighth, after `SET_RETURN_DATA`, with a bad index, size, key or account; or a field read or
    /// write with no open of its account before it, outside the registry, or of the wrong type.
    InvalidRegistry(usize),
```

  - In `TemplateError::code`, after the `InvalidIntrospection` arm add
    `TemplateError::InvalidRegistry(index) => (32, clamp(index)),`.
  - Change `VERIFIER_ERROR_NAMES: [&str; 32]` to `[&str; 33]` and add `"InvalidRegistry",` after
    `"InvalidIntrospection",`.
  - `Display` prints the `Debug` form, so it needs nothing.

- [ ] **Step 5: `error.rs`.** After the `WritableAccountBytesRead,` variant add:

```rust
    /// An open found an account that is not the entry the template named: the wrong owner, size or
    /// header, not writable, or, when it creates the entry, not the derived address. The context
    /// is the program counter.
    #[error("invalid registry entry")]
    InvalidRegistryEntry,
    /// A CPI passed an entry this run has open as writable. Only a nested Ballista run could use
    /// that, to write the entry between this run's read and its write. The context is the program
    /// counter.
    #[error("a CPI passed an open registry entry writable")]
    RegistryReentry,
```

- [ ] **Step 6: `errors.ts`.** Add `'InvalidRegistryEntry',` and `'RegistryReentry',` after
  `'WritableAccountBytesRead',`, and `'InvalidRegistry',` after `'InvalidIntrospection',`.
  `explainRunError` needs nothing: all three carry a program counter, which its default branch
  maps to the step.

- [ ] **Step 6b: The TypeScript opcode table.** In `compiler.ts`, after `bytesLength: 74,` add
  `openRegistry: 75,`, `readRegistry: 76,` and `writeRegistry: 77,`. In `opcodes.test.ts`, after
  `bytesLength: 'OP_BYTES_LEN',` add `openRegistry: 'OP_OPEN_REGISTRY',`,
  `readRegistry: 'OP_READ_REGISTRY',` and `writeRegistry: 'OP_WRITE_REGISTRY',`.

- [ ] **Step 7: Certora `errors.rs`.**
  - In `rule_runtime_error_codes_carry_context_and_stay_in_range`, add
    `BallistaError::InvalidRegistryEntry,` and `BallistaError::RegistryReentry,` to the end of
    the `pick!` list.
  - In `rule_verifier_error_codes_are_distinct_and_in_range`, replace
    `_ => TemplateError::InvalidIntrospection(nondet()),` with:

```rust
        31 => TemplateError::InvalidIntrospection(nondet()),
        _ => TemplateError::InvalidRegistry(nondet()),
```

- [ ] **Step 8: Regenerate the name fixtures and run the tests.**

```bash
pnpm fixtures
cargo test -p ballista-common --lib
cargo test -p ballista --lib
cargo test -p ballista-sdk error_codes_decode_with_context
pnpm --dir clients/js exec vitest run src/errors.test.ts src/fixtures.test.ts src/opcodes.test.ts
cargo check -p ballista-specs --features rt --manifest-path certora/Cargo.toml
```

Expected: PASS. `git diff fixtures` shows two names appended to `runtime-error-names.txt` and one
to `verifier-error-names.txt`, and nothing else.

- [ ] **Step 9: Commit.**

```bash
git add common/src/template/wire.rs common/src/template/verify.rs programs/ballista/src/error.rs \
  programs/ballista/src/processor/execute.rs clients/js/src/errors.ts clients/js/src/errors.test.ts \
  clients/js/src/compiler.ts clients/js/src/opcodes.test.ts clients/rust/src/lib.rs \
  certora/ballista-specs/src/rules/errors.rs fixtures
git commit -m "$(cat <<'MSG'
Number the registry opcodes, lay out their immediates and name their errors

Opcodes 75 to 77; InvalidRegistryEntry (6025), RegistryReentry (6026) and InvalidRegistry (6132).
The unknown-opcode sweeps sample 78, now the first unassigned opcode.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 2: The builder and the verifier

**Files:**
- `common/src/template/builder.rs`: `open_registry`, `read_registry`, `write_registry`.
- `common/src/template/verify.rs`: three arms in `verify_instruction`, `verify_open_registry`,
  `registry_field`, and a test.

- [ ] **Step 1: The builder methods.** After `bytes_len` in `builder.rs` add:

```rust
    /// Opens registry `index`'s entry in account `entry` for the rest of the run: the run checks
    /// the entry, or creates it with `payer`'s lamports through `system_program`. `key` is a
    /// `pubkey` register, or `None` for the zero key; `size` is the registry's field bytes.
    /// Returns the instruction's index.
    pub fn open_registry(
        &mut self,
        entry: u8,
        key: Option<u8>,
        payer: u8,
        index: u8,
        size: u16,
        system_program: u8,
    ) -> usize {
        let open = RegistryOpen { index, size, system_program };
        let key = key.unwrap_or(NO_INDEX);
        self.emit(record(OP_OPEN_REGISTRY, NO_INDEX, entry, key, payer, 0, open.encode()))
    }

    /// The field at `offset` past the header of the entry open in `entry`, with the width and type
    /// of `read_opcode`, one of the `OP_READ_*` opcodes.
    pub fn read_registry(&mut self, entry: u8, offset: u16, read_opcode: u8) -> u8 {
        let field = RegistryField { offset, selector: read_opcode };
        self.op(OP_READ_REGISTRY, entry, NO_INDEX, NO_INDEX, field.encode())
    }

    /// Writes `value` into the field at `offset` of the entry open in `entry`, with the width of
    /// `read_opcode`. Returns the instruction's index.
    pub fn write_registry(&mut self, entry: u8, offset: u16, read_opcode: u8, value: u8) -> usize {
        let field = RegistryField { offset, selector: read_opcode };
        self.emit(record(OP_WRITE_REGISTRY, NO_INDEX, value, entry, NO_INDEX, 0, field.encode()))
    }
```

- [ ] **Step 2: Write the failing verifier test.** Add to `verify.rs`'s `mod tests`:

```rust
    /// System program, entry and payer, then an open of registry 2 (16 bytes) keyed by the
    /// payer's address. Returns the builder, the three accounts and the open's pc.
    fn registry_program() -> (ProgramBuilder, u8, u8, u8, usize) {
        let mut builder = ProgramBuilder::new();
        let system = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
        let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let key = builder.account_key(payer);
        let pc = builder.open_registry(entry, Some(key), payer, 2, 16, system);
        (builder, system, entry, payer, pc)
    }

    fn next_pc(builder: &mut ProgramBuilder) -> usize {
        builder.instructions_mut().len()
    }

    #[test]
    fn registry_rules() {
        let invalid = TemplateError::InvalidRegistry;

        // A valid open, a read of each width and a write of each writable width; an open counts
        // three CPIs.
        let (mut builder, _, entry, _, _) = registry_program();
        let spent = builder.read_registry(entry, 0, OP_READ_U64);
        builder.read_registry(entry, 15, OP_READ_U8);
        builder.read_registry(entry, 12, OP_READ_I32);
        builder.write_registry(entry, 8, OP_READ_U64, spent);
        let flag = builder.const_bool(true);
        builder.write_registry(entry, 15, OP_READ_BOOL, flag);
        let stats = verify_builder(&builder).unwrap();
        assert_eq!(stats.max_expanded_cpis, 3);

        // A pubkey field takes 32 bytes: not in a 16-byte registry, and fine in a 32-byte one.
        let (mut shorter, _, entry, _, _) = registry_program();
        let pc = next_pc(&mut shorter);
        shorter.read_registry(entry, 0, OP_READ_PUBKEY);
        assert_eq!(verify_builder(&shorter).map(|_| ()), Err(invalid(pc)));
        let (mut wider, _, entry, _, open) = registry_program();
        wider.instructions_mut()[open].immediate_le =
            RegistryOpen { index: 2, size: 32, system_program: 0 }.encode().to_le_bytes();
        wider.read_registry(entry, 0, OP_READ_PUBKEY);
        assert!(verify_builder(&wider).is_ok());

        // No key: the zero key.
        let mut builder = ProgramBuilder::new();
        let system = builder.account(0, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
        let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        builder.open_registry(entry, None, payer, 0, 1, system);
        assert!(verify_builder(&builder).is_ok());

        // Each broken open, one at a time. `(entry flags, entry pinned, payer flags, system pinned
        // to, key type, index, size)`.
        let base = (ACCOUNT_WRITABLE, false, ACCOUNT_SIGNER | ACCOUNT_WRITABLE, SYSTEM_PROGRAM_ADDRESS, Some(VALUE_PUBKEY), 0u8, 16u16);
        let cases = [
            ("entry read-only", (0, false, base.2, base.3, base.4, 0, 16)),
            ("entry signer and writable is fine", (ACCOUNT_SIGNER | ACCOUNT_WRITABLE, false, base.2, base.3, base.4, 0, 16)),
            ("entry pinned", (ACCOUNT_WRITABLE, true, base.2, base.3, base.4, 0, 16)),
            ("payer not a signer", (base.0, false, ACCOUNT_WRITABLE, base.3, base.4, 0, 16)),
            ("payer read-only", (base.0, false, ACCOUNT_SIGNER, base.3, base.4, 0, 16)),
            ("system program elsewhere", (base.0, false, base.2, [1; 32], base.4, 0, 16)),
            ("key a u64", (base.0, false, base.2, base.3, Some(VALUE_U64), 0, 16)),
            ("key unset", (base.0, false, base.2, base.3, None, 0, 16)),
            ("index 8", (base.0, false, base.2, base.3, base.4, 8, 16)),
            ("size 0", (base.0, false, base.2, base.3, base.4, 0, 0)),
            ("size 513", (base.0, false, base.2, base.3, base.4, 0, 513)),
            ("size 512 is fine", (base.0, false, base.2, base.3, base.4, 0, 512)),
        ];
        for (name, (entry_flags, pinned, payer_flags, system_address, key_type, index, size)) in cases {
            let mut builder = ProgramBuilder::new();
            let system = builder.account(0, Some(system_address), None, 0);
            let entry = builder.account(entry_flags, pinned.then_some([5; 32]), None, 0);
            let payer = builder.account(payer_flags, None, None, 0);
            let key = typed_register(&mut builder, key_type);
            let pc = builder.open_registry(entry, Some(key), payer, index, size, system);
            let expected = if name.ends_with("is fine") { Ok(()) } else { Err(invalid(pc)) };
            assert_eq!(verify_builder(&builder).map(|_| ()), expected, "{name}");
        }

        // The accounts must be fixed ones: a row account, or one past the declared accounts.
        for (entry, payer, system) in [(ITERATION_ACCOUNT_BIT, 2, 0), (1, 9, 0), (1, 2, ITERATION_ACCOUNT_BIT)] {
            let mut builder = ProgramBuilder::new();
            builder.account(0, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
            builder.account(ACCOUNT_WRITABLE, None, None, 0);
            builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
            let pc = builder.open_registry(entry, None, payer, 0, 8, system);
            assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)), "{entry} {payer} {system}");
        }

        // A destination, or a spare immediate byte.
        let (mut builder, _, _, _, pc) = registry_program();
        builder.instructions_mut()[pc].dst = 0;
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)));
        let (mut builder, _, _, _, pc) = registry_program();
        builder.instructions_mut()[pc].immediate_le[4] = 1;
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)));

        // Only at the root.
        let mut builder = ProgramBuilder::new();
        let system = builder.account(0, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
        let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let count = builder.const_u64(1);
        let mut pc = 0;
        builder.repeat(count, 1, 0, |body| pc = body.open_registry(entry, None, payer, 0, 8, system));
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)));

        // Each entry account once; one size per registry index; two entries of one registry.
        let (mut builder, system, entry, payer, _) = registry_program();
        let pc = builder.open_registry(entry, None, payer, 3, 16, system);
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)), "entry opened twice");
        let (mut builder, system, _, payer, _) = registry_program();
        let other = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let pc = builder.open_registry(other, None, payer, 2, 24, system);
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)), "registry 2 is 16 bytes");
        let (mut builder, system, _, payer, _) = registry_program();
        let other = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        builder.open_registry(other, None, payer, 2, 16, system);
        assert_eq!(verify_builder(&builder).unwrap().max_expanded_cpis, 6);

        // At most eight opens.
        let mut builder = ProgramBuilder::new();
        let system = builder.account(0, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
        let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let entries: Vec<u8> = (0..9).map(|_| builder.account(ACCOUNT_WRITABLE, None, None, 0)).collect();
        let pcs: Vec<usize> = entries
            .iter()
            .map(|entry| builder.open_registry(*entry, None, payer, 0, 8, system))
            .collect();
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pcs[8])));

        // Never after SET_RETURN_DATA.
        let mut builder = ProgramBuilder::new();
        let system = builder.account(0, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
        let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let tag = builder.blob(&[1]);
        builder.set_return_data(&[Segment::Literal(tag)]);
        let pc = builder.open_registry(entry, None, payer, 0, 8, system);
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)));

        // Reads and writes need an open of their account before them, and a field inside it.
        for (name, on_payer, offset, selector, write, fine) in [
            ("read past the end", false, 9, OP_READ_U64, false, false),
            ("read the last byte", false, 15, OP_READ_U8, false, true),
            ("read an account never opened", true, 0, OP_READ_U64, false, false),
            ("read with a selector that is not a read", false, 0, OP_ADD, false, false),
            ("write a u8", false, 0, OP_READ_U8, true, false),
            ("write a u64", false, 8, OP_READ_U64, true, true),
            ("write an i64 from a u64", false, 8, OP_READ_I64, true, false),
        ] {
            let (mut program, _, entry, payer, _) = registry_program();
            let value = program.const_u64(1);
            let account = if on_payer { payer } else { entry };
            let pc = next_pc(&mut program);
            if write {
                program.write_registry(account, offset, selector, value);
            } else {
                program.read_registry(account, offset, selector);
            }
            let expected = if fine { Ok(()) } else { Err(invalid(pc)) };
            assert_eq!(verify_builder(&program).map(|_| ()), expected, "{name}");
        }

        // A read before its open.
        let mut builder = ProgramBuilder::new();
        let system = builder.account(0, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
        let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        builder.read_registry(entry, 0, OP_READ_U64);
        builder.open_registry(entry, None, payer, 0, 8, system);
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(0)));

        // Spare operands, and a write's destination.
        let (mut builder, _, entry, _, _) = registry_program();
        let pc = next_pc(&mut builder);
        builder.read_registry(entry, 0, OP_READ_U64);
        builder.instructions_mut()[pc].b = 0;
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)));
        let (mut builder, _, entry, _, _) = registry_program();
        let value = builder.const_u64(1);
        let pc = builder.write_registry(entry, 0, OP_READ_U64, value);
        builder.instructions_mut()[pc].dst = 0;
        assert_eq!(verify_builder(&builder).map(|_| ()), Err(invalid(pc)));

        // Reads and writes work inside a loop body; the open stays at the root.
        let (mut builder, _, entry, _, _) = registry_program();
        let count = builder.const_u64(2);
        builder.repeat(count, 2, 0, |body| {
            let spent = body.read_registry(entry, 0, OP_READ_U64);
            body.write_registry(entry, 0, OP_READ_U64, spent);
        });
        assert!(verify_builder(&builder).is_ok());
    }
```

- [ ] **Step 3: Run it and confirm it fails.**

Run: `cargo test -p ballista-common --lib registry_rules`
Expected: FAIL at the first `verify_builder(&builder).unwrap()`, with
`InvalidInstruction(1)`: the verifier does not know opcode 75 yet.

- [ ] **Step 4: The verifier arms.** In `verify_instruction`, before the `OP_EQ | OP_NE` arm, add:

```rust
            OP_OPEN_REGISTRY => {
                self.verify_open_registry(instruction, instruction_index, scope, registers)?;
                // Creating a pre-funded entry takes three CPIs; an open never encodes CPI data.
                return Ok((REGISTRY_OPEN_CPIS, 0));
            }
            OP_READ_REGISTRY => {
                if instruction.b != NO_INDEX || instruction.c != NO_INDEX {
                    return Err(TemplateError::InvalidRegistry(instruction_index));
                }
                let field = self.registry_field(instruction, instruction_index, instruction.a, false)?;
                self.write_register(registers, instruction.dst, scalar(read_type(field.selector)))?;
            }
            OP_WRITE_REGISTRY => {
                let invalid = TemplateError::InvalidRegistry(instruction_index);
                if instruction.dst != NO_INDEX || instruction.c != NO_INDEX {
                    return Err(invalid);
                }
                let field = self.registry_field(instruction, instruction_index, instruction.b, true)?;
                let value = self.read_register(registers, instruction.a).map_err(|_| invalid)?;
                if value.value_type != read_type(field.selector) {
                    return Err(invalid);
                }
            }
```

- [ ] **Step 5: The two helpers.** After `verify_output` in the `impl ProgramView` add:

```rust
    /// `OPEN_REGISTRY`, every rule `InvalidRegistry`: at the root; a valid [`RegistryOpen`] with a
    /// registry index below [`MAX_REGISTRIES`] and 1 to [`MAX_REGISTRY_SIZE`] field bytes; no
    /// destination; the entry a fixed account declared writable and not pinned; the payer a fixed
    /// account declared signer and writable; the System program account fixed and pinned to the
    /// System program; the key [`NO_INDEX`] or a set `pubkey` register. Against the opens before
    /// it: its entry account not opened already, the same size as any open of the same registry
    /// index, at most [`MAX_REGISTRY_OPENS`] in all, and no `SET_RETURN_DATA` before it, since
    /// creating an entry calls the System program and a CPI clears return data.
    ///
    /// The opens before it are found by scanning, as `SET_RETURN_DATA` scans the records after it:
    /// the per-instruction signature Certora verifies against has no room for a slot table.
    fn verify_open_registry(
        &self,
        instruction: &InstructionRecord,
        instruction_index: usize,
        scope: LoopScope,
        registers: &[Option<RegisterInfo>; MAX_REGISTERS],
    ) -> Result<(), TemplateError> {
        let invalid = TemplateError::InvalidRegistry(instruction_index);
        let open = RegistryOpen::decode(instruction.immediate()).ok_or(invalid)?;
        if scope != LoopScope::Root
            || instruction.dst != NO_INDEX
            || usize::from(open.index) >= MAX_REGISTRIES
            || !(1..=MAX_REGISTRY_SIZE).contains(&usize::from(open.size))
        {
            return Err(invalid);
        }
        // `in_row_loop` is false, so a row account never matches.
        let entry = self.account_constraint(instruction.a, false).ok_or(invalid)?;
        if entry.flags & ACCOUNT_WRITABLE == 0 || entry.address_index != NO_INDEX {
            return Err(invalid);
        }
        let payer = self.account_constraint(instruction.c, false).ok_or(invalid)?;
        let signer_and_writable = ACCOUNT_SIGNER | ACCOUNT_WRITABLE;
        if payer.flags & signer_and_writable != signer_and_writable {
            return Err(invalid);
        }
        let system_program = self
            .account_constraint(open.system_program, false)
            .filter(|constraint| constraint.address_index != NO_INDEX)
            .and_then(|constraint| self.pubkeys.get(constraint.address_index as usize))
            .is_some_and(|address| address.bytes == SYSTEM_PROGRAM_ADDRESS);
        if !system_program {
            return Err(invalid);
        }
        if instruction.b != NO_INDEX
            && !matches!(
                self.read_register(registers, instruction.b),
                Ok(info) if info.value_type == VALUE_PUBKEY
            )
        {
            return Err(invalid);
        }
        let mut opens = 0usize;
        for record in self.instructions.get(..instruction_index).unwrap_or(&[]) {
            match record.opcode {
                OP_SET_RETURN_DATA => return Err(invalid),
                OP_OPEN_REGISTRY => {
                    opens += 1;
                    let earlier = RegistryOpen::decode(record.immediate()).ok_or(invalid)?;
                    if record.a == instruction.a
                        || (earlier.index == open.index && earlier.size != open.size)
                    {
                        return Err(invalid);
                    }
                }
                _ => {}
            }
        }
        if opens >= MAX_REGISTRY_OPENS {
            return Err(invalid);
        }
        Ok(())
    }

    /// The field a `READ_REGISTRY` or `WRITE_REGISTRY` names, every rule `InvalidRegistry`: a
    /// valid [`RegistryField`] whose selector is a read opcode (for a write, one of the five whose
    /// width holds every value of its type), inside the size the open of `entry` at a lower pc
    /// declared. Opens are only at the root and loops run forward, so that open has run by the
    /// time this instruction does, wherever it sits.
    fn registry_field(
        &self,
        instruction: &InstructionRecord,
        instruction_index: usize,
        entry: u8,
        write: bool,
    ) -> Result<RegistryField, TemplateError> {
        let invalid = TemplateError::InvalidRegistry(instruction_index);
        let field = RegistryField::decode(instruction.immediate()).ok_or(invalid)?;
        let width = read_width(field.selector);
        let holds_its_type = matches!(
            field.selector,
            OP_READ_BOOL | OP_READ_U64 | OP_READ_I64 | OP_READ_U128 | OP_READ_PUBKEY
        );
        if width == 0 || (write && !holds_its_type) {
            return Err(invalid);
        }
        let size = self
            .instructions
            .get(..instruction_index)
            .unwrap_or(&[])
            .iter()
            .find(|record| record.opcode == OP_OPEN_REGISTRY && record.a == entry)
            .and_then(|record| RegistryOpen::decode(record.immediate()))
            .ok_or(invalid)?
            .size;
        if !valid_range(usize::from(size), usize::from(field.offset), width) {
            return Err(invalid);
        }
        Ok(field)
    }
```

- [ ] **Step 6: Run the verifier tests.**

```bash
cargo test -p ballista-common --lib
```

Expected: PASS, `registry_rules` included, and every existing test unchanged. If the unknown
opcode sweep in `every_opcode_rejects_uninitialized_or_mistyped_operands` reports 75, 76 or 77,
Task 1's sweep change is missing.

- [ ] **Step 7: Commit.**

```bash
git add common/src/template/builder.rs common/src/template/verify.rs
git commit -m "$(cat <<'MSG'
Verify the registry opcodes at finalize and add them to the builder

An open sits at the root, once per entry account, at most eight, never after SET_RETURN_DATA,
and names a writable unpinned entry, a signing writable payer and the System program. It counts
three CPIs. A field read or write names an account opened at a lower pc and stays inside its
size; a write takes only the five widths that hold every value of their type.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 3: The run path

**Files:**
- `programs/ballista/src/utils/pda.rs`: `get_registry_address` and its host test.
- `programs/ballista/src/processor/registry.rs`: **new**, with host tests.
- `programs/ballista/src/processor/mod.rs`: declare it.
- `programs/ballista/src/processor/execute.rs`: `run`, `Machine`, `execute_root`,
  `execute_instruction`, the router arm, `registry_instruction`, `reentry_or`, one test's
  `Machine`.
- `programs/ballista/src/lib.rs`: `run_template`.

- [ ] **Step 1: The address, test first.** In `pda.rs`'s `mod tests`, after
  `template_addresses_match_find_program_address`, add:

```rust
    #[test]
    fn registry_addresses_match_find_program_address() {
        let mut rng = SplitMix(11);
        for _ in 0..500 {
            let template = rng.address();
            let index = (rng.next() % 8) as u8;
            let key = rng.address().to_bytes();
            assert_eq!(
                get_registry_address(&template, index, &key),
                Some(Address::find_program_address(
                    &[REGISTRY_SEED, template.as_ref(), &[index], &key],
                    &crate::ID
                ))
            );
        }
    }
```

Run: `cargo test -p ballista --lib registry_addresses` — Expected: compile error,
`get_registry_address` and `REGISTRY_SEED` not found.

  Then, after `get_template_address`, add:

```rust
/// Seed bytes of a registry entry: the tag, the template, the registry index and the key.
const REGISTRY_PREIMAGE_LEN: usize = preimage_capacity(REGISTRY_SEED.len() + 32 + 1 + 32);

/// A registry entry's address and canonical bump, exactly what `Address::find_program_address`
/// returns for `["registry", template, [index], key]`. Only an open that creates an entry derives
/// it; an existing entry is proven by its header. `None` only if no bump from 255 down works.
#[inline(never)]
pub fn get_registry_address(template: &Address, index: u8, key: &[u8; 32]) -> Option<(Address, u8)> {
    let mut preimage = Preimage::<REGISTRY_PREIMAGE_LEN>::new();
    preimage.push_seed(REGISTRY_SEED)?;
    preimage.push_seed(template.as_ref())?;
    preimage.push_seed(&[index])?;
    preimage.push_seed(key)?;
    preimage.find(&crate::ID)
}
```

  and at the top of the file, `use ballista_common::template::REGISTRY_SEED;` beside the other
  imports (the test module's `use super::*;` then sees it). Run the test again: PASS.

- [ ] **Step 2: The registry module.** Create `programs/ballista/src/processor/registry.rs`:

```rust
//! Registry entries: accounts Ballista owns, one per template, registry and key, that only runs
//! of their template can write.
//!
//! An entry is a 72-byte header, then the registry's fields, zeroed when it is created:
//!
//! | Bytes | Holds |
//! | --- | --- |
//! | 0..4 | `BREG` |
//! | 4 | version, 1 |
//! | 5 | the registry index |
//! | 6..8 | zero |
//! | 8..40 | the template's address |
//! | 40..72 | the key |
//!
//! Its address is `find_program_address(["registry", template, [index], key], ballista)`. Only
//! [`create_entry`] writes a header, and only at that address, and only Ballista can write the data
//! of an account it owns. So a Ballista-owned account whose header names this template, registry
//! and key is that entry, and an open checks the header instead of deriving the address again.
use ballista_common::template::{
    OP_READ_BOOL, OP_READ_I64, OP_READ_PUBKEY, OP_READ_U128, OP_READ_U64,
    REGISTRY_ENTRY_HEADER_LEN, REGISTRY_ENTRY_MAGIC, REGISTRY_ENTRY_VERSION, REGISTRY_SEED,
};
use pinocchio::{
    cpi::{Seed, Signer},
    sysvars::{rent::Rent, Sysvar},
    AccountView, Address,
};
use pinocchio_system::instructions::{Allocate, Assign, CreateAccount, Transfer};

use super::execute::{RunResult, RuntimeValue};
use crate::{error::BallistaError, utils::pda::get_registry_address};

/// What names an entry: the running template, the registry index and the key.
pub struct EntryId<'a> {
    pub template: &'a Address,
    pub index: u8,
    pub key: [u8; 32],
}

impl EntryId<'_> {
    /// The header this entry holds.
    pub fn header(&self) -> [u8; REGISTRY_ENTRY_HEADER_LEN] {
        let mut header = [0; REGISTRY_ENTRY_HEADER_LEN];
        header[..4].copy_from_slice(&REGISTRY_ENTRY_MAGIC);
        header[4] = REGISTRY_ENTRY_VERSION;
        header[5] = self.index;
        header[8..40].copy_from_slice(self.template.as_ref());
        header[40..].copy_from_slice(&self.key);
        header
    }
}

/// Checks the entry `id` names in `entry`, or creates it with `payer`'s lamports, then marks the
/// entry's data exclusively borrowed for the rest of the run.
///
/// The mark is Ballista's record that the entry is open. `READ_REGISTRY` and `WRITE_REGISTRY`
/// require it, and every CPI meets it: the invocation refuses a writable account whose data is
/// borrowed, so no CPI can pass an open entry writable, and without that no nested Ballista run can
/// write the entry between this run's read and its write. The runtime refuses every other way back
/// into Ballista: a CPI chain may not re-enter a program deeper in the stack. The mark is never
/// cleared. It is a byte of the account's input region that the runtime does not read back.
#[inline(never)]
pub fn open(entry: &AccountView, payer: &AccountView, id: &EntryId, size: usize) -> RunResult<()> {
    if !entry.is_writable() {
        return Err(BallistaError::InvalidRegistryEntry.into());
    }
    if entry.owned_by(&crate::ID) {
        check_entry(entry, id, size)?;
    } else {
        create_entry(payer, entry, id, size)?;
    }
    // A second open of the same account, a duplicate in the transaction, finds it marked already.
    if !entry.is_borrowed_mut() {
        let mut view = *entry;
        let borrowed = view
            .try_borrow_mut()
            .map_err(|_| BallistaError::InvalidRegistryEntry)?;
        core::mem::forget(borrowed);
    }
    Ok(())
}

/// Checks an entry that exists: its size, and its header against `id`'s.
fn check_entry(entry: &AccountView, id: &EntryId, size: usize) -> RunResult<()> {
    // SAFETY: no reference into the entry's data outlives the opcode that took it, so none is
    // held while this one is.
    let data = unsafe { entry.borrow_unchecked() };
    if data.len() != REGISTRY_ENTRY_HEADER_LEN + size
        || data[..REGISTRY_ENTRY_HEADER_LEN] != id.header()
    {
        return Err(BallistaError::InvalidRegistryEntry.into());
    }
    Ok(())
}

/// Creates the entry `id` names in `entry`, paid for by `payer`. The only place a run signs.
///
/// The account must be the entry's canonical address, owned by the System program and empty. With
/// no lamports there, one `create_account` makes it rent-exempt and Ballista's. With lamports
/// already there, which anyone can send to block `create_account`, a transfer tops it up to rent
/// exemption and `allocate` and `assign` do the rest, as `create_template_account` does for a
/// template's address. The entry's seeds sign `create_account`, `allocate` and `assign`; the
/// transfer needs only the payer, whose signature the transaction carries. A template's own CPIs
/// go through `bounded_invoke`, which passes no seeds, so a template never holds a signature.
#[cold]
#[inline(never)]
fn create_entry(payer: &AccountView, entry: &AccountView, id: &EntryId, size: usize) -> RunResult<()> {
    if !entry.owned_by(&pinocchio_system::ID) || !entry.is_data_empty() {
        return Err(BallistaError::InvalidRegistryEntry.into());
    }
    let (address, bump) = get_registry_address(id.template, id.index, &id.key)
        .ok_or(BallistaError::InvalidRegistryEntry)?;
    if entry.address() != &address {
        return Err(BallistaError::InvalidRegistryEntry.into());
    }
    let index = [id.index];
    let bump = [bump];
    let seeds = [
        Seed::from(REGISTRY_SEED),
        Seed::from(id.template.as_ref()),
        Seed::from(index.as_ref()),
        Seed::from(id.key.as_ref()),
        Seed::from(bump.as_ref()),
    ];
    let signer = Signer::from(seeds.as_slice());
    let space = REGISTRY_ENTRY_HEADER_LEN + size;
    if entry.lamports() == 0 {
        CreateAccount::with_minimum_balance(payer, entry, space as u64, &crate::ID, None)?
            .invoke_signed(&[signer])?;
    } else {
        let required = Rent::get()?.try_minimum_balance(space)?;
        let shortfall = required.saturating_sub(entry.lamports());
        if shortfall > 0 {
            Transfer { from: payer, to: entry, lamports: shortfall }.invoke()?;
        }
        Allocate { account: entry, space: space as u64 }.invoke_signed(core::slice::from_ref(&signer))?;
        Assign { account: entry, owner: &crate::ID }.invoke_signed(&[signer])?;
    }
    let mut view = *entry;
    // SAFETY: the account is Ballista's now, `space` bytes long, and nothing holds a reference
    // into its data.
    let data = unsafe { view.borrow_unchecked_mut() };
    data.get_mut(..REGISTRY_ENTRY_HEADER_LEN)
        .ok_or(BallistaError::InvalidRegistryEntry)?
        .copy_from_slice(&id.header());
    Ok(())
}

/// Writes `value` into `entry`'s data at `offset`, which counts from the start of the data, with
/// `selector`'s width. The verifier bounds the field and the value's type; these checks keep a
/// template that got past it from writing outside the entry.
#[inline(always)]
pub fn write_field(
    entry: &AccountView,
    offset: usize,
    selector: u8,
    value: &RuntimeValue<'_>,
) -> RunResult<()> {
    let mut view = *entry;
    // SAFETY: nothing holds a reference into the entry's data, and this one ends with the write.
    let data = unsafe { view.borrow_unchecked_mut() };
    match (selector, value) {
        (OP_READ_U64, RuntimeValue::U64(value)) => put(data, offset, &value.to_le_bytes()),
        (OP_READ_I64, RuntimeValue::I64(value)) => put(data, offset, &value.to_le_bytes()),
        (OP_READ_U128, RuntimeValue::U128(value)) => put(data, offset, value),
        (OP_READ_PUBKEY, RuntimeValue::Pubkey(value)) => put(data, offset, value),
        (OP_READ_BOOL, RuntimeValue::Bool(value)) => put(data, offset, &[u8::from(*value)]),
        (_, RuntimeValue::Unset) => Err(BallistaError::InvalidRegister.into()),
        _ => Err(BallistaError::TypeMismatch.into()),
    }
}

/// Copies `bytes` to `data[offset..offset + N]`, whose length is known, so the copy is a few
/// stores rather than a `sol_memcpy_` call.
fn put<const N: usize>(data: &mut [u8], offset: usize, bytes: &[u8; N]) -> RunResult<()> {
    let field = data
        .get_mut(offset..)
        .and_then(|rest| rest.first_chunk_mut::<N>())
        .ok_or(BallistaError::InvalidTemplateProgram)?;
    *field = *bytes;
    Ok(())
}
```

- [ ] **Step 3: Declare it.** In `processor/mod.rs`, after the `math` pair add:

```rust

#[cfg(not(feature = "spec-api"))]
mod registry;
#[cfg(feature = "spec-api")]
pub mod registry;
```

- [ ] **Step 4: `run` takes the whole account list.** In `execute.rs`:
  - Replace `run`'s signature and first lines. The old parameters were `runtime_accounts:
    &'data [AccountView], template_address: &Address`:

```rust
/// Runs `program` against `accounts`, the instruction's whole account list: the template's account
/// first, then the runtime accounts the template names. Taking the list whole keeps the arguments
/// to five words, all passed in registers. The template's address as a sixth argument went on the
/// stack, and handing it on cost every run 2 compute units; this costs none, and the runs without
/// a CPI came out 1 unit cheaper.
pub fn run<'data>(
    program: &ProgramView<'data>,
    input_bytes: &'data [u8],
    accounts: &'data [AccountView],
) -> ProgramResult {
    let [template, runtime_accounts @ ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
```

  - The rest of `run` keeps using `runtime_accounts`. Pass `Some(template)` to `execute_root` as a
    new last argument, and in the event replace `template_address,` with `template.address(),`.
  - `Machine` gains, after `iterations`:

```rust
    /// The running template's account, which registry entries are keyed by. `None` when a test or
    /// a specification runs one instruction on its own through `execute_instruction`; an open then
    /// fails.
    template: Option<&'data AccountView>,
```

  - `execute_root` takes `template: Option<&'data AccountView>` as its last parameter and sets
    `template,`. The four tests that call `execute_root` directly pass `None` as that argument
    (`grep -n "execute_root(&program" programs/ballista/src/processor/execute.rs`).
    `execute_instruction` and the test that builds a `Machine` by hand
    (`every_loop_of_a_run_snapshots_into_one_buffer`) set `template: None,`.

- [ ] **Step 5: `run_template` hands over the list.** In `lib.rs`, replace the first lines of
  `run_template` and its call to `run`:

```rust
fn run_template(accounts: &mut [AccountView], input_bytes: &[u8]) -> ProgramResult {
    // Shared from here on: the run reads the template and lends the rest to the template's
    // instructions, and a registry open keys its entry by the template's address.
    let accounts: &[AccountView] = accounts;
    let [template, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };
```

  and `let result = processor::run(&program, input_bytes, accounts);`.

- [ ] **Step 6: The router arm and the helper.** In `extended_instruction`, before
  `_ => write_output(machine, instruction),` add:

```rust
        // A registry opcode names its entry by account, and all its work runs in one cold helper
        // that takes the machine and the instruction, two words in registers.
        OP_OPEN_REGISTRY | OP_READ_REGISTRY | OP_WRITE_REGISTRY => {
            registry_instruction(machine, instruction)
        }
```

  After `write_output` add:

```rust
/// `OPEN_REGISTRY`, `READ_REGISTRY` and `WRITE_REGISTRY`, reached through `extended_instruction`.
/// Cold and out of line, as `write_output` is: measured on `65c9b45`, the arm and this helper
/// cost no fixed case and no cookbook example anything.
///
/// An open checks or creates its entry and marks it open (see `registry::open`). A read or a write
/// requires that mark, which only an open in this run sets, then touches only its field. The
/// verifier guarantees the open before it, the field's range and the value's type; the checks
/// here keep memory safe regardless.
#[cold]
#[inline(never)]
fn registry_instruction(
    machine: &mut Machine<'_, '_>,
    instruction: &InstructionRecord,
) -> RunResult<()> {
    let immediate = instruction.immediate();
    if instruction.opcode == OP_OPEN_REGISTRY {
        let template = machine
            .template
            .ok_or(BallistaError::InvalidTemplateProgram)?;
        let open = RegistryOpen::decode(immediate).ok_or(BallistaError::InvalidTemplateProgram)?;
        let entry = resolve(machine.program, machine.accounts, instruction.a, None)?;
        let payer = resolve(machine.program, machine.accounts, instruction.c, None)?;
        let key = if instruction.b == NO_INDEX {
            [0; 32]
        } else {
            match get(machine.registers, instruction.b)? {
                RuntimeValue::Pubkey(key) => key,
                _ => return Err(BallistaError::TypeMismatch.into()),
            }
        };
        let id = registry::EntryId {
            template: template.address(),
            index: open.index,
            key,
        };
        return registry::open(entry, payer, &id, usize::from(open.size));
    }
    let account = if instruction.opcode == OP_READ_REGISTRY {
        instruction.a
    } else {
        instruction.b
    };
    let entry = resolve(machine.program, machine.accounts, account, None)?;
    if !entry.is_borrowed_mut() {
        return Err(BallistaError::InvalidTemplateProgram.into());
    }
    let offset = REGISTRY_ENTRY_HEADER_LEN + usize::from(immediate as u16);
    let selector = (immediate >> 16) as u8;
    if instruction.opcode == OP_READ_REGISTRY {
        // SAFETY: no reference into the entry's data outlives the opcode that took it.
        let data = unsafe { entry.borrow_unchecked() };
        let value = read_value(selector, data, offset)?;
        return set(machine.registers, instruction.dst as usize, value);
    }
    let value = operand(machine.registers, instruction.a)?;
    registry::write_field(entry, offset, selector, value)
}
```

  and add `registry` to `use super::{introspect, math};`.

- [ ] **Step 7: The reentry check.** In `invoke_cpi`, replace `invoked?;` with:

```rust
    invoked.map_err(|error| reentry_or(error, scratch.views.as_slice()))?;
```

  and after `invoke_cpi` add:

```rust
/// What a CPI that `bounded_invoke` refused reports. It refuses a writable account whose data is
/// borrowed, and during a run only a registry open marks an account's data exclusively borrowed,
/// so a refusal of a CPI that passes such an account is one that could let a nested run write an
/// open entry: `RegistryReentry`. Anything else passes through. (A CPI that passes an open entry
/// read-only and some other borrowed account writable, which only the template's own account can
/// be, reports `RegistryReentry` too; both fail the run.)
///
/// This is the whole reentry check. It runs only when an invocation fails, so a CPI that succeeds
/// pays nothing for it; a check of the invoked program before every CPI measured 3 compute units a
/// CPI, 89 on `run, payroll 30 rows`. It takes the views alone: given the whole `Scratch`, to
/// look at the metas' writable flags as well, the invocation path cost 14 units more per CPI.
#[cold]
#[inline(never)]
fn reentry_or(error: ProgramError, views: &[&AccountView]) -> RunError {
    if error == ProgramError::AccountBorrowFailed && views.iter().any(|view| view.is_borrowed_mut()) {
        return BallistaError::RegistryReentry.into();
    }
    error.into()
}
```

- [ ] **Step 8: Build.**

```bash
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
cargo test -p ballista --lib
```

Expected: both succeed; every existing executor test passes unchanged.

- [ ] **Step 9: Host tests.** Add to the end of `registry.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::processor::execute::{execute_instruction, RunError, Scratch};
    use ballista_common::template::{
        record, InstructionRecord, ProgramBuilder, ProgramView, RegistryField, ACCOUNT_WRITABLE,
        NO_INDEX, OP_READ_REGISTRY, OP_READ_U8, OP_WRITE_REGISTRY, SYSTEM_PROGRAM_ADDRESS,
    };
    use pinocchio::account::{RuntimeAccount, NOT_BORROWED};
    use RuntimeValue::{Bool, Pubkey, Unset, I64, U128, U64};

    const TEMPLATE: [u8; 32] = [7; 32];
    const KEY: [u8; 32] = [9; 32];

    fn err(kind: BallistaError) -> RunError {
        RunError::Vm(kind)
    }

    /// An account laid out as the entrypoint hands it over: the runtime header, then its data.
    struct TestAccount {
        buffer: Vec<u64>,
    }

    impl TestAccount {
        fn new(address: [u8; 32], owner: [u8; 32], lamports: u64, writable: bool, data: &[u8]) -> Self {
            let header = core::mem::size_of::<RuntimeAccount>();
            let mut buffer = vec![0u64; (header + data.len()).div_ceil(8)];
            let account = RuntimeAccount {
                borrow_state: NOT_BORROWED,
                is_signer: 0,
                is_writable: u8::from(writable),
                executable: 0,
                padding: [0; 4],
                address: Address::new_from_array(address),
                owner: Address::new_from_array(owner),
                lamports,
                data_len: data.len() as u64,
            };
            // SAFETY: the buffer is eight-aligned and holds the header and the data.
            unsafe {
                core::ptr::write(buffer.as_mut_ptr().cast::<RuntimeAccount>(), account);
                core::ptr::copy_nonoverlapping(
                    data.as_ptr(),
                    buffer.as_mut_ptr().cast::<u8>().add(header),
                    data.len(),
                );
            }
            Self { buffer }
        }

        fn view(&mut self) -> AccountView {
            // SAFETY: `new` wrote a runtime account header followed by its data.
            unsafe { AccountView::new_unchecked(self.buffer.as_mut_ptr().cast()) }
        }
    }

    fn id(template: &Address) -> EntryId<'_> {
        EntryId { template, index: 2, key: KEY }
    }

    /// An existing 16-byte entry of registry 2 for `KEY` in `TEMPLATE`, its data as given.
    fn entry_data(fields: &[u8; 16]) -> Vec<u8> {
        let template = Address::new_from_array(TEMPLATE);
        let mut data = id(&template).header().to_vec();
        data.extend_from_slice(fields);
        data
    }

    #[test]
    fn the_header_is_magic_version_index_zeros_template_key() {
        let template = Address::new_from_array(TEMPLATE);
        let header = id(&template).header();
        assert_eq!(&header[..8], b"BREG\x01\x02\x00\x00");
        assert_eq!(header[8..40], TEMPLATE);
        assert_eq!(header[40..], KEY);
    }

    #[test]
    fn an_existing_entry_opens_only_when_everything_matches() {
        let template = Address::new_from_array(TEMPLATE);
        let crate_id = crate::ID.to_bytes();
        let good = entry_data(&[0; 16]);
        let mut payer = TestAccount::new([1; 32], SYSTEM_PROGRAM_ADDRESS, 10, true, &[]);
        let payer = payer.view();

        let mut entry = TestAccount::new([3; 32], crate_id, 10, true, &good);
        let view = entry.view();
        assert_eq!(open(&view, &payer, &id(&template), 16), Ok(()));
        assert!(view.is_borrowed_mut(), "an open marks the entry");
        assert_eq!(open(&view, &payer, &id(&template), 16), Ok(()), "a duplicate opens again");

        let wrong = |name: &str, data: Vec<u8>, owner: [u8; 32], writable: bool, size: usize| {
            let mut entry = TestAccount::new([3; 32], owner, 10, writable, &data);
            assert_eq!(
                open(&entry.view(), &payer, &id(&template), size),
                Err(err(BallistaError::InvalidRegistryEntry)),
                "{name}"
            );
        };
        wrong("read-only", good.clone(), crate_id, false, 16);
        wrong("declared size differs", good.clone(), crate_id, true, 24);
        wrong("owned by another program", good.clone(), [4; 32], true, 16);
        let mut other = good.clone();
        other[0] = b'X';
        wrong("magic", other, crate_id, true, 16);
        let mut other = good.clone();
        other[4] = 2;
        wrong("version", other, crate_id, true, 16);
        let mut other = good.clone();
        other[5] = 3;
        wrong("registry index", other, crate_id, true, 16);
        let mut other = good.clone();
        other[8] ^= 1;
        wrong("another template's entry", other, crate_id, true, 16);
        let mut other = good.clone();
        other[40] ^= 1;
        wrong("another key's entry", other, crate_id, true, 16);
        wrong("System-owned with data", good.clone(), SYSTEM_PROGRAM_ADDRESS, true, 16);
    }

    #[test]
    fn a_missing_entry_must_sit_at_its_derived_address() {
        let template = Address::new_from_array(TEMPLATE);
        let mut payer = TestAccount::new([1; 32], SYSTEM_PROGRAM_ADDRESS, 10, true, &[]);
        // Any address but the PDA fails before a CPI, so this runs on the host.
        let mut entry = TestAccount::new([3; 32], SYSTEM_PROGRAM_ADDRESS, 0, true, &[]);
        assert_eq!(
            open(&entry.view(), &payer.view(), &id(&template), 16),
            Err(err(BallistaError::InvalidRegistryEntry))
        );
    }

    #[test]
    fn fields_are_written_at_their_width_and_type() {
        let mut entry = TestAccount::new([3; 32], crate::ID.to_bytes(), 10, true, &entry_data(&[0; 16]));
        let view = entry.view();
        let at = |offset: usize| REGISTRY_ENTRY_HEADER_LEN + offset;
        assert_eq!(write_field(&view, at(0), OP_READ_U64, &U64(5)), Ok(()));
        assert_eq!(write_field(&view, at(8), OP_READ_I64, &I64(-2)), Ok(()));
        assert_eq!(write_field(&view, at(15), OP_READ_BOOL, &Bool(true)), Ok(()));
        // SAFETY: nothing else borrows the test account.
        let data = unsafe { view.borrow_unchecked() };
        assert_eq!(&data[at(0)..at(8)], &5u64.to_le_bytes());
        assert_eq!(&data[at(8)..at(15)], &(-2i64).to_le_bytes()[..7]);
        assert_eq!(data[at(15)], 1);

        let mut wide_account = TestAccount::new([3; 32], crate::ID.to_bytes(), 10, true, &[0; 72 + 48]);
        let wide = wide_account.view();
        assert_eq!(write_field(&wide, at(0), OP_READ_U128, &U128([6; 16])), Ok(()));
        assert_eq!(write_field(&wide, at(16), OP_READ_PUBKEY, &Pubkey([8; 32])), Ok(()));

        assert_eq!(write_field(&view, at(0), OP_READ_U64, &I64(1)), Err(err(BallistaError::TypeMismatch)));
        assert_eq!(write_field(&view, at(0), OP_READ_U64, &Unset), Err(err(BallistaError::InvalidRegister)));
        assert_eq!(write_field(&view, at(0), OP_READ_U8, &U64(1)), Err(err(BallistaError::TypeMismatch)));
        assert_eq!(
            write_field(&view, at(9), OP_READ_U64, &U64(1)),
            Err(err(BallistaError::InvalidTemplateProgram)),
            "past the end of the data"
        );
    }

    /// Reads and writes through the executor: only an entry an open has marked, and each read
    /// typed as its selector.
    #[test]
    fn reads_and_writes_go_through_the_executor_on_an_open_entry() {
        let mut builder = ProgramBuilder::new();
        let entry_account = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let value = builder.const_u64(41);
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        let mut entry = TestAccount::new([3; 32], crate::ID.to_bytes(), 10, true, &entry_data(&[0; 16]));
        let accounts = [entry.view()];
        let mut scratch = Scratch::new(&program);
        let mut registers = vec![U64(41), Unset];
        let write = builder_record(OP_WRITE_REGISTRY, NO_INDEX, value, entry_account, 0, OP_READ_U64);
        let read = builder_record(OP_READ_REGISTRY, 1, entry_account, NO_INDEX, 0, OP_READ_U64);
        let read_byte = builder_record(OP_READ_REGISTRY, 1, entry_account, NO_INDEX, 1, OP_READ_U8);

        assert_eq!(
            execute_instruction(&program, &[], &accounts, &mut registers, &mut scratch, &write, None),
            Err(err(BallistaError::InvalidTemplateProgram)),
            "not opened in this run"
        );
        let template = Address::new_from_array(TEMPLATE);
        let mut payer = TestAccount::new([1; 32], SYSTEM_PROGRAM_ADDRESS, 10, true, &[]);
        open(&accounts[0], &payer.view(), &id(&template), 16).unwrap();
        for record in [write, read] {
            assert_eq!(
                execute_instruction(&program, &[], &accounts, &mut registers, &mut scratch, &record, None),
                Ok(())
            );
        }
        assert_eq!(registers[1], U64(41));
        assert_eq!(
            execute_instruction(&program, &[], &accounts, &mut registers, &mut scratch, &read_byte, None),
            Ok(())
        );
        assert_eq!(registers[1], U64(0), "the u64's second byte");
    }

    fn builder_record(opcode: u8, dst: u8, a: u8, b: u8, offset: u16, selector: u8) -> InstructionRecord {
        let field = RegistryField { offset, selector };
        record(opcode, dst, a, b, NO_INDEX, 0, field.encode())
    }
}
```

  Also, in `execute.rs`'s tests, add an open with no template to
  `opcodes_the_executor_does_not_run_fail_before_reading_operands`, after its loop:

```rust
        // An open run on its own has no template to key its entry by.
        assert_eq!(
            execute_instruction(
                &program,
                &[],
                &[],
                &mut registers,
                &mut scratch,
                &record(OP_OPEN_REGISTRY, NO_INDEX, 0, NO_INDEX, 0, 0, 1 << 8),
                None,
            ),
            Err(err(BallistaError::InvalidTemplateProgram))
        );
```

- [ ] **Step 10: Run the host tests.**

```bash
cargo test -p ballista --lib
cargo test -p ballista --lib --features spec-api
```

Expected: PASS. If `a_missing_entry_must_sit_at_its_derived_address` reaches a CPI, the address
check is missing: it must fail before `CreateAccount`.

- [ ] **Step 11: Commit.**

```bash
git add programs/ballista/src
git commit -m "$(cat <<'MSG'
Open, read and write registry entries on chain

An open checks an existing entry's owner, size and header, or creates a missing one at its
derived address from the payer's lamports, pre-funded or not, signing with the entry's seeds:
the only place a run signs. It leaves the entry's data marked borrowed, which reads and writes
require and which makes any CPI that passes the entry writable fail with RegistryReentry.

run() now takes the whole account list, so the template's address reaches the registry opcodes
without a sixth argument on the stack.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 4: The executor under Mollusk

The host tests cannot make a CPI, so creation, the pre-funded path, the header a creation writes
and `RegistryReentry` are tested here, with templates built from `ProgramBuilder`, uploaded and
finalized through Ballista's own instructions, so the on-chain verifier accepts each one first.

**Files:** `tests/ballista/src/lib.rs`: a nested `mod registry` at the end of `mod tests`.

- [ ] **Step 1: Write the tests.** At the end of `mod tests` in `tests/ballista/src/lib.rs` add:

```rust
    mod registry {
        use super::*;
        use ballista_common::template::{
            OP_READ_BOOL, OP_READ_I64, OP_READ_PUBKEY, OP_READ_U128, OP_READ_U64,
            SYSTEM_PROGRAM_ADDRESS, VALUE_BOOL, VALUE_I64, VALUE_PUBKEY, VALUE_U128, VALUE_U64,
        };

        const INVALID_REGISTRY_ENTRY: u32 = 6025;
        const REGISTRY_REENTRY: u32 = 6026;
        const ACCOUNT_CONSTRAINT_FAILED: u32 = 6020;

        /// The accounts every registry template here declares, in this order.
        struct Accounts {
            system: u8,
            payer: u8,
            entry: u8,
        }

        fn declare(builder: &mut ProgramBuilder) -> Accounts {
            Accounts {
                system: builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ADDRESS), None, 0),
                payer: builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0),
                entry: builder.account(ACCOUNT_WRITABLE, None, None, 0),
            }
        }

        /// Opens registry 0 (16 bytes) keyed by the payer, adds the `u64` input to field 0 and
        /// writes the clock to field 8.
        fn counter() -> Vec<u8> {
            let mut builder = ProgramBuilder::new();
            let accounts = declare(&mut builder);
            let input = builder.input(VALUE_U64, 0);
            let amount = builder.load_input(input);
            let key = builder.account_key(accounts.payer);
            builder.open_registry(accounts.entry, Some(key), accounts.payer, 0, 16, accounts.system);
            let spent = builder.read_registry(accounts.entry, 0, OP_READ_U64);
            let total = builder.binary(OP_ADD, spent, amount);
            builder.write_registry(accounts.entry, 0, OP_READ_U64, total);
            let now = builder.clock_timestamp();
            builder.write_registry(accounts.entry, 8, OP_READ_I64, now);
            builder.build().unwrap()
        }

        fn entry_address(template: &Pubkey, index: u8, key: &Pubkey) -> Pubkey {
            Pubkey::find_program_address(&[b"registry", template.as_ref(), &[index], key.as_ref()], &ID).0
        }

        /// A context with `creator` and `payers` funded and `payload` uploaded as template `id`.
        fn setup(payload: &[u8], id: u16, payers: &[Pubkey]) -> (MolluskContext<HashMap<Pubkey, Account>>, Pubkey) {
            let creator = Pubkey::new_unique();
            let mut accounts = funded_accounts([creator], 10_000_000_000);
            for payer in payers {
                accounts.insert(*payer, Account::new(1_000_000_000, 0, &system_program::id()));
            }
            let mut context = context(accounts);
            context.mollusk.sysvars.clock.unix_timestamp = 1_800_000_000;
            let created = context.process_instruction(&create_template_instruction(creator, id, payload));
            assert!(created.program_result.is_ok(), "{created:#?}");
            (context, find_template_pda(&creator, id).0)
        }

        fn run(
            context: &MolluskContext<HashMap<Pubkey, Account>>,
            template: Pubkey,
            payer: Pubkey,
            entry: AccountMeta,
            amount: u64,
        ) -> mollusk_svm::result::InstructionResult {
            let metas = vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(payer, true),
                entry,
            ];
            context.process_instruction(&run_instruction(template, metas, &amount.to_le_bytes()))
        }

        /// The error kind and program counter of a failed run.
        fn failure(result: &mollusk_svm::result::InstructionResult) -> (u32, u32) {
            let code = custom_code(result).unwrap_or_else(|| panic!("no custom error: {result:#?}"));
            (code & 0xffff, code >> 16)
        }

        fn account(context: &MolluskContext<HashMap<Pubkey, Account>>, address: Pubkey) -> Account {
            context.account_store.borrow()[&address].clone()
        }

        #[test]
        fn the_first_run_creates_the_entry_and_later_runs_reopen_it() {
            let payer = Pubkey::new_unique();
            let (context, template) = setup(&counter(), 1, &[payer]);
            let entry = entry_address(&template, 0, &payer);
            let rent = context.mollusk.sysvars.rent.minimum_balance(88);

            let result = run(&context, template, payer, AccountMeta::new(entry, false), 5);
            assert!(result.program_result.is_ok(), "{result:#?}");
            let created = account(&context, entry);
            assert_eq!(created.owner, ID);
            assert_eq!(created.lamports, rent, "an entry holds its rent and nothing else");
            assert_eq!(created.data.len(), 88);
            assert_eq!(&created.data[..8], b"BREG\x01\x00\x00\x00");
            assert_eq!(&created.data[8..40], template.as_ref());
            assert_eq!(&created.data[40..72], payer.as_ref());
            assert_eq!(created.data[72..80], 5u64.to_le_bytes());
            assert_eq!(created.data[80..88], 1_800_000_000i64.to_le_bytes());
            assert_eq!(lamports(&context, payer), 1_000_000_000 - rent, "the payer paid the rent");

            let result = run(&context, template, payer, AccountMeta::new(entry, false), 7);
            assert!(result.program_result.is_ok(), "{result:#?}");
            assert_eq!(account(&context, entry).data[72..80], 12u64.to_le_bytes());
            assert_eq!(lamports(&context, payer), 1_000_000_000 - rent, "no second charge");
        }

        #[test]
        fn a_pre_funded_address_is_topped_up_allocated_and_assigned() {
            for already in [1u64, 1_503_360, 2_000_000] {
                let payer = Pubkey::new_unique();
                let (context, template) = setup(&counter(), 2, &[payer]);
                let entry = entry_address(&template, 0, &payer);
                context
                    .account_store
                    .borrow_mut()
                    .insert(entry, Account::new(already, 0, &system_program::id()));
                let rent = context.mollusk.sysvars.rent.minimum_balance(88);
                let result = run(&context, template, payer, AccountMeta::new(entry, false), 5);
                assert!(result.program_result.is_ok(), "{already}: {result:#?}");
                let created = account(&context, entry);
                assert_eq!(created.owner, ID, "{already}");
                assert_eq!(created.data.len(), 88, "{already}");
                assert_eq!(created.lamports, rent.max(already), "{already}");
                assert_eq!(
                    lamports(&context, payer),
                    1_000_000_000 - rent.saturating_sub(already),
                    "{already}: the payer covers only the shortfall"
                );
                assert_eq!(created.data[72..80], 5u64.to_le_bytes(), "{already}");
            }
        }

        #[test]
        fn an_account_that_is_not_the_named_entry_fails() {
            let payer = Pubkey::new_unique();
            let other_payer = Pubkey::new_unique();
            let (context, template) = setup(&counter(), 3, &[payer, other_payer]);
            // `counter` loads its input (pc 0) and the payer's key (pc 1) before the open.
            let open_pc = 2;

            // Creation at an address that is not the entry's.
            let stranger = Pubkey::new_unique();
            let result = run(&context, template, payer, AccountMeta::new(stranger, false), 1);
            assert_eq!(failure(&result), (INVALID_REGISTRY_ENTRY, open_pc));

            // Create the payer's entry, then pass it for another payer: another key's entry.
            let entry = entry_address(&template, 0, &payer);
            assert!(run(&context, template, payer, AccountMeta::new(entry, false), 1).program_result.is_ok());
            let result = run(&context, template, other_payer, AccountMeta::new(entry, false), 1);
            assert_eq!(failure(&result), (INVALID_REGISTRY_ENTRY, open_pc));

            // The same code published at another address: another template's entry.
            let creator = Pubkey::new_unique();
            context.account_store.borrow_mut().insert(creator, Account::new(10_000_000_000, 0, &system_program::id()));
            assert!(context.process_instruction(&create_template_instruction(creator, 4, &counter())).program_result.is_ok());
            let copy = find_template_pda(&creator, 4).0;
            let result = run(&context, copy, payer, AccountMeta::new(entry, false), 1);
            assert_eq!(failure(&result), (INVALID_REGISTRY_ENTRY, open_pc));

            // An account another program owns.
            let foreign = Pubkey::new_unique();
            context.account_store.borrow_mut().insert(foreign, Account::new(1_000_000, 88, &Pubkey::new_unique()));
            let result = run(&context, template, payer, AccountMeta::new(foreign, false), 1);
            assert_eq!(failure(&result), (INVALID_REGISTRY_ENTRY, open_pc));

            // A read-only entry never reaches the open: the account check before the first
            // instruction refuses it, account 2, since the template declares the entry writable.
            let result = run(&context, template, payer, AccountMeta::new_readonly(entry, false), 1);
            assert_eq!(failure(&result), (ACCOUNT_CONSTRAINT_FAILED, 2));
        }

        /// A created entry is the header plus the size its template declares. (An existing entry of
        /// the wrong size needs a header that matches, which only the template's own entries have,
        /// and a template's sizes never change; the host test
        /// `an_existing_entry_opens_only_when_everything_matches` covers that check.)
        #[test]
        fn a_created_entry_is_the_header_plus_the_declared_size() {
            let mut builder = ProgramBuilder::new();
            let accounts = declare(&mut builder);
            let key = builder.account_key(accounts.payer);
            builder.open_registry(accounts.entry, Some(key), accounts.payer, 0, 24, accounts.system);
            let payer = Pubkey::new_unique();
            let (context, template) = setup(&builder.build().unwrap(), 5, &[payer]);
            let entry = entry_address(&template, 0, &payer);
            let metas = vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(payer, true),
                AccountMeta::new(entry, false),
            ];
            let result = context.process_instruction(&run_instruction(template, metas, &[]));
            assert!(result.program_result.is_ok(), "{result:#?}");
            assert_eq!(account(&context, entry).data.len(), 72 + 24);
        }

        #[test]
        fn every_writable_width_round_trips() {
            let mut builder = ProgramBuilder::new();
            let accounts = declare(&mut builder);
            let key = builder.account_key(accounts.payer);
            builder.open_registry(accounts.entry, Some(key), accounts.payer, 1, 65, accounts.system);
            // bool at 0, u64 at 1, i64 at 9, u128 at 17, pubkey at 33.
            let fields = [
                (VALUE_BOOL, OP_READ_BOOL, 0),
                (VALUE_U64, OP_READ_U64, 1),
                (VALUE_I64, OP_READ_I64, 9),
                (VALUE_U128, OP_READ_U128, 17),
                (VALUE_PUBKEY, OP_READ_PUBKEY, 33),
            ];
            for (value_type, selector, offset) in fields {
                let input = builder.input(value_type, 0);
                let value = builder.load_input(input);
                builder.write_registry(accounts.entry, offset, selector, value);
                let read = builder.read_registry(accounts.entry, offset, selector);
                let same = builder.binary(OP_EQ, read, value);
                builder.require(same);
            }
            let payer = Pubkey::new_unique();
            let (context, template) = setup(&builder.build().unwrap(), 6, &[payer]);
            let entry = entry_address(&template, 1, &payer);
            let mut inputs = vec![1u8];
            inputs.extend_from_slice(&u64::MAX.to_le_bytes());
            inputs.extend_from_slice(&(-3i64).to_le_bytes());
            inputs.extend_from_slice(&(u128::MAX - 1).to_le_bytes());
            inputs.extend_from_slice(&[4; 32]);
            let metas = vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(payer, true),
                AccountMeta::new(entry, false),
            ];
            let result = context.process_instruction(&run_instruction(template, metas, &inputs));
            assert!(result.program_result.is_ok(), "{result:#?}");
            assert_eq!(account(&context, entry).data[72..], inputs[..]);
        }

        /// A registry template that, after its open, invokes Ballista to run `inner` with the
        /// accounts `pass` names; `pass_entry` also passes its entry, writable.
        fn nested(pass_entry: bool) -> Vec<u8> {
            let mut builder = ProgramBuilder::new();
            let accounts = declare(&mut builder);
            let ballista = builder.account(ACCOUNT_EXECUTABLE, Some(ID.to_bytes()), None, 0);
            let inner = builder.account(0, None, None, 0);
            builder.open_registry(accounts.entry, None, accounts.payer, 0, 8, accounts.system);
            let mut cpi_accounts = vec![(inner, 0)];
            if pass_entry {
                cpi_accounts.push((accounts.entry, ACCOUNT_WRITABLE));
            }
            let data = builder.blob(&[IX_RUN]);
            let cpi = builder.cpi(ballista, &cpi_accounts, &[Segment::Literal(data)]);
            builder.invoke(cpi, None);
            builder.build().unwrap()
        }

        #[test]
        fn a_cpi_that_passes_an_open_entry_writable_fails_with_registry_reentry() {
            // The inner template asserts a constant and names no account.
            let mut inner = ProgramBuilder::new();
            let yes = inner.const_bool(true);
            inner.require(yes);
            let inner = inner.build().unwrap();
            for (pass_entry, id) in [(true, 7u16), (false, 8)] {
                let payer = Pubkey::new_unique();
                let (context, template) = setup(&nested(pass_entry), id, &[payer]);
                let creator = Pubkey::new_unique();
                context.account_store.borrow_mut().insert(creator, Account::new(10_000_000_000, 0, &system_program::id()));
                assert!(context.process_instruction(&create_template_instruction(creator, 1, &inner)).program_result.is_ok());
                let inner_template = find_template_pda(&creator, 1).0;
                let entry = entry_address(&template, 0, &Pubkey::default());
                let metas = vec![
                    AccountMeta::new_readonly(system_program::id(), false),
                    AccountMeta::new(payer, true),
                    AccountMeta::new(entry, false),
                    AccountMeta::new_readonly(ID, false),
                    AccountMeta::new_readonly(inner_template, false),
                ];
                let result = context.process_instruction(&run_instruction(template, metas, &[]));
                if pass_entry {
                    // The invoke is the second instruction, after the open.
                    assert_eq!(failure(&result), (REGISTRY_REENTRY, 1));
                } else {
                    assert!(result.program_result.is_ok(), "a CPI to Ballista that leaves the entry out runs: {result:#?}");
                }
            }
        }
    }
```

  The program counters follow from the builder: `counter()` emits `load_input` (pc 0) and
  `account_key` (pc 1) before its open (pc 2); `nested()` emits its open at pc 0 and its invoke at
  pc 1 (`blob` and `cpi` emit no instruction). `nested()` opens with no key, the zero key, which
  is `Pubkey::default()`.

- [ ] **Step 2: Run them.**

```bash
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
cargo test --manifest-path tests/ballista/Cargo.toml registry -- --nocapture
```

Expected: PASS. A failure in `a_pre_funded_address_is_topped_up_allocated_and_assigned` with
`already = 2_000_000` (above the rent) that reports a transfer means the shortfall check is
missing; one in the reentry test with `pass_entry` true that reports `AccountBorrowFailed`
instead of 6026 means `reentry_or` is not wired into `invoke_cpi`.

- [ ] **Step 3: Run the whole Mollusk suite.** The ceiling tests may fail here: Task 3 changed
  the run path, and Task 11 settles the ceilings. Everything else must pass.

```bash
cargo test --manifest-path tests/ballista/Cargo.toml 2>&1 | grep -E "test result|FAILED|panicked" | head -20
```

- [ ] **Step 4: Commit.**

```bash
git add tests/ballista/src/lib.rs
git commit -m "$(cat <<'MSG'
Test registry entries end to end under Mollusk

Creation with and without lamports already at the address, reopening, every writable width,
entries that belong to another payer or template, and a nested Ballista run that passes an open
entry writable, which fails with RegistryReentry, while one that leaves it out runs.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 5: The TypeScript schema

**Files:** `clients/js/src/schema.ts`; tests in `clients/js/src/compiler.test.ts`.

- [ ] **Step 1: Write the failing tests.** Add a `describe` block at the end of
  `compiler.test.ts` (the imports gain nothing yet; `defineTemplate`, `account`, `expression`
  and `step` are already there):

```ts
describe('registries: schema', () => {
  const limits = () =>
    defineTemplate({
      registries: { limits: { spent: 'u64', lastSpend: 'i64' } },
      accounts: {
        caller: { signer: true, writable: true },
        limits: account.registry('limits', { key: expression.accountKey('caller'), payer: 'caller' }),
        systemProgram: account.systemProgram(),
      },
      steps: [step.setRegistry('limits', 'spent', expression.u64(1))],
    });

  test('declares registries, registry accounts and the System program', () => {
    const template = limits();
    expect(template.registries).toEqual({ limits: { spent: 'u64', lastSpend: 'i64' } });
    expect(template.accounts.limits).toMatchObject({
      writable: true,
      signer: false,
      registry: { name: 'limits', payer: 'caller', key: expression.accountKey('caller') },
    });
    expect(template.accounts.systemProgram).toMatchObject({ executable: true, address: new Uint8Array(32) });
    expect(expression.accountKey('caller')).toEqual(expression.accountField(account.fixed('caller'), 'key'));
    expect(expression.registry('limits', 'spent')).toEqual({ kind: 'registry', account: 'limits', field: 'spent' });
    expect(step.setRegistry('limits', 'spent', expression.u64(2), 'charge')).toEqual({
      kind: 'setRegistry',
      account: 'limits',
      field: 'spent',
      value: expression.u64(2),
      label: 'charge',
    });
    expect(account.registry('limits', { payer: 'caller' })).toEqual({
      writable: true,
      registry: { name: 'limits', payer: 'caller' },
    });
  });

  test('a registry holds 1 to 512 bytes of the five writable types, and a template at most 8', () => {
    const withRegistries = (registries: Record<string, Record<string, string>>) => () =>
      defineTemplate({
        registries: registries as never,
        accounts: {},
        steps: [step.require(expression.bool(true))],
      });
    expect(withRegistries({ empty: {} })).toThrow(/1 to 512 bytes/);
    expect(withRegistries({ narrow: { count: 'u8' } })).toThrow();
    expect(withRegistries({ bytes: { blob: 'bytes' } })).toThrow();
    const sixteenKeys = Object.fromEntries(Array.from({ length: 16 }, (_, i) => [`k${i}`, 'pubkey']));
    expect(withRegistries({ full: sixteenKeys })).not.toThrow();
    expect(withRegistries({ over: { ...sixteenKeys, flag: 'bool' } })).toThrow(/1 to 512 bytes/);
    const nine = Object.fromEntries(Array.from({ length: 9 }, (_, i) => [`r${i}`, { flag: 'bool' }]));
    expect(withRegistries(nine)).toThrow(/at most 8 registries/);
  });

  test('registry accounts are fixed accounts', () => {
    expect(() =>
      defineTemplate({
        registries: { limits: { spent: 'u64' } },
        accounts: { caller: { signer: true, writable: true } },
        batch: { maxIterations: 2, row: { entry: account.registry('limits', { payer: 'caller' }) } },
        steps: [step.forEach([step.require(expression.bool(true))])],
      }),
    ).toThrow(/registry accounts are fixed accounts/);
  });
});
```

- [ ] **Step 2: Run them and confirm they fail.**

Run: `pnpm --dir clients/js exec vitest run src/compiler.test.ts -t "registries: schema"`
Expected: FAIL: `account.registry`, `account.systemProgram`, `expression.accountKey`,
`expression.registry` and `step.setRegistry` are not functions.

- [ ] **Step 3: The field types and the account constraint.** In `schema.ts`, after the
  `readWidth` table, add:

```ts
/** The types a registry field can hold: the five whose width holds every value of the type. */
export const RegistryFieldTypeSchema = z.enum(['bool', 'u64', 'i64', 'u128', 'pubkey']);
export type RegistryFieldType = z.infer<typeof RegistryFieldTypeSchema>;

/** The bytes a registry's fields take, packed in declaration order with no padding. */
export function registrySize(fields: Record<string, RegistryFieldType>): number {
  return Object.values(fields).reduce((size, type) => size + readWidth[type], 0);
}

/** A registry's fields, in declaration order: 1 to 512 bytes. */
const RegistryLayoutSchema = z
  .record(identifier, RegistryFieldTypeSchema)
  .refine((fields) => registrySize(fields) >= 1 && registrySize(fields) <= 512, {
    error: 'A registry holds 1 to 512 bytes of fields',
  });
```

  In `AccountConstraintSchema`'s object, after `unsafeUnpinned`, add:

```ts
    /**
     * Makes this account a registry entry; build it with `account.registry`. Every run opens the
     * entry of registry `name` for `key` before its first step, creating it if it does not exist
     * with `payer`'s lamports. `key` is a `pubkey` expression evaluated before the first step, or
     * absent for the one template-wide entry.
     */
    registry: z
      .object({
        name: identifier,
        key: z.lazy(() => ExpressionSchema).optional(),
        payer: identifier,
      })
      .strict()
      .optional(),
```

  and after `export type AccountConstraint = ...` add
  `export type AccountConstraintInput = z.input<typeof AccountConstraintSchema>;`.

- [ ] **Step 4: The expression and the step.** In the `Expression` type, after the
  `bytesLength` member, add:

```ts
  /**
   * A field of the registry entry in fixed account `account`, which must be declared with
   * `account.registry`. Typed as the field.
   */
  | { kind: 'registry'; account: string; field: string };
```

  (move the terminating `;` accordingly), and in `ExpressionSchema`'s union, after the
  `bytesLength` member:

```ts
    z.object({ kind: z.literal('registry'), account: identifier, field: identifier }).strict(),
```

  In the `Step` type, after the `setReturnData` member, add:

```ts
  | {
      /**
       * Writes `value` into a field of the registry entry in fixed account `account`. The write
       * lands at once; a run that fails later rolls it back with the transaction.
       */
      kind: 'setRegistry';
      account: string;
      field: string;
      value: Expression;
      label?: string;
    }
```

  and in `StepSchema`'s union, after `setReturnData`:

```ts
    z
      .object({ kind: z.literal('setRegistry'), account: identifier, field: identifier, value: ExpressionSchema, label })
      .strict(),
```

- [ ] **Step 5: `registries` in the template.** In `TemplateSchema`'s object, after `inputs`,
  add:

```ts
    /**
     * State that outlives a run, one entry per registry and key; the registry index is the
     * declaration order. `account.registry` names the entry a run opens.
     */
    registries: z
      .record(identifier, RegistryLayoutSchema)
      .default({})
      .refine((registries) => Object.keys(registries).length <= 8, {
        error: 'A template declares at most 8 registries',
      }),
```

  and in its `superRefine`, before the loops check, add:

```ts
    for (const [name, constraint] of Object.entries(template.batch?.row ?? {})) {
      if (constraint.registry !== undefined) {
        context.addIssue({
          code: 'custom',
          message: `${name}: registry accounts are fixed accounts`,
          path: ['batch', 'row', name],
        });
      }
    }
```

- [ ] **Step 6: The constructors.** Replace `export const account = { ... };` with:

```ts
export const account = {
  fixed: (name: string): AccountReference => AccountReferenceSchema.parse({ kind: 'account', name }),
  iteration: (name: string): AccountReference =>
    AccountReferenceSchema.parse({ kind: 'iterationAccount', name }),
  /**
   * An account holding the entry of `registry` for `options.key`, a `pubkey` (the template-wide
   * zero key when absent). `options.payer`, a fixed account declared signer and writable, pays
   * the entry's rent the first time. A declaration for `accounts`, not a reference.
   */
  registry: (registry: string, options: { key?: Expression; payer: string }): AccountConstraintInput => ({
    writable: true,
    registry: { name: registry, ...(options.key ? { key: options.key } : {}), payer: options.payer },
  }),
  /** The System program, pinned: a template with registry accounts declares it. */
  systemProgram: (): AccountConstraintInput => ({ executable: true, address: new Uint8Array(32) }),
};
```

  In `expression`, after `bytesLength`, add:

```ts
  /** A field of the registry entry in fixed account `account`, typed as the field. */
  registry: (accountName: string, field: string): Expression => ({ kind: 'registry', account: accountName, field }),
  /** Shorthand for `accountField(account.fixed(name), 'key')`. */
  accountKey: (name: string): Expression => ({ kind: 'accountField', account: { kind: 'account', name }, field: 'key' }),
```

  In `step`, after `setReturnData`, add:

```ts
  /** Writes `value` into a field of the registry entry in fixed account `account`. */
  setRegistry: (accountName: string, field: string, value: Expression, label?: string): Step => ({
    kind: 'setRegistry',
    account: accountName,
    field,
    value,
    ...(label ? { label } : {}),
  }),
```

- [ ] **Step 7: Keep the compiler type-checking.** Two fallthroughs in `compiler.ts` no longer
  type-check. `compileSteps` ends with `else { this.compileInvoke(current, loop, bindings); }`, and
  a step can now be `setRegistry`; `compileExpression` ends by treating every remaining kind as a
  binary operation, and an expression can now be `registry`. Until Task 6 lowers them, add before
  that `else`:

```ts
      } else if (current.kind === 'setRegistry') {
        throw new TypeError('setRegistry is not compiled yet');
```

  and in `compileExpression`, before the `select` branch:

```ts
    if (current.kind === 'registry') throw new TypeError('registry reads are not compiled yet');
```

- [ ] **Step 8: Run the tests.**

```bash
pnpm --dir clients/js exec vitest run src/compiler.test.ts -t "registries: schema"
pnpm --dir clients/js exec tsc --noEmit -p .
pnpm --dir clients/js exec vitest run
```

Expected: PASS, and every existing test unchanged.

- [ ] **Step 9: Commit.**

```bash
git add clients/js/src/schema.ts clients/js/src/compiler.ts clients/js/src/compiler.test.ts
git commit -m "$(cat <<'MSG'
Declare registries, registry accounts and their fields in the TypeScript schema

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 6: The TypeScript compiler

**Files:** `clients/js/src/compiler.ts`; tests in `clients/js/src/compiler.test.ts`.

- [ ] **Step 1: Write the failing tests.** Append to `compiler.test.ts`:

```ts
describe('registries: compiler', () => {
  const SYSTEM = new Uint8Array(32);
  const base = (steps: Step[], extra: Partial<TemplateInput> = {}): TemplateInput => ({
    inputs: { owner: { type: 'pubkey' } },
    registries: { flags: { on: 'bool' }, limits: { spent: 'u64', lastSpend: 'i64', holder: 'pubkey' } },
    accounts: {
      caller: { signer: true, writable: true },
      mine: account.registry('limits', { key: expression.accountKey('caller'), payer: 'caller' }),
      theirs: account.registry('limits', { key: expression.input('owner'), payer: 'caller' }),
      global: account.registry('flags', { payer: 'caller' }),
      systemProgram: account.systemProgram(),
    },
    steps,
    ...extra,
  });

  test('opens every entry before the first step, in declaration order', () => {
    const compiled = compileTemplate(
      base([step.require(expression.equal(expression.registry('global', 'on'), expression.bool(false)))]),
    );
    const opens = records(compiled).filter((record) => record[0] === opcode.openRegistry);
    // mine: accounts 1, key register, payer 0; registry 1 (limits), 48 bytes, System program 4.
    expect(Array.from(opens[0]!.slice(0, 6))).toEqual([opcode.openRegistry, 0xff, 1, opens[0]![3], 0, 0]);
    expect(readU64(opens[0]!, 6)).toBe(1n | (48n << 8n) | (4n << 24n));
    expect(opens[1]![2]).toBe(2);
    expect(readU64(opens[1]!, 6)).toBe(1n | (48n << 8n) | (4n << 24n));
    // global: no key, registry 0, one byte.
    expect(opens[2]![2]).toBe(3);
    expect(opens[2]![3]).toBe(0xff);
    expect(readU64(opens[2]!, 6)).toBe(0n | (1n << 8n) | (4n << 24n));
    const pcs = records(compiled).map((record) => record[0]);
    expect(pcs.lastIndexOf(opcode.openRegistry)).toBeLessThan(pcs.indexOf(opcode.readRegistry));
    expect(compiled.stats.maxExpandedCpis).toBe(9);
  });

  test('reads and writes fields at their packed offsets and types', () => {
    const compiled = compileTemplate(
      base([
        step.let('holder', expression.registry('theirs', 'holder')),
        step.setRegistry('mine', 'lastSpend', expression.clockUnixTimestamp()),
        step.setRegistry('mine', 'holder', expression.variable('holder')),
      ]),
    );
    const read = records(compiled).find((record) => record[0] === opcode.readRegistry)!;
    expect(read[2]).toBe(2);
    expect(readU64(read, 6)).toBe(16n | (BigInt(opcode.readPubkey) << 16n));
    const writes = records(compiled).filter((record) => record[0] === opcode.writeRegistry);
    expect(writes.map((record) => [record[1], record[3], readU64(record, 6)])).toEqual([
      [0xff, 1, 8n | (BigInt(opcode.readI64) << 16n)],
      [0xff, 1, 16n | (BigInt(opcode.readPubkey) << 16n)],
    ]);
  });

  test('refuses what the verifier or the run would refuse', () => {
    const fails = (input: TemplateInput, message: RegExp) =>
      expect(() => compileTemplate(input), String(message)).toThrow(message);
    const read = (accountName: string, field: string) => [
      step.require(expression.equal(expression.registry(accountName, field), expression.u64(0))),
    ];
    fails(base(read('caller', 'spent')), /caller is not a registry account/);
    fails(base(read('mine', 'missing')), /limits has no field missing/);
    fails(base([step.setRegistry('mine', 'spent', expression.i64(1))]), /spent is a u64/);
    const { systemProgram: _, ...withoutSystem } = base([]).accounts!;
    fails({ ...base(read('mine', 'spent')), accounts: withoutSystem }, /pinned to the System program/);
    fails(
      { ...base(read('mine', 'spent')), accounts: { ...base([]).accounts!, caller: { signer: true } } },
      /payer caller must be a fixed account declared signer and writable/,
    );
    fails(
      {
        ...base(read('mine', 'spent')),
        accounts: { ...base([]).accounts!, stray: account.registry('nothing', { payer: 'caller' }) },
      },
      /Unknown registry: nothing/,
    );
    fails(
      {
        ...base(read('mine', 'spent')),
        accounts: { ...base([]).accounts!, mine: { ...account.registry('limits', { payer: 'caller' }), signer: true } },
      },
      /mine must be declared only writable/,
    );
    fails(
      base([
        step.invoke({
          program: account.fixed('systemProgram'),
          programAddress: SYSTEM,
          accounts: [{ account: account.fixed('mine'), signer: false, writable: true }],
          data: [data.literal(Uint8Array.of(2, 0, 0, 0))],
        }),
      ]),
      /mine is a registry entry: a CPI that passes it writable fails with RegistryReentry/,
    );
  });
});
```

  (`records`, `readU64`, `opcode` and `TemplateInput` are already defined or imported in
  `compiler.test.ts`.)

- [ ] **Step 2: Run them and confirm they fail.**

Run: `pnpm --dir clients/js exec vitest run src/compiler.test.ts -t "registries: compiler"`
Expected: FAIL: no `OPEN_REGISTRY` records are emitted, and `setRegistry` throws "not compiled
yet".

- [ ] **Step 3: Constants and layouts.** In `compiler.ts`:
  - After `MAX_RETURN_DATA_LENGTH` add:

```ts
export const MAX_REGISTRIES = 8;
export const MAX_REGISTRY_OPENS = 8;
export const MAX_REGISTRY_SIZE = 512;
/** An entry's header before its fields: magic, version, registry index, template and key. */
export const REGISTRY_ENTRY_HEADER_LENGTH = 72;
/** The CPIs an open makes at most: a transfer, an allocate and an assign for a pre-funded entry. */
const REGISTRY_OPEN_CPIS = 3;
```

  - Import `registrySize` and `type RegistryFieldType` from `./schema.js`.
  - Add, near `instructionRecord`:

```ts
interface RegistryField {
  offset: number;
  type: RegistryFieldType;
}

interface RegistryLayout {
  index: number;
  size: number;
  fields: Map<string, RegistryField>;
}

/** `READ_REGISTRY`'s and `WRITE_REGISTRY`'s immediate: the offset, then the read opcode at byte 2. */
function registryFieldImmediate(field: RegistryField): bigint {
  return BigInt(field.offset) | (BigInt(readOpcode[field.type]) << 16n);
}
```

  - In the `Compiler` class, beside the other maps:

```ts
  /** Each registry's index, size and fields, in declaration order. */
  readonly registries = new Map<string, RegistryLayout>();
```

    and at the end of the constructor:

```ts
    for (const [index, [name, fields]] of Object.entries(template.registries).entries()) {
      let offset = 0;
      const layout: RegistryLayout = { index, size: registrySize(fields), fields: new Map() };
      for (const [field, type] of Object.entries(fields)) {
        layout.fields.set(field, { offset, type });
        offset += readWidth[type];
      }
      this.registries.set(name, layout);
    }
```

    (import `readWidth` from `./schema.js` if `compiler.ts` does not already).

- [ ] **Step 4: The opens.** In `compile()`, after the constants loop and before
  `this.location = { path: 'template' };`, call `this.compileRegistryOpens();`, and add the
  method after `compileInput`:

```ts
  /**
   * One `OPEN_REGISTRY` per registry account, in declaration order, after the hoisted inputs and
   * constants and before the first step: each key can be an input, an account's key or a constant,
   * and every open precedes any `setReturnData`, as the verifier requires.
   */
  compileRegistryOpens(): void {
    const entries = this.fixedEntries.filter(([, constraint]) => constraint.registry !== undefined);
    if (entries.length === 0) return;
    if (entries.length > MAX_REGISTRY_OPENS) {
      throw new RangeError(`A template opens at most ${MAX_REGISTRY_OPENS} registry entries`);
    }
    const systemProgram = this.fixedEntries.findIndex(
      ([, constraint]) => constraint.address !== undefined && constraint.address.every((byte) => byte === 0),
    );
    if (systemProgram < 0) {
      throw new TypeError(
        'Registry accounts need a fixed account pinned to the System program: declare one with account.systemProgram()',
      );
    }
    for (const [name, constraint] of entries) {
      const registry = constraint.registry!;
      this.location = { path: `accounts.${name}` };
      const layout = this.registries.get(registry.name);
      if (layout === undefined) throw new TypeError(`Unknown registry: ${registry.name}`);
      if (constraint.signer || constraint.executable || constraint.address || constraint.owner || constraint.minDataLength !== 0) {
        throw new TypeError(`${name} must be declared only writable: build it with account.registry`);
      }
      const payer = this.template.accounts[registry.payer];
      if (payer === undefined || !payer.signer || !payer.writable || payer.registry !== undefined) {
        throw new TypeError(`${name}'s payer ${registry.payer} must be a fixed account declared signer and writable`);
      }
      let key = NO_INDEX;
      if (registry.key !== undefined) {
        const value = this.compileExpression(registry.key, undefined, new Map());
        requireType(value, 'pubkey', `${name}'s key`);
        key = value.register;
      }
      const immediate = BigInt(layout.index) | (BigInt(layout.size) << 8n) | (BigInt(systemProgram) << 24n);
      this.pushInstruction(
        instructionRecord(opcode.openRegistry, NO_INDEX, this.fixedIndices.get(name)!, key, this.fixedIndices.get(registry.payer)!, immediate),
      );
    }
  }
```

  In `compile()`, replace the `maxExpandedCpis` line with:

```ts
    const opens = this.fixedEntries.filter(([, constraint]) => constraint.registry !== undefined).length;
    const maxExpandedCpis =
      worstCaseCpis(this.template.steps, this.template.batch?.maxIterations ?? 0) + REGISTRY_OPEN_CPIS * opens;
```

- [ ] **Step 5: Fields.** Add the lookup after `compileRegistryOpens`:

```ts
  /** The field `field` of the registry entry in fixed account `accountName`. */
  registryField(accountName: string, field: string): RegistryField {
    const registry = this.template.accounts[accountName]?.registry;
    if (registry === undefined) throw new TypeError(`${accountName} is not a registry account`);
    const found = this.registries.get(registry.name)?.fields.get(field);
    if (found === undefined) throw new TypeError(`${registry.name} has no field ${field}`);
    return found;
  }
```

  In `compileExpression`, replace Task 5's placeholder `registry` branch with:

```ts
    if (current.kind === 'registry') {
      const field = this.registryField(current.account, current.field);
      return this.emit(
        opcode.readRegistry,
        field.type,
        0,
        this.fixedIndices.get(current.account)!,
        NO_INDEX,
        NO_INDEX,
        registryFieldImmediate(field),
      );
    }
```

  In `compileSteps`, replace Task 5's placeholder branch with:

```ts
      } else if (current.kind === 'setRegistry') {
        const field = this.registryField(current.account, current.field);
        const value = this.compileExpression(current.value, loop, bindings);
        if (value.type !== field.type) {
          throw new TypeError(`${current.account}.${current.field}: ${current.field} is a ${field.type}, not a ${value.type}`);
        }
        this.pushInstruction(
          instructionRecord(
            opcode.writeRegistry,
            NO_INDEX,
            value.register,
            this.fixedIndices.get(current.account)!,
            NO_INDEX,
            registryFieldImmediate(field),
          ),
        );
```

- [ ] **Step 6: A registry account passed writable.** In `compileInvoke`'s loop over
  `current.accounts`, after the `writable` schema check, add:

```ts
      if (account.writable && account.account.kind === 'account' && this.template.accounts[account.account.name]?.registry) {
        throw new TypeError(
          `${account.account.name} is a registry entry: a CPI that passes it writable fails with RegistryReentry`,
        );
      }
```

- [ ] **Step 7: Run the tests.**

```bash
pnpm --dir clients/js exec vitest run src/compiler.test.ts
pnpm --dir clients/js exec tsc --noEmit -p .
pnpm fixtures && git diff --exit-code fixtures
```

Expected: PASS; the fixtures do not change, since no existing template declares a registry. The
test's `fails(...)` messages are matched as regular expressions: if a message reads differently,
fix the message in the compiler, not the test, unless the test's wording is wrong.

- [ ] **Step 8: Commit.**

```bash
git add clients/js/src/compiler.ts clients/js/src/compiler.test.ts
git commit -m "$(cat <<'MSG'
Compile registry opens, field reads and field writes from TypeScript

Every registry account opens before the first step, in declaration order, and counts three CPIs.
Field names resolve to packed offsets and typed reads; a write must match its field's type. The
compiler refuses a missing System program, a payer that cannot pay, and a CPI that passes an
open entry writable.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 7: The `rateLimit` helper

**Files:** `clients/js/src/helpers.ts`; tests in `clients/js/src/compiler.test.ts`.

- [ ] **Step 1: Write the failing test.** Add `rateLimit` to `compiler.test.ts`'s import from
  `./index.js`, and append:

```ts
describe('rateLimit', () => {
  // The spec's example, verbatim: 1 SOL a caller, refilling over a day. The cap and the rate are
  // literals; a caller who could pass them would set their own limit.
  const limited = () =>
    defineTemplate({
      inputs: { amount: { type: 'u64' } },
      registries: { limits: { spent: 'u64', lastSpend: 'i64' } },
      accounts: {
        caller: { signer: true, writable: true },
        limits: account.registry('limits', { key: expression.accountKey('caller'), payer: 'caller' }),
        systemProgram: account.systemProgram(),
      },
      steps: [
        ...rateLimit({
          registry: 'limits',
          cap: expression.u64(1_000_000_000),
          refillPerSecond: expression.u64(11_574),
          amount: expression.input('amount'),
        }),
      ],
    });

  test('refills in u128, requires withinRateLimit, and writes both fields back', () => {
    const steps = limited().steps;
    expect(steps.map((item) => (item.kind === 'let' ? `let ${item.name}` : item.kind))).toEqual([
      'let rateLimitLast',
      'let rateLimitNow',
      'let rateLimitSpent',
      'let rateLimitRefill',
      'let rateLimitTotal',
      'require',
      'setRegistry',
      'setRegistry',
    ]);
    // `now` is the clock, but never earlier than the last spend, and it is what the run writes
    // back: after a clock that steps back, `lastSpend` stays put, so no later run refills the same
    // seconds twice, and the elapsed time is never below zero.
    const last = expression.variable('rateLimitLast');
    const now = expression.variable('rateLimitNow');
    expect(steps[0]).toMatchObject({ value: expression.registry('limits', 'lastSpend') });
    expect(steps[1]).toMatchObject({ value: expression.max(expression.clockUnixTimestamp(), last) });
    expect(steps.at(-1)).toMatchObject({ kind: 'setRegistry', field: 'lastSpend', value: now });
    const compiled = compileTemplate(limited());
    // The caller supplies the amount alone.
    expect(compiled.inputOrder).toEqual(['amount']);
    expect(compiled.sourceMap.some((entry) => entry.label === 'withinRateLimit')).toBe(true);
    const kinds = records(compiled).map((record) => record[0]);
    expect(kinds.filter((kind) => kind === opcode.readRegistry)).toHaveLength(2);
    expect(kinds.filter((kind) => kind === opcode.writeRegistry)).toHaveLength(2);
    // Five u128 casts: the spent amount, the elapsed time, the rate, the new amount and the cap.
    expect(kinds.filter((kind) => kind === opcode.castU128)).toHaveLength(5);
    expect(compiled.stats.maxExpandedCpis).toBe(3);
  });

  test('names its variables and requirement after `name`, and takes other field names', () => {
    const steps = rateLimit({
      registry: 'limits',
      cap: expression.u64(10),
      refillPerSecond: expression.u64(1),
      amount: expression.u64(1),
      spent: 'used',
      lastSpend: 'at',
      name: 'daily',
    });
    expect(steps[0]).toMatchObject({ kind: 'let', name: 'dailyLast', value: expression.registry('limits', 'at') });
    expect(steps[1]).toMatchObject({ kind: 'let', name: 'dailyNow' });
    expect(steps.find((item) => item.kind === 'require')).toMatchObject({ label: 'withinDaily' });
    expect(steps.filter((item) => item.kind === 'setRegistry').map((item) => (item as { field: string }).field)).toEqual([
      'used',
      'at',
    ]);
  });
});
```

- [ ] **Step 2: Run it and confirm it fails.**

Run: `pnpm --dir clients/js exec vitest run src/compiler.test.ts -t rateLimit`
Expected: FAIL: `rateLimit` is not exported.

- [ ] **Step 3: The helper.** In `helpers.ts`, after `ed25519Signature`, add:

```ts
/**
 * Steps that spend `amount` from a limit that refills over time, kept in the registry entry in
 * fixed account `registry` (declared with `account.registry`).
 *
 * The entry's `spent` field (a `u64`) is what has been spent and not yet refilled, and its
 * `lastSpend` field (an `i64`) the Unix time of the last spend. Each run takes `now` as the clock,
 * or `lastSpend` if the clock reads earlier, refills `spent` by `(now − lastSpend) ×
 * refillPerSecond`, never below zero, adds `amount`, requires the total to be at most `cap`, and
 * writes `spent` and `now` back.
 *
 * - The refill is computed in `u128`: the elapsed seconds are below 2^63 and the rate below 2^64,
 *   so no gap between runs can overflow it. A fresh entry's `lastSpend` of 0 refills fully.
 * - The clock can step back between slots. A run then refills nothing, rather than failing, and
 *   leaves `lastSpend` where it was: it never moves back, so no later run refills the same seconds
 *   twice.
 * - `cap`, `refillPerSecond` and `amount` are `u64` expressions. `cap` and `refillPerSecond` must
 *   not come from the caller: whoever builds the transaction sets every input, so a cap taken from
 *   an input is a limit the caller picks. Pass literals, such as `expression.u64(1_000_000)`, or
 *   values the author controls, such as a registry field only an author-only branch writes.
 *
 * `name` prefixes the variables the steps bind, `<name>Last`, `<name>Now`, `<name>Spent`,
 * `<name>Refill` and `<name>Total`, and names the requirement `within<Name>`: `withinRateLimit` by
 * default.
 */
export function rateLimit(input: {
  registry: string;
  cap: Expression;
  refillPerSecond: Expression;
  amount: Expression;
  /** The entry's `u64` field of what has been spent. Default `spent`. */
  spent?: string;
  /** The entry's `i64` field of when it was last spent. Default `lastSpend`. */
  lastSpend?: string;
  name?: string;
}): Step[] {
  const name = input.name ?? 'rateLimit';
  const spentField = input.spent ?? 'spent';
  const lastSpendField = input.lastSpend ?? 'lastSpend';
  const u128 = (value: Expression) => expression.cast('u128', value);
  const last = expression.variable(`${name}Last`);
  const now = expression.variable(`${name}Now`);
  const spent = expression.variable(`${name}Spent`);
  const refill = expression.variable(`${name}Refill`);
  const total = expression.variable(`${name}Total`);
  return [
    step.let(`${name}Last`, expression.registry(input.registry, lastSpendField)),
    // `now` never reads earlier than `lastSpend`, so `now − lastSpend` is never negative, and the
    // `lastSpend` written back never moves back.
    step.let(`${name}Now`, expression.max(expression.clockUnixTimestamp(), last)),
    step.let(`${name}Spent`, u128(expression.registry(input.registry, spentField))),
    step.let(`${name}Refill`, expression.multiply(u128(expression.subtract(now, last)), u128(input.refillPerSecond))),
    step.let(
      `${name}Total`,
      expression.add(expression.subtract(spent, expression.min(spent, refill)), u128(input.amount)),
    ),
    step.require(
      expression.lessThanOrEqual(total, u128(input.cap)),
      `within${name.charAt(0).toUpperCase()}${name.slice(1)}`,
    ),
    step.setRegistry(input.registry, spentField, expression.cast('u64', total)),
    step.setRegistry(input.registry, lastSpendField, now),
  ];
}
```

- [ ] **Step 4: Run the tests.**

```bash
pnpm --dir clients/js exec vitest run src/compiler.test.ts
pnpm --dir clients/js exec tsc --noEmit -p .
```

Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add clients/js/src/helpers.ts clients/js/src/compiler.test.ts
git commit -m "$(cat <<'MSG'
Add the rateLimit helper: a refilling spend limit kept in a registry entry

The refill is computed in u128, never goes below zero, and refills a fresh entry fully; the run
requires withinRateLimit and writes the spent amount and the time back.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 8: A rate-limited transfer, end to end

The spec's end-to-end test: a TypeScript-compiled template, uploaded and run under Mollusk. The
first run creates the entry and the caller pays its rent; later runs spend within the cap; one
over the cap fails at `withinRateLimit`; after the clock moves, the refill lets it land.

**Files:**
- `clients/js/src/fixtures.test.ts`: the `rate-limited-transfer` fixture.
- `fixtures/rate-limited-transfer.hex`, `fixtures/manifest.json` (generated).
- `common/src/template/verify.rs`: the shared fixture list.
- `tests/ballista/src/lib.rs`: `fixture`'s match, and the test in `mod registry`.

- [ ] **Step 1: The fixture.** In `fixtures.test.ts`, import `rateLimit` from `./index.js`, and add
  to `fixtures` after `introspection`:

```ts
  /**
   * A transfer of `amount` lamports from the caller, capped per caller at 1,000,000 lamports that
   * refill at 10 a second. The Mollusk suite runs it across a clock change.
   */
  'rate-limited-transfer': () =>
    defineTemplate({
      inputs: { amount: { type: 'u64' } },
      registries: { limits: { spent: 'u64', lastSpend: 'i64' } },
      accounts: {
        caller: { signer: true, writable: true },
        recipient: { writable: true },
        limits: account.registry('limits', { key: expression.accountKey('caller'), payer: 'caller' }),
        systemProgram: account.systemProgram(),
      },
      steps: [
        ...rateLimit({
          registry: 'limits',
          cap: expression.u64(1_000_000),
          refillPerSecond: expression.u64(10),
          amount: expression.input('amount'),
        }),
        systemTransfer({
          systemProgram: account.fixed('systemProgram'),
          from: account.fixed('caller'),
          to: account.fixed('recipient'),
          lamports: expression.input('amount'),
          label: 'pay',
        }),
      ],
    }),
```

  Then generate it:

```bash
pnpm fixtures
git status --short fixtures
```

Expected: `fixtures/rate-limited-transfer.hex` is new and `fixtures/manifest.json` gains its
entry; nothing else changes.

- [ ] **Step 2: Verify it in Rust.** In `verify.rs`'s `every_shared_fixture_parses_and_verifies`,
  change `[(&str, &str); 19]` to `20` and add after the `signed-quote-settlement` entry:

```rust
            (
                "rate-limited-transfer",
                include_str!("../../../fixtures/rate-limited-transfer.hex"),
            ),
```

  and in `tests/ballista/src/lib.rs`'s `fixture` match, beside `"introspection"`:

```rust
            "rate-limited-transfer" => include_str!("../../../fixtures/rate-limited-transfer.hex"),
```

  Run: `cargo test -p ballista-common --lib every_shared_fixture` — Expected: PASS.

- [ ] **Step 3: The end-to-end test.** Add to `mod registry` in `tests/ballista/src/lib.rs`:

```rust
        /// The kind and the source label of a failed run of fixture `name`.
        fn labeled(result: &mollusk_svm::result::InstructionResult, name: &str) -> (u32, String) {
            let (kind, pc) = failure(result);
            let manifest: serde_json::Value =
                serde_json::from_str(include_str!("../../../fixtures/manifest.json")).expect("manifest");
            let label = manifest[name]["sourceMap"]
                .as_array()
                .and_then(|entries| entries.iter().find(|entry| entry["pc"] == pc))
                .and_then(|entry| entry["label"].as_str())
                .unwrap_or_default()
                .to_owned();
            (kind, label)
        }

        #[test]
        fn a_rate_limited_transfer_spends_within_its_cap_and_refills_with_time() {
            const REQUIREMENT_FAILED: u32 = 6015;
            let caller = Pubkey::new_unique();
            let recipient = Pubkey::new_unique();
            let (mut context, template) = setup(&fixture("rate-limited-transfer"), 20, &[caller, recipient]);
            let entry = entry_address(&template, 0, &caller);
            let pay = |context: &MolluskContext<HashMap<Pubkey, Account>>, amount: u64| {
                // The fixture's account order: caller, recipient, limits, systemProgram.
                let metas = vec![
                    AccountMeta::new(caller, true),
                    AccountMeta::new(recipient, false),
                    AccountMeta::new(entry, false),
                    AccountMeta::new_readonly(system_program::id(), false),
                ];
                context.process_instruction(&run_instruction(template, metas, &amount.to_le_bytes()))
            };
            let rent = context.mollusk.sysvars.rent.minimum_balance(72 + 16);

            // The first run creates the entry; the caller pays its rent and the transfer.
            let result = pay(&context, 600_000);
            assert!(result.program_result.is_ok(), "{result:#?}");
            assert_eq!(lamports(&context, caller), 1_000_000_000 - rent - 600_000);
            assert_eq!(account(&context, entry).owner, ID);
            // Up to the cap exactly.
            assert!(pay(&context, 400_000).program_result.is_ok());
            assert_eq!(lamports(&context, recipient), 1_000_000_000 + 1_000_000);
            // One lamport over.
            let result = pay(&context, 1);
            assert_eq!(labeled(&result, "rate-limited-transfer"), (REQUIREMENT_FAILED, "withinRateLimit".into()));

            // Ten seconds refill 100 lamports: 101 is still over, 100 lands.
            context.mollusk.sysvars.clock.unix_timestamp += 10;
            let result = pay(&context, 101);
            assert_eq!(labeled(&result, "rate-limited-transfer"), (REQUIREMENT_FAILED, "withinRateLimit".into()));
            assert!(pay(&context, 100).program_result.is_ok());
            let data = account(&context, entry).data;
            assert_eq!(data[72..80], 1_000_000u64.to_le_bytes());
            assert_eq!(data[80..88], 1_800_000_010i64.to_le_bytes());

            // A clock that steps back refills nothing, and does not fail the run on its own.
            context.mollusk.sysvars.clock.unix_timestamp -= 5;
            let result = pay(&context, 1);
            assert_eq!(labeled(&result, "rate-limited-transfer"), (REQUIREMENT_FAILED, "withinRateLimit".into()));
        }
```

  `setup` funds each payer it is given with 1,000,000,000 lamports, which is why the recipient
  starts there too. It returns the context by value; bind it `mut` here to move the clock.

- [ ] **Step 4: Run it.**

```bash
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
cargo test --manifest-path tests/ballista/Cargo.toml a_rate_limited_transfer -- --nocapture
pnpm --dir clients/js exec vitest run src/fixtures.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add clients/js/src/fixtures.test.ts fixtures common/src/template/verify.rs tests/ballista/src/lib.rs
git commit -m "$(cat <<'MSG'
Run a TypeScript-compiled rate-limited transfer end to end under Mollusk

The first run creates the caller's entry and pays its rent, runs spend up to the cap, one
lamport over fails at withinRateLimit, and ten seconds of clock refill exactly a hundred.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 9: Generated programs open, read and write a registry

The spec: "the generator produces registries, and generated programs never hit structural
errors." A generated program's registry is always its last three fixed accounts (System program,
payer, entry) with one fixed layout, so the Mollusk harness can supply them. The harness runs
each program twice, so the first run creates the entry and the second reopens it.

**Files:** `common/src/template/generate.rs`; `tests/ballista/src/lib.rs`
(`generated_programs_never_hit_structural_errors`).

- [ ] **Step 1: The registry description.** In `generate.rs`, after `GeneratedProgram`'s
  `min_iterations` field add:

```rust
    /// The registry the program opens, if any. Its accounts are the last three fixed accounts.
    pub registry: Option<GeneratedRegistry>,
```

  and after the struct:

```rust
/// A generated program's registry: its accounts are the last three fixed accounts, the System
/// program, the payer and the entry, in that order, and its fields are [`GENERATED_REGISTRY_FIELDS`].
#[derive(Clone, Copy, Debug)]
pub struct GeneratedRegistry {
    /// The registry index, below `MAX_REGISTRIES`.
    pub index: u8,
    /// Whether the entry is keyed by the payer's address; if not, by the zero key.
    pub keyed_by_payer: bool,
}

/// A generated registry's fields as `(type, read opcode, offset)`: disjoint, so a `bool` read
/// always finds a byte a `bool` write left, or the zero a creation left.
pub const GENERATED_REGISTRY_FIELDS: [(u8, u8, u16); 5] = [
    (VALUE_BOOL, OP_READ_BOOL, 0),
    (VALUE_U64, OP_READ_U64, 1),
    (VALUE_I64, OP_READ_I64, 9),
    (VALUE_U128, OP_READ_U128, 17),
    (VALUE_PUBKEY, OP_READ_PUBKEY, 33),
];
/// The bytes those fields take.
pub const GENERATED_REGISTRY_SIZE: u16 = 65;
```

- [ ] **Step 2: Declare and open it.** In `from_choices`, after the `accounts` of the generic
  fixed accounts are declared and before `let batched = ...`, add:

```rust
        // One program in three opens a registry. Drawn this early for the same reason as
        // `returns_data` below: the loops read many choices, and a draw after them is mostly zero.
        let registry = (choices.below(3) == 0).then(|| GeneratedRegistry {
            index: choices.below(MAX_REGISTRIES) as u8,
            keyed_by_payer: choices.below(2) == 1,
        });
        let registry_accounts = registry.map(|_| {
            let system = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ADDRESS), None, 0);
            let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
            let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
            (system, payer, entry)
        });
        let entry = registry_accounts.map(|(_, _, entry)| entry);
```

  After `registers.push(seed, VALUE_U64);` add:

```rust
        if let (Some(registry), Some((system, payer, entry))) = (registry, registry_accounts) {
            let key = registry.keyed_by_payer.then(|| builder.account_key(payer));
            if let Some(key) = key {
                registers.push(key, VALUE_PUBKEY);
            }
            builder.open_registry(entry, key, payer, registry.index, GENERATED_REGISTRY_SIZE, system);
        }
```

  In the returned `Self`, set `fixed_accounts: fixed_accounts + if registry.is_some() { 3 } else
  { 0 },` and `registry,`.

- [ ] **Step 3: Read and write it.** Give `emit_operation` and `emit_loop` a last parameter
  `entry: Option<u8>`, pass it through every call (`from_choices` passes `entry`, `emit_loop`
  passes its own `entry` to the body's `emit_operation`), change `match choices.below(17)` to
  `match choices.below(19)`, and add before the `_ =>` arm:

```rust
        17 => {
            if let Some(entry) = entry {
                let (value_type, selector, offset) =
                    GENERATED_REGISTRY_FIELDS[choices.below(GENERATED_REGISTRY_FIELDS.len())];
                let register = builder.read_registry(entry, offset, selector);
                registers.push(register, value_type);
            }
        }
        18 => {
            if let Some(entry) = entry {
                let (value_type, selector, offset) =
                    GENERATED_REGISTRY_FIELDS[choices.below(GENERATED_REGISTRY_FIELDS.len())];
                if let Some(value) = choices.pick(&registers.of_type(value_type)) {
                    builder.write_registry(entry, offset, selector, value);
                }
            }
        }
```

- [ ] **Step 4: Run the host property test.**

```bash
cargo test -p ballista-common --features proptest --test generated
```

Expected: PASS: every generated program verifies, registry or not. A failure names the
program's choices; `InvalidRegistry(pc)` there is a generator bug.

- [ ] **Step 5: Supply the registry's accounts in the Mollusk harness.** In
  `generated_programs_never_hit_structural_errors`, after `let (template, _) =
  find_template_pda(&creator, 1);` add:

```rust
                // A registry takes the last three fixed accounts: the System program, a payer
                // that signs and can pay the rent, and the entry at its derived address.
                let payer = Pubkey::new_unique();
                context
                    .account_store
                    .borrow_mut()
                    .insert(payer, Account::new(10_000_000_000, 0, &system_program::id()));
                let registry_metas = program.registry.map(|registry| {
                    let key = if registry.keyed_by_payer { payer } else { Pubkey::default() };
                    let (entry, _) = Pubkey::find_program_address(
                        &[b"registry", template.as_ref(), &[registry.index], key.as_ref()],
                        &ID,
                    );
                    [
                        AccountMeta::new_readonly(system_program::id(), false),
                        AccountMeta::new(payer, true),
                        AccountMeta::new(entry, false),
                    ]
                });
```

  and after `let metas: Vec<AccountMeta> = ... .collect();` (make it `let mut metas`) add:

```rust
                    if let Some(registry_metas) = &registry_metas {
                        let base = program.fixed_accounts - registry_metas.len();
                        metas[base..program.fixed_accounts].clone_from_slice(registry_metas);
                    }
```

  The two runs per program (`min_iterations`, then `max_iterations`) now create the entry and
  then reopen it.

- [ ] **Step 6: Run the Mollusk property test, with more cases once.**

```bash
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
PROPTEST_CASES=256 cargo test --manifest-path tests/ballista/Cargo.toml generated_programs_never_hit_structural_errors
cargo test --manifest-path tests/ballista/Cargo.toml generated_programs_never_hit_structural_errors
```

Expected: PASS both times. `InvalidRegistryEntry` (6025) in a failure means the harness passed
the wrong entry, payer or key; `TypeMismatch` (6012) means two fields overlap.

- [ ] **Step 7: Commit.**

```bash
git add common/src/template/generate.rs tests/ballista/src/lib.rs
git commit -m "$(cat <<'MSG'
Generate programs that open, read and write a registry

One program in three opens an entry, keyed by the payer or the zero key, and reads and writes
its five disjoint fields anywhere, loop bodies included. The Mollusk harness supplies the System
program, the payer and the entry, so the first run creates it and the second reopens it.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 10: Formal specifications

**Files:** `certora/ballista-specs/src/rules/typing.rs`.

- [ ] **Step 1: Mark the new errors value-dependent.** In `value_dependent`, add
  `| BallistaError::InvalidRegistryEntry` and `| BallistaError::RegistryReentry` to the
  `matches!`, and extend its doc comment:

```rust
/// The registry opcodes never reach the executor in either rule: neither spec program declares a
/// fixed account pinned to the System program, so the verifier rejects every open, and a field
/// read or write verified at pc 0 has no open before it. Their errors depend on the accounts a
/// run passes, not on the template's shape, so they are value-dependent too.
```

- [ ] **Step 2: Check and test.**

```bash
cargo check -p ballista-specs --features rt --manifest-path certora/Cargo.toml
cargo test --manifest-path certora/Cargo.toml -p ballista-specs --features rt 2>&1 | tail -3
```

Expected: both succeed. The prover itself runs in CI (`certora/` README); nothing here needs a
new rule, since the typing rule already quantifies over every opcode and the verifier rejects
these three against both spec programs.

- [ ] **Step 3: Commit.**

```bash
git add certora/ballista-specs/src/rules/typing.rs
git commit -m "$(cat <<'MSG'
Count the registry's runtime errors as value-dependent in the typing rule

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 11: Compute units

**Files:**
- `tests/ballista/src/cases.rs`: the `run, registry open and update` case.
- `fixtures/cu-ceilings.json`, `fixtures/example-ceilings.json`: **by hand**.
- `benches/CHANGELOG.md`.

What to expect, measured with this plan's code on a copy of `65c9b45`: every fixed case without
a CPI 1 CU cheaper (`run, empty template` 582 → 581, and the other eight), every case and example
with a CPI unchanged, `create template, payroll 30 rows` 4,483 → 4,485 (the verifier's new arms),
and the new case 1,632. Anything else moving means the dispatch or invocation shape changed: look
before raising a ceiling. A CPI costing 14 more, for instance, is `reentry_or` taking more than the
views.

- [ ] **Step 1: The ceiling case.** In `cases.rs`, add `OP_READ_I64` to the
  `ballista_common::template` import, add to `cases()` after the introspection case:

```rust
        ("run, registry open and update", registry_update(creator, 14)),
```

  and after `introspection`:

```rust
/// A run that opens an existing registry entry, reads two fields and writes both back: the steady
/// state of a rate limit, after the first run has created the entry.
fn registry_update(creator: Pubkey, template_id: u16) -> Case {
    let mut builder = ProgramBuilder::new();
    let system = builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
    let payer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    let entry = builder.account(ACCOUNT_WRITABLE, None, None, 0);
    let key = builder.account_key(payer);
    builder.open_registry(entry, Some(key), payer, 0, 16, system);
    let spent = builder.read_registry(entry, 0, OP_READ_U64);
    builder.read_registry(entry, 8, OP_READ_I64);
    let amount = builder.const_u64(100);
    let total = builder.binary(OP_ADD, spent, amount);
    builder.write_registry(entry, 0, OP_READ_U64, total);
    let now = builder.clock_timestamp();
    builder.write_registry(entry, 8, OP_READ_I64, now);

    let (template, _) = Pubkey::find_program_address(
        &[TEMPLATE_SEED, creator.as_ref(), &template_id.to_le_bytes()],
        &ID,
    );
    let (payer_key, payer_account) = funded(template_id * 100 + 1);
    let (address, _) = Pubkey::find_program_address(
        &[b"registry", template.as_ref(), &[0], payer_key.as_ref()],
        &ID,
    );
    let mut data = vec![0u8; 72 + 16];
    data[..8].copy_from_slice(b"BREG\x01\x00\x00\x00");
    data[8..40].copy_from_slice(template.as_ref());
    data[40..72].copy_from_slice(payer_key.as_ref());
    let mut entry_account = Account::new(1_503_360, data.len(), &ID);
    entry_account.data = data;
    run_case(
        creator,
        template_id,
        builder.build().expect("builds"),
        vec![
            (AccountMeta::new_readonly(system_program::id(), false), system_program_account()),
            (AccountMeta::new(payer_key, true), payer_account),
            (AccountMeta::new(address, false), entry_account),
        ],
        Vec::new(),
    )
}
```

  Template ID 14 is the first one no case uses (`grep -n "(creator, 1[0-9])\|upload(" cases.rs`
  to confirm).

- [ ] **Step 2: Measure every case and example exactly.** The ceiling tests, with empty ceiling
  files and `UPDATE_BENCHMARKS=1`, write each case's exact figure. Save the committed files first,
  and restore them and the benchmark results after: the runtime phases leave
  `fixtures/benchmark-results.json` to the docs session's `pnpm benchmarks`, which also writes
  under `docs/`.

```bash
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
mkdir -p target/registry-cu
cp fixtures/cu-ceilings.json target/registry-cu/cu-saved.json
cp fixtures/example-ceilings.json target/registry-cu/examples-saved.json
echo '{}' > fixtures/cu-ceilings.json
echo '{}' > fixtures/example-ceilings.json
UPDATE_BENCHMARKS=1 cargo test --manifest-path tests/ballista/Cargo.toml -- \
  compute_units_stay_under measure_every_example 2>&1 | grep -E "test result|FAILED"
cp fixtures/cu-ceilings.json target/registry-cu/cu-after.json
cp fixtures/example-ceilings.json target/registry-cu/examples-after.json
cp target/registry-cu/cu-saved.json fixtures/cu-ceilings.json
cp target/registry-cu/examples-saved.json fixtures/example-ceilings.json
git checkout -- fixtures/benchmark-results.json
git status --short fixtures
```

Expected: both tests pass (with empty files there is nothing to exceed), and `git status` shows
no change under `fixtures`.

- [ ] **Step 3: The before/after table.** Against the figures Task 0 saved:

```bash
node -e '
const fs = require("fs");
const read = (name) => JSON.parse(fs.readFileSync(`target/registry-cu/${name}.json`, "utf8"));
for (const [kind, before, after] of [["case", "cu-before", "cu-after"], ["example", "examples-before", "examples-after"]]) {
  const b = read(before), a = read(after);
  let total = 0;
  for (const name of Object.keys({ ...b, ...a }).sort()) {
    if (b[name] === a[name]) continue;
    const delta = b[name] === undefined ? "new" : String(a[name] - b[name]);
    if (b[name] !== undefined) total += a[name] - b[name];
    console.log(`| ${kind} | ${name} | ${b[name] ?? "—"} | ${a[name]} | ${delta} |`);
  }
  console.log(`| ${kind} | total of the changed rows | | | ${total} |`);
}'
```

  Paste the output into the commit message under a `| Kind | Case | Before | After | Change |`
  header. If a case with a CPI moved, or a run moved by more than 2 units, stop and compare the
  dispatch loop's and `extended_instruction`'s disassembly with the base before accepting it
  (`benches/CHANGELOG.md`, "How to measure").

- [ ] **Step 4: Edit the ceilings by hand.** In `fixtures/cu-ceilings.json`, add
  `"run, registry open and update": <measured>` in sorted position and set every row the table
  lists to its measured figure, lower or higher. Do the same in `fixtures/example-ceilings.json`
  for any example the table lists. Do not run `pnpm cu:ceilings` or `pnpm benchmarks`.

- [ ] **Step 5: The changelog.** In `benches/CHANGELOG.md`, under "## Pending: runtime
  extensions, not merged", add as the first entry, with the measured numbers:

```markdown
### 2026-09-28 · Registry entries · `claude/runtime-registry`
- **Change:** opcodes 75 to 77 reach one cold helper through `extended_instruction`'s fallback
  router; the outer dispatch is untouched. `run` takes the instruction's whole account list, so
  the template's address needs no sixth argument on the stack. An open entry is marked in its own
  borrow flag, so the reentry check is the borrow check every CPI already makes, mapped to
  `RegistryReentry` only when an invocation fails.
- **Measured** against the phase-4 tip (`65c9b45`):
  - The fixed cases without a CPI: <before> → <after> each (−1). Every case and example with a
    CPI: unchanged.
  - `create template, payroll 30 rows`: <before> → <after>, the verifier's three new arms.
  - `run, registry open and update`, a new case: <measured>, now ratcheted.
- **Measured on the prototype, and not built:** a slot table in `Scratch` cost every run 2 CU,
  in `Machine` 426 on a 30-pass count loop; the literal reentry check (the invoked program is
  Ballista and an entry is open) cost 3 CU a CPI, 89 on `run, payroll 30 rows`.
- **Checked:** the verifier's, executor's and registry module's unit tests; the Mollusk registry
  tests, the rate-limited transfer end to end, the generated-program property tests; both ceiling
  tests.
- **Watch:** a new field in `Machine` can cost the dispatch loop hundreds of units; one in
  `Scratch` costs every run about 2. Keep per-run registry state in the entry account itself.
```

- [ ] **Step 6: Run the ceiling tests.**

```bash
cargo test --manifest-path tests/ballista/Cargo.toml -- compute_units_stay_under measure_every_example
git status --short fixtures benches
```

Expected: PASS; only `fixtures/cu-ceilings.json`, possibly `fixtures/example-ceilings.json`, and
`benches/CHANGELOG.md` are modified.

- [ ] **Step 7: Commit, with the table.**

```bash
git add tests/ballista/src/cases.rs fixtures/cu-ceilings.json fixtures/example-ceilings.json benches/CHANGELOG.md
git commit -m "$(cat <<'MSG'
Measure the registry: a ceiling for open-and-update, and every case against phase 4

<paste Step 3's table here>

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
MSG
)"
```

---

### Task 12: Phase verification

- [ ] **Step 1: Everything, from a clean build.**

```bash
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
pnpm fixtures && git diff --exit-code fixtures
pnpm check
pnpm test
cargo test --manifest-path tests/ballista/Cargo.toml
PROPTEST_CASES=256 cargo test --manifest-path tests/ballista/Cargo.toml generated_programs_never_hit_structural_errors
cargo test -p ballista-common --features proptest --test generated
cargo test -p ballista --lib --features spec-api
cargo check -p ballista-specs --features rt --manifest-path certora/Cargo.toml
cargo test --manifest-path certora/Cargo.toml -p ballista-specs --features rt 2>&1 | tail -3
cargo clippy -p ballista -p ballista-common --all-targets 2>&1 | grep -c -E "^(warning|error)"
```

Expected: every command exits 0, and the clippy count is Task 0's: the registry code adds no
warning (`AccountView` is `Copy`, so the code copies it with `*entry` rather than `clone()`).

- [ ] **Step 2: Nothing outside the plan's files changed.**

```bash
git diff --stat claude/runtime-extensions...HEAD -- package.json pnpm-lock.yaml docs ':!docs/superpowers'
```

Expected: no output.

- [ ] **Step 3: Self-review against the spec.** For each line of the spec's "Verifier rules",
  "Errors", "Opening an entry" and "Testing" sections, name the test that covers it; add a test
  for any line without one before handing the branch over. Then use
  superpowers:requesting-code-review.

---

### Task 13: A per-caller daily cap on a real Jupiter swap (on `claude/protocol-tests`)

**Runs on `claude/protocol-tests`, after `claude/runtime-registry` has been merged into it.** That
merge happens separately; this task does not make it. Everything this task needs is below.

The template, `jupiterDailyCapSwap`, sells through a Jupiter `route` whose parts the caller hands
over, as `jupiterOracleCheckedSwap` does, and charges the route's `inAmount` against a cap kept in
a registry entry keyed by the caller: 1.728 SOL, refilling at 20,000 lamports a second, which is
the cap over a day. The template reassembles the route's data itself, so the `inAmount` it charges
is the one Jupiter sells. The test runs route `solToUsdc` (1 SOL for USDC) twice: the first swap
creates the entry and lands, the second fails at `withinRateLimit` until the clock has moved
exactly far enough (write rule 3).

**Files:**
- `clients/js/examples/protocols/jupiter-daily-cap-swap.ts`: **new**, the template.
- `clients/js/examples/protocols/index.ts`: export it.
- `clients/js/src/protocol-examples.test.ts`: the count.
- `clients/js/src/protocol-semantics.test.ts`: one test.
- `fixtures/protocol-examples.json` (generated).
- `clients/rust/examples/protocol_templates.rs`, `clients/rust/examples/protocol_templates_run.rs`:
  the Rust mirror and its run, which `clients/rust/tests/protocol_templates.rs` requires of every
  fixture entry.
- `tests/protocols/tests/jupiter_daily_cap.rs`: **new**, the test.

- [ ] **Step 1: Check the base.**

```bash
git switch claude/protocol-tests
git merge-base --is-ancestor claude/runtime-registry HEAD && echo merged
grep -n "pub fn open_registry" common/src/template/builder.rs
grep -n "export function rateLimit" clients/js/src/helpers.ts
grep -n "expect(entries.length).toBe" clients/js/src/protocol-examples.test.ts
grep -n "pub const TEMPLATES\|pub const RUNS" clients/rust/examples/protocol_templates*.rs
git lfs pull
pnpm install --frozen-lockfile
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
cargo test --manifest-path tests/protocols/Cargo.toml --test pyth_fresh_price_gate
```

Expected: `merged`; the builder method and the helper exist; note the example count and the two
array lengths (12 on `6155f0f`; use whatever they are, plus one, below); the Pyth gate's tests
pass, so the snapshot and LiteSVM work. If the branch is not merged, stop: this task needs it.

- [ ] **Step 2: The template.** Create `clients/js/examples/protocols/jupiter-daily-cap-swap.ts`:

```ts
/**
 * A per-caller daily cap on a Jupiter swap.
 *
 * Each caller may sell at most `DAILY_CAP` of a route's input, refilling at `REFILL_PER_SECOND`,
 * which is the cap over a day. What a caller has sold and when lives in a registry entry keyed by
 * the caller's address: the first run creates it, at the caller's expense, and only this
 * template's runs can change it.
 *
 * The caller hands over the route in parts, as `splitJupiterRoute` splits the Swap API's data,
 * and the template reassembles Jupiter's `route` data from them, so the `inAmount` it charges is
 * the `inAmount` Jupiter sells. It passes the route its token program and the actor itself, and
 * forwards the rest of the route's accounts as its group. The cap and the rate are constants: a
 * cap the caller could set would limit nothing.
 */
import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  data,
  defineTemplate,
  expression,
  rateLimit,
  step,
} from '../../src/index.js';
import { JUPITER_ROUTE, JUPITER_V6, addressBytes } from './shared.js';

/** 1.728 SOL, in lamports. */
export const DAILY_CAP = 1_728_000_000n;
/** The cap over 86,400 seconds. */
export const REFILL_PER_SECOND = 20_000n;

export const jupiterDailyCapSwap = defineTemplate({
  inputs: {
    routePlan: { type: 'bytes', maxLength: 512 },
    inAmount: { type: 'u64' },
    quotedOutAmount: { type: 'u64' },
    slippageBps: { type: 'u64' },
    platformFeeBps: { type: 'u64' },
  },
  registries: { dailySpend: { spent: 'u64', lastSpend: 'i64' } },
  accounts: {
    actionProgram: { executable: true, address: addressBytes(JUPITER_V6) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    actor: { signer: true, writable: true },
    spend: account.registry('dailySpend', { key: expression.accountKey('actor'), payer: 'actor' }),
    systemProgram: account.systemProgram(),
  },
  accountGroups: ['actionAccounts'],
  steps: [
    ...rateLimit({
      registry: 'spend',
      cap: expression.u64(DAILY_CAP),
      refillPerSecond: expression.u64(REFILL_PER_SECOND),
      amount: expression.input('inAmount'),
    }),
    step.invoke({
      program: account.fixed('actionProgram'),
      accounts: [
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('actor'), signer: true, writable: false },
      ],
      accountGroup: 'actionAccounts',
      data: [
        data.literal(JUPITER_ROUTE),
        data.encode('bytes', expression.input('routePlan')),
        data.encode('u64', expression.input('inAmount')),
        data.encode('u64', expression.input('quotedOutAmount')),
        data.encode('u16', expression.input('slippageBps')),
        data.encode('u8', expression.input('platformFeeBps')),
      ],
      label: 'swapWithinTheCap',
    }),
  ],
});

export const compiled = compileTemplate(jupiterDailyCapSwap);
```

  In `index.ts`, add in alphabetical position:
  `export { jupiterDailyCapSwap } from './jupiter-daily-cap-swap.js';`. In
  `protocol-examples.test.ts`, raise `expect(entries.length).toBe(N)` by one.

- [ ] **Step 3: A semantics test.** In `protocol-semantics.test.ts`, import `jupiterDailyCapSwap`
  from `../examples/protocols/index.js` (beside the other examples) and add:

```ts
test('jupiterDailyCapSwap charges the inAmount it forwards to Jupiter', () => {
  const swap = jupiterDailyCapSwap.steps.find((item) => item.kind === 'invoke');
  expect(swap?.kind === 'invoke' && swap.data[2]).toEqual(data.encode('u64', expression.input('inAmount')));
  const total = jupiterDailyCapSwap.steps.find((item) => item.kind === 'let' && item.name === 'rateLimitTotal');
  expect(JSON.stringify(total)).toContain('"inAmount"');
  expect(jupiterDailyCapSwap.steps.some((item) => item.kind === 'require' && item.label === 'withinRateLimit')).toBe(true);
});
```

  (import `data` and `expression` from `./index.js` if the file does not already.)

- [ ] **Step 4: The fixture.**

```bash
pnpm fixtures
git status --short fixtures
pnpm --dir clients/js exec vitest run src/protocol-examples.test.ts src/protocol-semantics.test.ts src/fixtures.test.ts
```

Expected: only `fixtures/protocol-examples.json` changes, gaining `jupiterDailyCapSwap`; the
tests pass.

- [ ] **Step 5: The Rust mirror.** In `clients/rust/examples/protocol_templates.rs`, add before the
  `TEMPLATES` array, and add `("jupiterDailyCapSwap", jupiter_daily_cap_swap),` to `TEMPLATES` in
  name order, raising its length by one:

```rust
// #region jupiter-daily-cap
/// A per-caller daily cap on a Jupiter swap: the route's `inAmount` is charged against 1.728 SOL
/// that refills at 20,000 lamports a second, in a registry entry keyed by the actor.
pub fn jupiter_daily_cap_swap() -> Vec<u8> {
    let mut b = ProgramBuilder::new();
    let action_program = program(&mut b, JUPITER_V6);
    let token_program = program(&mut b, TOKEN_PROGRAM_ID);
    let actor = b.account(SIGN | WRITE, None, None, 0);
    let spend = b.account(WRITE, None, None, 0);
    let system_program = program(&mut b, SYSTEM_PROGRAM_ID);
    b.account_groups(1); // actionAccounts
    let route_plan = b.input(VALUE_BYTES, 512);
    let in_amount = b.input(VALUE_U64, 0);
    let quoted_out_amount = b.input(VALUE_U64, 0);
    let slippage_bps = b.input(VALUE_U64, 0);
    let platform_fee_bps = b.input(VALUE_U64, 0);

    // The TypeScript compiler loads the inputs, then the constants in the order the steps use
    // them, then opens the entry, all before the first step.
    let route_plan = b.load_input(route_plan);
    let in_amount = b.load_input(in_amount);
    let quoted_out_amount = b.load_input(quoted_out_amount);
    let slippage_bps = b.load_input(slippage_bps);
    let platform_fee_bps = b.load_input(platform_fee_bps);
    let refill_per_second = b.const_u64(20_000);
    let cap = b.const_u64(1_728_000_000);
    let key = b.account_key(actor);
    b.open_registry(spend, Some(key), actor, 0, 16, system_program);

    // rateLimit: `now` never reads earlier than `lastSpend`, so it, and the `lastSpend` written
    // back (being `now`), never move backward — a clock step-back refills nothing and never
    // double-refills once the clock recovers. The refill itself is computed in u128, then charged
    // against the cap.
    let last = b.read_registry(spend, 8, OP_READ_I64);
    let now = b.clock_timestamp();
    let now = b.binary(OP_MAX, now, last);
    let spent = b.read_registry(spend, 0, OP_READ_U64);
    let spent = b.cast(OP_CAST_U128, spent);
    let elapsed = b.binary(OP_SUB, now, last);
    let elapsed = b.cast(OP_CAST_U128, elapsed);
    let rate = b.cast(OP_CAST_U128, refill_per_second);
    let refill = b.binary(OP_MUL, elapsed, rate);
    let refilled = b.binary(OP_MIN, spent, refill);
    let kept = b.binary(OP_SUB, spent, refilled);
    let amount = b.cast(OP_CAST_U128, in_amount);
    let total = b.binary(OP_ADD, kept, amount);
    let cap = b.cast(OP_CAST_U128, cap);
    let within = b.binary(OP_LTE, total, cap);
    b.require(within);
    let total = b.cast(OP_CAST_U64, total);
    b.write_registry(spend, 0, OP_READ_U64, total);
    b.write_registry(spend, 8, OP_READ_I64, now);

    let route = b.blob(&anchor("route"));
    let swap = b.cpi_with_group(
        action_program,
        &[(token_program, READ), (actor, SIGN)],
        &[
            Segment::Literal(route),
            Segment::Register(DATA_REG_BYTES, route_plan),
            Segment::Register(DATA_REG_U64, in_amount),
            Segment::Register(DATA_REG_U64, quoted_out_amount),
            Segment::Register(DATA_REG_U16, slippage_bps),
            Segment::Register(DATA_REG_U8, platform_fee_bps),
        ],
        0,
    );
    b.set_cpi_max_data_len(swap, 8 + 512 + 8 + 8 + 2 + 1);
    b.invoke(swap, None);
    b.build().unwrap()
}
// #endregion jupiter-daily-cap
```

  Run: `cargo test -p ballista-sdk --test protocol_templates` — the byte comparison names the
  first table that differs. The order above follows the TypeScript compiler (inputs used by the
  steps in declaration order, then literals in the order the steps first use them, then the
  registry opens, then the steps, each expression's left operand first); if a table differs, fix
  the mirror's order to match the fixture, never the template.

- [ ] **Step 6: The Rust run.** In `clients/rust/examples/protocol_templates_run.rs`, add
  `BALLISTA_ID` and `SYSTEM_PROGRAM_ID` to the `ballista_sdk` import if they are not there, add
  after `run_pyth_gate`:

```rust
/// Runs `jupiterDailyCapSwap` for `actor` on `route`, with the actor's entry at its address.
pub fn run_jupiter_daily_cap(
    template: Pubkey,
    actor: Pubkey,
    route: &RouteQuote,
    action_accounts: Vec<AccountMeta>,
) -> Instruction {
    let inputs = RunInputs::new()
        .groups(&[action_accounts.len() as u8]) // actionAccounts
        .bytes(route.route_plan)
        .u64(route.in_amount)
        .u64(route.quoted_out_amount)
        .u64(route.slippage_bps.into())
        .u64(route.platform_fee_bps.into())
        .finish();
    let (spend, _) = Pubkey::find_program_address(
        &[b"registry", template.as_ref(), &[0], actor.as_ref()],
        &BALLISTA_ID,
    );
    let mut accounts = vec![
        pinned(JUPITER_V6),
        pinned(TOKEN_PROGRAM_ID),
        AccountMeta::new(actor, true),
        AccountMeta::new(spend, false),
        pinned(SYSTEM_PROGRAM_ID),
    ];
    accounts.extend(action_accounts);
    run_instruction(template, accounts, &inputs)
}
```

  and to `RUNS`, in name order, raising its length by one:

```rust
    ("jupiterDailyCapSwap", || {
        let data = route_data();
        let route = RouteQuote::split(&data);
        vec![run_jupiter_daily_cap(TEMPLATE, key(1), &route, group(20))]
    }),
```

  Run: `cargo test -p ballista-sdk --test protocol_templates` — Expected: PASS.

- [ ] **Step 7: The protocol test.** Create `tests/protocols/tests/jupiter_daily_cap.rs`:

```rust
//! `jupiterDailyCapSwap` against the real programs: route `solToUsdc`, 1 SOL for USDC through
//! Meteora DLMM, capped per caller at 1.728 SOL that refills at 20,000 lamports a second.
//!
//! The cap passes `route` its token program and signer itself and forwards the rest of the
//! route's accounts as its group. The run takes `route`'s place in Jupiter's own transaction:
//! Jupiter's setup wraps the SOL before it, and its cleanup closes the wrapped SOL account after.

use {
    ballista_protocol_tests::{
        snapshot::{warp, Leg, Snapshot, SNAPSHOT_DIR},
        template::{examples, upload, Example, Run},
        tx::{self, assert_requirement_failed, Failure, Outcome},
        wallet::{self, fund, keypair, SOL},
    },
    ballista_sdk::{BALLISTA_ID, SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID},
    litesvm::LiteSVM,
    solana_address::Address,
    solana_instruction::Instruction,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const TEMPLATE: &str = "jupiterDailyCapSwap";
const ROUTE: &str = "solToUsdc";
const TEMPLATE_ID: u16 = 13;
/// The accounts at the head of `route`'s list that the cap passes itself.
const ROUTE_HEAD: usize = 2;
/// `route`'s arguments end with `in_amount`, `quoted_out_amount`, `slippage_bps` and
/// `platform_fee_bps`.
const ROUTE_TAIL: usize = 8 + 8 + 2 + 1;
/// The template's cap and rate.
const DAILY_CAP: u64 = 1_728_000_000;
const REFILL_PER_SECOND: u64 = 20_000;

struct Cap {
    svm: LiteSVM,
    actor: Keypair,
    template: Address,
    leg: Leg,
    jupiter: Address,
    entry: Address,
}

impl Cap {
    fn new(snapshot: &Snapshot, example: &Example) -> Cap {
        let mut svm = snapshot.svm();
        let actor = wallet::wallet();
        let creator = keypair(b"ballista-protocol-tests-creator1");
        for key in [actor.pubkey(), creator.pubkey()] {
            fund(&mut svm, &key, 10 * SOL);
        }
        let template = upload(&mut svm, &creator, TEMPLATE_ID, &example.payload);
        let leg = snapshot.route(ROUTE).legs[0].clone();
        let head: Vec<Address> = leg.instructions.swap.accounts[..ROUTE_HEAD]
            .iter()
            .map(|meta| meta.pubkey)
            .collect();
        assert_eq!(head, [TOKEN_PROGRAM_ID, actor.pubkey()]);
        let (entry, _) = Address::find_program_address(
            &[b"registry", template.as_ref(), &[0], actor.pubkey().as_ref()],
            &BALLISTA_ID,
        );
        Cap { svm, actor, template, leg, jupiter: snapshot.named("jupiter"), entry }
    }

    /// The route's `in_amount`: what each run sells and is charged.
    fn in_amount(&self) -> u64 {
        let args = &self.leg.route.args;
        let tail = &args[args.len() - ROUTE_TAIL..];
        u64::from_le_bytes(tail[..8].try_into().unwrap())
    }

    /// A run on the route's own terms, its arguments split as the template takes them.
    fn run(&self, example: &Example) -> Instruction {
        let args = &self.leg.route.args;
        let (plan, tail) = args.split_at(args.len() - ROUTE_TAIL);
        let swap = &self.leg.instructions.swap;
        Run::new(self.template, example)
            .account("actionProgram", self.jupiter, false, false)
            .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
            .account("actor", self.actor.pubkey(), true, true)
            .account("spend", self.entry, true, false)
            .account("systemProgram", SYSTEM_PROGRAM_ID, false, false)
            .input_bytes("routePlan", plan)
            .input_u64("inAmount", u64::from_le_bytes(tail[0..8].try_into().unwrap()))
            .input_u64("quotedOutAmount", u64::from_le_bytes(tail[8..16].try_into().unwrap()))
            .input_u64("slippageBps", u16::from_le_bytes([tail[16], tail[17]]).into())
            .input_u64("platformFeeBps", tail[18].into())
            .group("actionAccounts", swap.accounts[ROUTE_HEAD..].to_vec())
            .build()
    }

    /// Sends a run in `route`'s place, between Jupiter's own setup and cleanup.
    fn act(&mut self, example: &Example) -> Result<Outcome, Failure> {
        let instructions = self.leg.instructions.with_swap(self.run(example));
        tx::send(&mut self.svm, &self.actor, &[], &instructions, &self.leg.lookup_tables)
    }

    /// Moves the clock on by `seconds`, and the slot at mainnet's 400 ms a slot (write rule 3).
    fn wait(&mut self, seconds: u64) {
        warp(&mut self.svm, seconds * 5 / 2, seconds);
    }

    /// The entry's `spent` field.
    fn spent(&self) -> u64 {
        let entry = self.svm.get_account(&self.entry).expect("the entry exists");
        u64::from_le_bytes(entry.data[72..80].try_into().unwrap())
    }
}

/// The first swap creates the caller's entry, at the caller's expense, and charges the route's
/// `inAmount`.
#[test]
fn the_first_swap_creates_the_callers_entry() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut cap = Cap::new(&snapshot, example);
    assert_eq!(cap.svm.get_account(&cap.entry), None);

    let outcome = cap.act(example).unwrap_or_else(|failure| panic!("{failure:?}"));
    let entry = cap.svm.get_account(&cap.entry).expect("the run created the entry");
    assert_eq!(entry.owner, BALLISTA_ID);
    assert_eq!(entry.data.len(), 72 + 16);
    assert_eq!(entry.lamports, cap.svm.minimum_balance_for_rent_exemption(72 + 16));
    assert_eq!(&entry.data[..4], b"BREG");
    assert_eq!(cap.spent(), cap.in_amount());
    println!(
        "run: {} CU in the transaction, {} in Ballista's run; {} bytes",
        outcome.compute_units,
        outcome.compute_units_of(&ballista_sdk::ID).unwrap(),
        outcome.size,
    );
}

/// With one sale on the books, a second fails at `withinRateLimit` until the refill covers it:
/// one second short it still fails, and at the exact second it lands with the entry full.
#[test]
fn a_swap_past_the_cap_waits_for_the_refill() {
    let snapshot = Snapshot::load(SNAPSHOT_DIR);
    let examples = examples();
    let example = &examples[TEMPLATE];
    let mut cap = Cap::new(&snapshot, example);
    let sold = cap.in_amount();
    assert!(sold <= DAILY_CAP && 2 * sold > DAILY_CAP, "the route sells {sold}: one fits, two do not");

    cap.act(example).unwrap_or_else(|failure| panic!("{failure:?}"));
    let failure = cap.act(example).unwrap_err();
    assert_requirement_failed(&failure, example, "withinRateLimit");

    // The second sale fits once `2 × sold − refill <= DAILY_CAP`.
    let needed = (2 * sold - DAILY_CAP).div_ceil(REFILL_PER_SECOND);
    cap.wait(needed - 1);
    let failure = cap.act(example).unwrap_err();
    assert_requirement_failed(&failure, example, "withinRateLimit");
    cap.wait(1);
    cap.act(example).unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(cap.spent(), sold - needed * REFILL_PER_SECOND + sold);
}
```

  Two things to check against the harness as it is on the branch: `Run::account` takes
  `(name, address, writable, signer)`, as `pyth_fresh_price_gate.rs` uses it; and
  `svm.minimum_balance_for_rent_exemption` is LiteSVM's rent helper (if the name differs, compute
  the rent from `svm.get_sysvar::<solana_rent::Rent>().minimum_balance(88)`).

- [ ] **Step 8: Run it.**

```bash
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
cargo test --manifest-path tests/protocols/Cargo.toml --test jupiter_daily_cap -- --nocapture
cargo test --manifest-path tests/protocols/Cargo.toml
```

Expected: PASS. With the snapshot's 1 SOL route, `needed` is 13,600 seconds. If the second
swap fails inside Jupiter rather than at `withinRateLimit` after the wait, the first sale moved
the pool past the route's slippage: report it rather than widening anything.

- [ ] **Step 9: Commit.**

```bash
git add clients/js/examples/protocols/jupiter-daily-cap-swap.ts clients/js/examples/protocols/index.ts \
  clients/js/src/protocol-examples.test.ts clients/js/src/protocol-semantics.test.ts fixtures/protocol-examples.json \
  clients/rust/examples/protocol_templates.rs clients/rust/examples/protocol_templates_run.rs \
  tests/protocols/tests/jupiter_daily_cap.rs
git commit -m "$(cat <<'MSG'
Cap a caller's Jupiter swaps per day with a registry entry, against the real programs

jupiterDailyCapSwap charges each route's inAmount against 1.728 SOL a caller that refills at
20,000 lamports a second. On the snapshot's 1 SOL route, the first swap creates the caller's
entry and lands; the second fails at withinRateLimit until the clock has moved 13,600 seconds,
one second short still failing.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
MSG
)"
```
