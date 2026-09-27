# Runtime introspection: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to carry out this plan task by task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add eleven opcodes to the Ballista VM:
- Nine read the transaction through the Instructions sysvar (`INSTRUCTION_COUNT` to
  `READ_INSTRUCTION_BYTES`).
- `READ_ACCOUNT_BYTES` borrows a byte range of a read-only account.
- `BYTES_LEN` measures a `bytes` value.

Each opcode is verified at finalize, executed on chain, authorable from TypeScript and from the
Rust `ProgramBuilder`, and covered end to end under Mollusk. Then add the SDK's
`ed25519Signature` helper and a signed-quote settlement example that uses it.

**Architecture:**
- Opcodes 64 to 74, fixed by the spec:
  `docs/superpowers/specs/2026-09-26-runtime-extensions-design.md`, section 4 and "Opcode
  numbers". The run path never re-verifies, so every rule lives in a new opcode, never in a field
  the current verifier ignores.
- **`RuntimeValue` gains no variant.** A `bytes` result borrows memory that cannot change while the
  instruction runs: the Instructions sysvar's data, or a read-only account's data.
  - Both borrows use `AccountView::borrow_unchecked` (solana-account-view 2.0.0) on the
    `&'data AccountView`, which yields `&'data [u8]` and holds no borrow flag, so a later CPI's
    borrow check still passes.
  - The runtime checks the sysvar's address and the account's `!is_writable()` right before each
    borrow. The SAFETY comments carry the rest of the argument.
- **Parsing uses `pinocchio::sysvars::instructions` (0.11.2).**
  - `Instructions::try_from` takes a `Ref`, not `'data`, so the executor builds
    `Instructions::new_unchecked` over the unchecked borrow instead.
  - pinocchio checks the instruction index and the account position; its errors become
    `InstructionOutOfRange`.
  - Every data range is checked in Ballista.
  - pinocchio ties the slices it returns to a short borrow of its parser, so
    `READ_INSTRUCTION_BYTES` re-slices the sysvar itself, bounds-checked, to get `'data`.
- **Dispatch follows phase 1's Task 9 and its review.**
  - The outer `execute_instruction` match is frozen: no arms, guards or `if`s.
  - All eleven opcodes reach `extended_instruction` through its fallback, and it stays a thin
    router. The count, the index and `BYTES_LEN` run in its inner match.
  - The sysvar parsing and both byte reads run in two `#[inline(never)]` sub-helpers. Each takes
    four words, all passed in registers.
  - A prototype of this plan measured the alternatives on `7a5e15e`'s layout; they are recorded in
    Task 11.
- **The verifier** requires every introspection opcode's `a` to be a fixed account pinned to
  `Sysvar1nstructions1111111111111111111111111`; anything else is `InvalidIntrospection`.
- **The SDK helper** needs no new opcode. It returns requirement steps that bind the precompile's
  verified signature to a signer and a message length, and a `field` reader for the signed
  message.

**Decisions this plan makes where the spec is silent:**
- **Byte ranges.** An account byte range past the data fails with `InstructionOutOfRange`, as the
  spec's "an index, position or byte range outside what exists" says. The alternative,
  `InvalidRuntimeAccount` as the older dynamic reads use, is a structural error in the Certora
  typing rule. `InstructionOutOfRange` is value-dependent, so it belongs in `value_dependent`
  (Task 10).
- **Verifier errors.** A bad `READ_INSTRUCTION_DATA` selector, or a byte-read length outside 1 to
  1,024, is `InvalidInstruction(pc)`. `InvalidIntrospection(pc)` is kept for the sysvar pin, as the
  spec defines it: unpinned, pinned elsewhere, a row account, or an undeclared one.
- **The helper's header check.** The signature count, the three instruction indexes and the
  message size all sit in the Ed25519 instruction's first 16 bytes. The helper checks them with
  one masked `u128` comparison: five comparisons cost the signed-quote example 60 of its 64
  registers, the masked one 50.
  - It binds `<name>Instruction` and `<name>Message` once, so the index expression and the message
    offset are evaluated once.
  - It takes an optional `name` so a template can check two signatures.
  - It refuses at build time any `field` that reaches past the signed message.
- **The quote also signs the two mints.** Without them, a quote for one market would settle in
  another. The maker co-signs the transaction, because Ballista never controls the maker's tokens
  (see `docs/examples/payments.md`, "Authorization"). The quote is what lets that co-signer sign
  without checking the terms. Ballista keeps no state, so the example documents that the co-signer
  refuses a second settlement of one quote.
- **Precompile timing.** Agave runs the Ed25519 precompile in instruction order, and Mollusk's
  `precompiles` feature does too. The example requires the signature in the instruction
  immediately before its run, so the signature is verified before Ballista runs. Whatever the
  order, a failed signature fails the whole transaction.
- **Mollusk's precompile inputs.** Mollusk passes the precompile only its own data as
  `instruction_datas`. That is enough here, because the helper requires every offset to name
  `u16::MAX`, the precompile instruction itself.
- **TypeScript extras.**
  - `readWidth` moves from `compiler.ts` to `schema.ts`, so `helpers.ts` can bound `field` without
    importing the compiler (which imports `helpers.ts`).
  - `expression.instructionAccountFlags` is exported beside the spec's two sugar functions.
  - The compiler refuses `accountDataBytes` on an account declared `writable`, a run that could
    never succeed.
- **The program generator is unchanged.** `generate.rs` builds no `bytes` registers and no data
  reads, and introspection needs a transaction-built sysvar that the generated-program harness
  does not supply. The spec asks the generator to cover only math and loops.

**Tech stack:**
- Rust (no_std SBF program on pinocchio 0.11.2, shared `ballista-common` crate).
- TypeScript SDK (Zod, vitest).
- Mollusk 0.14 integration suite, with its `precompiles` feature; Certora specs.

**Base and order:**
- **Branch.** `claude/runtime-introspection`, cut from `claude/runtime-extensions` once phase 1
  has finished. That means Task 10's oracle example (`f0524c2`, `dedc168`), Task 11's
  verification, and the `extended_instruction(machine, instruction, loop_context)` shape the Task 9
  review adopted.
- **Line numbers** refer to `dedc168`. `execute.rs` is named by function, because that refactor
  moves it.
- **Depends on phases 2 and 3, which merge first.**
  - Loops (phase 2) assign runtime code 6022 (`LoopCountExceeded`) and verifier code 6129
    (`InvalidLoop`). Output (phase 3) assigns 6130 (`InvalidOutput`).
  - This branch is built in parallel, then rebased onto them before it merges (Task 13).
  - Until then Task 1 reserves those three codes, so this branch uses its final numbers
    throughout: runtime 6023 `InstructionOutOfRange` and 6024 `WritableAccountBytesRead`, verifier
    6131 `InvalidIntrospection`. The first unused codes are 6025 and 6132.
  - If phases 2 and 3 have already merged when you start, skip Task 1 and Task 13.
- All paths are relative to the worktree root.

**Conventions:**
- Keep the surrounding style:
  - doc comments that explain why;
  - `#[inline(never)]` for heavy helpers reached from the dispatch loop;
  - `RunResult` and `BallistaError` for failures.
- Never use `git stash`. To set work aside, commit it.
- End every commit message with your own `Co-Authored-By:` line.
- Measure before raising any compute-unit ceiling, and raise it by hand to the exact figure.

---

## File map

| File | Change |
| --- | --- |
| `common/src/template/wire.rs` | `OP_INSTRUCTION_COUNT` … `OP_BYTES_LEN` (64–74); `INSTRUCTIONS_SYSVAR_ID`; error names; `InvalidIntrospection` (and Task 1's reserved variants) |
| `common/src/template/verify.rs` | Rules for the eleven opcodes, `require_instructions_sysvar`, `byte_read_len`; tests; fixture list |
| `common/src/template/builder.rs` | `introspect`, `read_instruction_data`, `read_instruction_bytes`, `read_account_bytes`, `bytes_len` |
| `programs/ballista/src/error.rs` | `InstructionOutOfRange`, `WritableAccountBytesRead` (and Task 1's `LoopCountExceeded`) |
| `programs/ballista/src/processor/introspect.rs` | **New.** Sysvar parsing, byte reads, the two borrows and their SAFETY arguments, host tests |
| `programs/ballista/src/processor/mod.rs` | Declare `introspect` (public under `spec-api`, like `math`) |
| `programs/ballista/src/processor/execute.rs` | Router arms in `extended_instruction`; the unrun-opcode test list |
| `programs/ballista/src/processor/math.rs` | Only if Task 11's disassembly check calls for it: `#[inline(always)]` on `remainder` |
| `clients/rust/src/lib.rs` | `INSTRUCTIONS_SYSVAR_ID`, `ED25519_PROGRAM_ID`; decode tests |
| `clients/js/src/errors.ts`, `errors.test.ts` | Error names; decode tests |
| `clients/js/src/schema.ts` | `readWidth` (moved here); eight expression kinds; their schemas and constructors |
| `clients/js/src/compiler.ts` | Opcode table; lowering; `encodeSysvar` |
| `clients/js/src/helpers.ts` | `INSTRUCTIONS_SYSVAR_ADDRESS_BYTES`, `ED25519_PROGRAM_ADDRESS_BYTES`, `ed25519Signature` |
| `clients/js/src/opcodes.test.ts` | Parity entries for 64–74 |
| `clients/js/src/compiler.test.ts` | Introspection and helper tests |
| `clients/js/src/fixtures.test.ts` | `introspection` and `signed-quote-settlement` fixtures |
| `clients/js/examples/protocols/signed-quote-settlement.ts` | **New.** The example |
| `clients/js/examples/protocols/index.ts`, `shared.ts` | Export; `TOKEN_ACCOUNT_OWNER_OFFSET` |
| `clients/js/src/protocol-examples.test.ts` | Count 12 → 13 |
| `clients/js/src/protocol-semantics.test.ts` | `dependsOn` cases; signed-quote tests |
| `tests/ballista/Cargo.toml`, `Cargo.lock` | Mollusk `precompiles`; `ed25519-dalek`, `solana-ed25519-program` |
| `tests/ballista/src/lib.rs` | Transaction helpers; introspection and signed-quote tests |
| `tests/ballista/src/cases.rs` | `run, introspection, no cpi` case |
| `certora/ballista-specs/src/rules/errors.rs`, `typing.rs` | Error-code rules; `value_dependent` |
| `fixtures/*` | Regenerated: error names, `introspection.hex`, `signed-quote-settlement.hex`, manifest, protocol examples; `cu-ceilings.json` by hand |
| `benches/CHANGELOG.md` | The entry the ceiling raise needs |

---

### Task 0: Worktree and baseline

- [ ] **Step 1: Create the worktree.** Use superpowers:using-git-worktrees. Create branch
  `claude/runtime-introspection` from the tip of `claude/runtime-extensions`, at
  `.claude/worktrees/runtime-introspection`.
- [ ] **Step 2: Check the base.**

```bash
cd .claude/worktrees/runtime-introspection
git log --oneline -3
grep -n "case 'multiplyDivide'" clients/js/src/protocol-semantics.test.ts
grep -n "TOKEN_ACCOUNT_MINT_OFFSET = 0" clients/js/examples/protocols/shared.ts
grep -n -A6 "fn extended_instruction" programs/ballista/src/processor/execute.rs
grep -n "LoopCountExceeded\|InvalidLoop\|InvalidOutput" common/src/template/wire.rs
grep -n "pub const OP_" common/src/template/wire.rs | tail -4
```

Expected:
- The first two greps each print one line: phase 1's Task 10 is in. If not, stop; this plan
  builds on it.
- The third shows `extended_instruction`'s parameters. Note whether it takes `machine: &mut
  Machine` (the shape Task 4 is written for) or separate `program, accounts, registers` (the
  `7a5e15e` shape Task 4 also covers).
- The fourth prints nothing unless phases 2 and 3 have merged. If it prints all three names, skip
  Task 1.
- The last shows no opcode above 63.

- [ ] **Step 3: Install and build.**

```bash
pnpm install --frozen-lockfile
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
```

- [ ] **Step 4: Baseline, all green before any change.**

```bash
pnpm fixtures && git diff --exit-code fixtures
pnpm check
pnpm test
cargo test --manifest-path tests/ballista/Cargo.toml
cargo check -p ballista-specs --features rt --manifest-path certora/Cargo.toml
cargo clippy -p ballista -p ballista-common --all-targets 2>&1 | grep -c -E "^(warning|error)"
```

Expected: every command exits 0. Record the clippy count; Task 12 compares against it. If
anything fails on the untouched base, stop and report it rather than fixing it silently.

- [ ] **Step 5: Commit this plan.**

```bash
mkdir -p docs/superpowers/plans
cp /private/tmp/claude-501/-Users-jacob-Documents-ballista/06470e69-8486-4adc-b478-7e3c20bf99aa/scratchpad/plans/2026-09-27-runtime-introspection.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-09-27-runtime-introspection.md
git commit -m "Plan the introspection and byte opcodes and the Ed25519 helper"
```

---

### Task 1: Reserve the codes phases 2 and 3 assign

Skip this task if Task 0 found `LoopCountExceeded`, `InvalidLoop` and `InvalidOutput` already in
`wire.rs`.

Error codes are positions in name tables, so 6023 exists only if 6022 does. This commit adds the
three earlier phases' names exactly as the spec gives them, and nothing that raises them. Task 13
drops it when this branch is rebased onto those phases.

**Files:**
- `common/src/template/wire.rs`: `RUNTIME_ERROR_NAMES` (50–73), `TemplateError` (612–649),
  `TemplateError::code` (656–691), `VERIFIER_ERROR_NAMES` (695–725).
- `programs/ballista/src/error.rs`: the enum and the test list.
- `clients/js/src/errors.ts`, `clients/js/src/errors.test.ts`.
- `clients/rust/src/lib.rs`: `error_codes_decode_with_context`.
- `certora/ballista-specs/src/rules/errors.rs`.
- `fixtures/runtime-error-names.txt`, `fixtures/verifier-error-names.txt` (regenerated).

- [ ] **Step 1: `wire.rs`.** Change `pub const RUNTIME_ERROR_NAMES: [&str; 22] = [` to
  `[&str; 23]` and add `"LoopCountExceeded",` after `"CpiAccountLimitExceeded",`. After the
  `TooManyAccountGroups,` variant of `TemplateError`, add:

```rust
    /// Reserved for count loops (runtime extensions, phase 2): a count loop's shape, the loop
    /// limit, or rows inside a count loop.
    InvalidLoop(usize),
    /// Reserved for output (runtime extensions, phase 3): output length, or where and how often
    /// `SET_RETURN_DATA` appears.
    InvalidOutput(usize),
```

  In `TemplateError::code`, after `TemplateError::TooManyAccountGroups => (28, 0),` add:

```rust
            TemplateError::InvalidLoop(index) => (29, clamp(index)),
            TemplateError::InvalidOutput(index) => (30, clamp(index)),
```

  Change `pub const VERIFIER_ERROR_NAMES: [&str; 29] = [` to `[&str; 31]` and add
  `"InvalidLoop",` and `"InvalidOutput",` after `"TooManyAccountGroups",`.

- [ ] **Step 2: `error.rs`.** After the `CpiAccountLimitExceeded,` variant add:

```rust
    /// Reserved for count loops (runtime extensions, phase 2): a count above the loop's static
    /// maximum. Assigned here so the codes after it keep their numbers until that phase merges.
    #[error("loop count exceeds its maximum")]
    LoopCountExceeded,
```

  In `runtime_error_names_match_the_shared_fixture_in_code_order`, add
  `BallistaError::LoopCountExceeded,` after `BallistaError::CpiAccountLimitExceeded,`.

- [ ] **Step 3: `errors.ts`.** Add `'LoopCountExceeded',` after `'CpiAccountLimitExceeded',`,
  and `'InvalidLoop',` and `'InvalidOutput',` after `'TooManyAccountGroups',`.

- [ ] **Step 4: The first unused codes move up.** In `clients/rust/src/lib.rs`, replace

```rust
        assert!(decode_ballista_error(6022).is_none());
        assert!(decode_ballista_error(6129).is_none());
```

  with

```rust
        assert!(decode_ballista_error(6023).is_none());
        assert!(decode_ballista_error(6131).is_none());
```

  In `clients/js/src/errors.test.ts`, change `decodeBallistaError(6022)` to
  `decodeBallistaError(6023)` and `decodeBallistaError(6129)` to `decodeBallistaError(6131)`.

- [ ] **Step 5: Certora.** In `rule_verifier_error_codes_are_distinct_and_in_range`, replace
  `_ => TemplateError::TooManyAccountGroups,` with:

```rust
        28 => TemplateError::TooManyAccountGroups,
        29 => TemplateError::InvalidLoop(nondet()),
        _ => TemplateError::InvalidOutput(nondet()),
```

- [ ] **Step 6: Regenerate the name fixtures and run the tests.**

```bash
pnpm fixtures
cargo test -p ballista --lib error
cargo test -p ballista-common --lib wire
cargo test -p ballista-sdk error_codes_decode_with_context
pnpm --dir clients/js exec vitest run src/errors.test.ts src/fixtures.test.ts
cargo check -p ballista-specs --features rt --manifest-path certora/Cargo.toml
```

Expected: PASS. `git diff fixtures` shows the three names appended to the two `.txt` files and
nothing else.

- [ ] **Step 7: Commit.** Record its SHA; Task 13 needs it.

```bash
git add common/src/template/wire.rs programs/ballista/src/error.rs clients/js/src/errors.ts \
  clients/js/src/errors.test.ts clients/rust/src/lib.rs certora/ballista-specs/src/rules/errors.rs fixtures
git commit -m "Reserve the error codes the loop and output phases assign

LoopCountExceeded (6022), InvalidLoop (6129) and InvalidOutput (6130) belong to
phases 2 and 3. Reserving them lets this branch use its own final codes. Drop
this commit when rebasing onto those phases."
git rev-parse HEAD
```

---
### Task 2: The new error kinds

**Files:**
- `common/src/template/wire.rs`: the two name tables, `TemplateError`, `TemplateError::code`,
  and `error_codes_are_unique_and_round_trip_context` (800–853).
- `programs/ballista/src/error.rs`: the enum and the test list.
- `clients/js/src/errors.ts`, `clients/js/src/errors.test.ts`.
- `clients/rust/src/lib.rs`: `error_codes_decode_with_context`.
- `certora/ballista-specs/src/rules/errors.rs`.
- `fixtures/*-error-names.txt` (regenerated).

- [ ] **Step 1: Write the failing tests.**
  - `wire.rs`, in `error_codes_are_unique_and_round_trip_context`: add
    `TemplateError::InvalidIntrospection(15),` after `TemplateError::InvalidMinIterations,`, and
    change `assert_eq!(*codes.last().unwrap(), VERIFIER_ERROR_BASE + 27);` to `+ 31`.
  - `error.rs`, in the test's `variants` list: add `BallistaError::InstructionOutOfRange,` and
    `BallistaError::WritableAccountBytesRead,` after `BallistaError::LoopCountExceeded,`.
  - `clients/rust/src/lib.rs`: replace the two `is_none` lines Task 1 left with:

```rust
        assert_eq!(decode_ballista_error(6023).unwrap().name, "InstructionOutOfRange");
        assert_eq!(
            decode_ballista_error((5 << 16) | 6024).unwrap().name,
            "WritableAccountBytesRead"
        );
        assert_eq!(decode_ballista_error(6131).unwrap().name, "InvalidIntrospection");
        assert!(decode_ballista_error(6025).is_none());
        assert!(decode_ballista_error(6132).is_none());
```

  - `errors.test.ts`: replace `expect(decodeBallistaError(6023)).toBeUndefined();` with

```ts
    expect(decodeBallistaError((4 << 16) | 6023)).toMatchObject({ name: 'InstructionOutOfRange', context: 4 });
    expect(decodeBallistaError(6024)).toMatchObject({ name: 'WritableAccountBytesRead', source: 'runtime' });
    expect(decodeBallistaError(6131)).toMatchObject({ name: 'InvalidIntrospection', source: 'verifier' });
    expect(decodeBallistaError(6025)).toBeUndefined();
```

    and `expect(decodeBallistaError(6131)).toBeUndefined();` with
    `expect(decodeBallistaError(6132)).toBeUndefined();`.

- [ ] **Step 2: Run them and confirm they fail.**

Run: `cargo test -p ballista-common --lib wire && cargo test -p ballista --lib error`
Expected: compile errors. `TemplateError::InvalidIntrospection`,
`BallistaError::InstructionOutOfRange` and `BallistaError::WritableAccountBytesRead` are not
defined.

- [ ] **Step 3: `wire.rs`.**
  - Change `RUNTIME_ERROR_NAMES: [&str; 23]` to `[&str; 25]` and add
    `"InstructionOutOfRange",` and `"WritableAccountBytesRead",` after `"LoopCountExceeded",`.
  - After the `InvalidOutput(usize),` variant add:

```rust
    /// An introspection opcode's account is not a fixed account pinned to the Instructions sysvar.
    InvalidIntrospection(usize),
```

  - In `TemplateError::code`, after the `InvalidOutput` arm add
    `TemplateError::InvalidIntrospection(index) => (31, clamp(index)),`.
  - Change `VERIFIER_ERROR_NAMES: [&str; 31]` to `[&str; 32]` and add `"InvalidIntrospection",`
    after `"InvalidOutput",`.

- [ ] **Step 4: `error.rs`.** After the `LoopCountExceeded,` variant add:

```rust
    /// An introspection index or position, or a byte range read from instruction or account
    /// data, is outside what exists. The context is the program counter.
    #[error("instruction, account position or byte range out of range")]
    InstructionOutOfRange,
    /// `READ_ACCOUNT_BYTES` named an account the transaction can write. Its bytes could change
    /// under a CPI, so they are not lent to a register. The context is the program counter.
    #[error("bytes were read from a writable account")]
    WritableAccountBytesRead,
```

- [ ] **Step 5: `errors.ts`.** Add `'InstructionOutOfRange',` and `'WritableAccountBytesRead',`
  after `'LoopCountExceeded',`, and `'InvalidIntrospection',` after `'InvalidOutput',`.
  `explainRunError` needs nothing: both runtime kinds carry a program counter, which its default
  branch maps to the step.

- [ ] **Step 6: Certora `errors.rs`.**
  - In `rule_runtime_error_codes_carry_context_and_stay_in_range`, add
    `BallistaError::InstructionOutOfRange,` and `BallistaError::WritableAccountBytesRead,` to the
    end of the `pick!` list.
  - In `rule_verifier_error_codes_are_distinct_and_in_range`, replace
    `_ => TemplateError::InvalidOutput(nondet()),` with:

```rust
        30 => TemplateError::InvalidOutput(nondet()),
        _ => TemplateError::InvalidIntrospection(nondet()),
```

- [ ] **Step 7: Regenerate the name fixtures and run the tests.**

```bash
pnpm fixtures
cargo test -p ballista-common --lib wire
cargo test -p ballista --lib error
cargo test -p ballista-sdk error_codes_decode_with_context
pnpm --dir clients/js exec vitest run src/errors.test.ts src/fixtures.test.ts
cargo check -p ballista-specs --features rt --manifest-path certora/Cargo.toml
```

Expected: PASS. `git diff fixtures` shows two names appended to `runtime-error-names.txt` and one
to `verifier-error-names.txt`.

- [ ] **Step 8: Commit.**

```bash
git add common/src/template/wire.rs programs/ballista/src/error.rs clients/js/src/errors.ts \
  clients/js/src/errors.test.ts clients/rust/src/lib.rs certora/ballista-specs/src/rules/errors.rs fixtures
git commit -m "Name the introspection errors: InstructionOutOfRange, WritableAccountBytesRead, InvalidIntrospection"
```

---

### Task 3: Opcodes, the sysvar address and the verifier

**Files:**
- `common/src/template/wire.rs`: after `MAX_RETURN_DATA_LEN` (28), and after `OP_READ_I32`
  (159).
- `common/src/template/builder.rs`: before `loop_index` (250).
- `common/src/template/verify.rs`:
  - `verify_instruction`, after the `OP_POW10` arm (411–414);
  - after `require_account` (763–767) and before `valid_range` (813);
  - tests.
- `clients/js/src/compiler.ts`: the `opcode` table (48–108).
- `clients/js/src/opcodes.test.ts`: `rustName`.
- `clients/rust/src/lib.rs`: constants and a test.

- [ ] **Step 1: Write the failing verifier tests.** In `verify.rs`, add these cases to the
  `cases` array in `every_opcode_rejects_uninitialized_or_mistyped_operands`, before the
  `(39, …)` line. Opcodes 64 to 74 are taken now, so the unknown-opcode probe moves to 75; 39 and
  `0xfe` stay.

```rust
            (OP_BYTES_LEN, Some(VALUE_BYTES), None, Ok(())),
            (OP_BYTES_LEN, Some(VALUE_U64), None, Err(TemplateError::TypeMismatch)),
            (OP_BYTES_LEN, Some(VALUE_PUBKEY), None, Err(TemplateError::TypeMismatch)),
            (OP_BYTES_LEN, None, None, Err(TemplateError::RegisterNotInitialized(0))),
            (OP_BYTES_LEN + 1, Some(VALUE_U64), None, Err(TemplateError::InvalidInstruction(1))),
```

Then add these helpers and tests before `fn row_inputs_and_account_groups_are_verified`:

```rust
    /// A builder whose fixed account 0 is pinned to the Instructions sysvar.
    fn with_sysvar() -> (ProgramBuilder, u8) {
        let mut builder = ProgramBuilder::new();
        let sysvar = builder.account(0, Some(INSTRUCTIONS_SYSVAR_ID), None, 0);
        (builder, sysvar)
    }

    /// A valid use of `opcode`, one of `OP_INSTRUCTION_COUNT` to `OP_BYTES_LEN`, as the last
    /// instruction of its program. Returns the builder and the result register.
    fn introspection_program(opcode: u8) -> (ProgramBuilder, u8) {
        let (mut builder, sysvar) = with_sysvar();
        let zero = builder.const_u64(0);
        let result = match opcode {
            OP_READ_INSTRUCTION_DATA => {
                builder.read_instruction_data(OP_READ_U64, sysvar, zero, zero)
            }
            OP_READ_INSTRUCTION_BYTES => builder.read_instruction_bytes(sysvar, zero, zero, 8),
            OP_READ_ACCOUNT_BYTES => builder.read_account_bytes(sysvar, zero, 8),
            OP_BYTES_LEN => {
                let bytes = builder.const_bytes(&[1, 2]);
                builder.bytes_len(bytes)
            }
            _ => builder.introspect(opcode, sysvar, zero, zero),
        };
        (builder, result)
    }

    #[test]
    fn introspection_and_byte_opcodes_type_their_results() {
        let cases = [
            (OP_INSTRUCTION_COUNT, VALUE_U64),
            (OP_INSTRUCTION_INDEX, VALUE_U64),
            (OP_INSTRUCTION_PROGRAM, VALUE_PUBKEY),
            (OP_INSTRUCTION_ACCOUNT_COUNT, VALUE_U64),
            (OP_INSTRUCTION_ACCOUNT, VALUE_PUBKEY),
            (OP_INSTRUCTION_ACCOUNT_FLAGS, VALUE_U64),
            (OP_INSTRUCTION_DATA_LEN, VALUE_U64),
            (OP_READ_INSTRUCTION_DATA, VALUE_U64),
            (OP_READ_INSTRUCTION_BYTES, VALUE_BYTES),
            (OP_READ_ACCOUNT_BYTES, VALUE_BYTES),
            (OP_BYTES_LEN, VALUE_U64),
        ];
        for (opcode, expected) in cases {
            for witness in [VALUE_U64, VALUE_PUBKEY, VALUE_BYTES] {
                let (mut builder, result) = introspection_program(opcode);
                let other = typed_register(&mut builder, Some(witness));
                builder.binary(OP_EQ, result, other);
                let outcome = verify_builder(&builder).map(|_| ());
                if witness == expected {
                    assert_eq!(outcome, Ok(()), "opcode {opcode}");
                } else {
                    assert_eq!(
                        outcome,
                        Err(TemplateError::TypeMismatch),
                        "opcode {opcode} vs {witness}"
                    );
                }
            }
        }
    }

    #[test]
    fn introspection_needs_a_fixed_account_pinned_to_the_instructions_sysvar() {
        for opcode in OP_INSTRUCTION_COUNT..=OP_READ_INSTRUCTION_BYTES {
            let immediate = match opcode {
                OP_READ_INSTRUCTION_DATA => OP_READ_U64 as u64,
                OP_READ_INSTRUCTION_BYTES => 8,
                _ => 0,
            };
            // Unpinned but owned by the sysvar's address, and pinned to another address.
            for address in [None, Some([7; 32])] {
                let mut builder = ProgramBuilder::new();
                let account = builder.account(0, address, Some(INSTRUCTIONS_SYSVAR_ID), 0);
                let zero = builder.const_u64(0);
                builder.op(opcode, account, zero, zero, immediate);
                assert_eq!(
                    verify_builder(&builder),
                    Err(TemplateError::InvalidIntrospection(1)),
                    "opcode {opcode} with address {address:?}"
                );
            }
            // An account the schema does not declare.
            let (mut builder, _) = with_sysvar();
            let zero = builder.const_u64(0);
            builder.op(opcode, 1, zero, zero, immediate);
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::InvalidIntrospection(1)),
                "opcode {opcode} with an undeclared account"
            );
            // A row account pinned to the sysvar is still not a fixed account.
            let mut builder = ProgramBuilder::new();
            let row = builder.row_account(0, Some(INSTRUCTIONS_SYSVAR_ID), None, 0);
            builder.batch(1, 0);
            let zero = builder.const_u64(0);
            builder.for_each(0, |body| {
                body.op(opcode, row, zero, zero, immediate);
            });
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::InvalidIntrospection(2)),
                "opcode {opcode} on a row account"
            );
            // The pinned fixed account works inside a loop body too.
            let mut builder = ProgramBuilder::new();
            let sysvar = builder.account(0, Some(INSTRUCTIONS_SYSVAR_ID), None, 0);
            builder.row_account(0, None, None, 0);
            builder.batch(1, 0);
            let zero = builder.const_u64(0);
            builder.for_each(0, |body| {
                body.op(opcode, sysvar, zero, zero, immediate);
            });
            assert_eq!(verify_builder(&builder).map(|_| ()), Ok(()), "opcode {opcode} in a loop");
        }
    }

    #[test]
    fn introspection_indexes_positions_and_offsets_are_u64_registers() {
        let indexed = [
            (OP_INSTRUCTION_PROGRAM, 0),
            (OP_INSTRUCTION_ACCOUNT_COUNT, 0),
            (OP_INSTRUCTION_ACCOUNT, 0),
            (OP_INSTRUCTION_ACCOUNT_FLAGS, 0),
            (OP_INSTRUCTION_DATA_LEN, 0),
            (OP_READ_INSTRUCTION_DATA, OP_READ_U8 as u64),
            (OP_READ_INSTRUCTION_BYTES, 1),
        ];
        for (opcode, immediate) in indexed {
            let (mut builder, sysvar) = with_sysvar();
            let index = builder.const_i64(0);
            let zero = builder.const_u64(0);
            builder.op(opcode, sysvar, index, zero, immediate);
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::TypeMismatch),
                "opcode {opcode} index"
            );

            let (mut builder, sysvar) = with_sysvar();
            let index = builder.register();
            let zero = builder.const_u64(0);
            builder.op(opcode, sysvar, index, zero, immediate);
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::RegisterNotInitialized(index)),
                "opcode {opcode} unset index"
            );
        }
        // The account opcodes take a position in `c`, the data reads an offset.
        let positioned = [
            (OP_INSTRUCTION_ACCOUNT, 0),
            (OP_INSTRUCTION_ACCOUNT_FLAGS, 0),
            (OP_READ_INSTRUCTION_DATA, OP_READ_U8 as u64),
            (OP_READ_INSTRUCTION_BYTES, 1),
        ];
        for (opcode, immediate) in positioned {
            let (mut builder, sysvar) = with_sysvar();
            let zero = builder.const_u64(0);
            let position = builder.const_u128(0);
            builder.op(opcode, sysvar, zero, position, immediate);
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::TypeMismatch),
                "opcode {opcode} c"
            );
        }
    }

    #[test]
    fn instruction_data_reads_take_a_read_opcode_as_their_width() {
        let selectors = [
            (OP_READ_U8, VALUE_U64),
            (OP_READ_U16, VALUE_U64),
            (OP_READ_I32, VALUE_I64),
            (OP_READ_I64, VALUE_I64),
            (OP_READ_BOOL, VALUE_BOOL),
            (OP_READ_U128, VALUE_U128),
            (OP_READ_PUBKEY, VALUE_PUBKEY),
        ];
        for (selector, witness) in selectors {
            let (mut builder, sysvar) = with_sysvar();
            let zero = builder.const_u64(0);
            let value = builder.read_instruction_data(selector, sysvar, zero, zero);
            let other = typed_register(&mut builder, Some(witness));
            builder.binary(OP_EQ, value, other);
            assert_eq!(verify_builder(&builder).map(|_| ()), Ok(()), "selector {selector}");
        }
        // Anything but a read opcode is refused, including a read opcode above the low byte.
        let immediates = [
            0,
            OP_ADD as u64,
            OP_READ_INSTRUCTION_DATA as u64,
            0x100 | OP_READ_U64 as u64,
            u64::MAX,
        ];
        for immediate in immediates {
            let (mut builder, sysvar) = with_sysvar();
            let zero = builder.const_u64(0);
            builder.op(OP_READ_INSTRUCTION_DATA, sysvar, zero, zero, immediate);
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::InvalidInstruction(1)),
                "immediate {immediate:#x}"
            );
        }
    }

    #[test]
    fn byte_reads_take_one_to_1024_bytes_and_are_typed_that_long() {
        for opcode in [OP_READ_INSTRUCTION_BYTES, OP_READ_ACCOUNT_BYTES] {
            let lengths = [
                (0, Err(TemplateError::InvalidInstruction(1))),
                (1, Ok(())),
                (MAX_INPUT_BYTES as u64, Ok(())),
                (MAX_INPUT_BYTES as u64 + 1, Err(TemplateError::InvalidInstruction(1))),
                (u64::MAX, Err(TemplateError::InvalidInstruction(1))),
            ];
            for (len, expected) in lengths {
                let (mut builder, sysvar) = with_sysvar();
                let zero = builder.const_u64(0);
                builder.op(opcode, sysvar, zero, zero, len);
                assert_eq!(
                    verify_builder(&builder).map(|_| ()),
                    expected,
                    "opcode {opcode} len {len}"
                );
            }
            // A CPI that forwards the bytes must declare exactly their length.
            let (mut builder, sysvar) = with_sysvar();
            let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
            let zero = builder.const_u64(0);
            let bytes = builder.op(opcode, sysvar, zero, zero, 40);
            let cpi = builder.cpi(program, &[], &[Segment::Register(DATA_REG_BYTES, bytes)]);
            builder.set_cpi_max_data_len(cpi, 40);
            builder.invoke(cpi, None);
            assert_eq!(verify_builder(&builder).map(|_| ()), Ok(()), "opcode {opcode}");
            builder.set_cpi_max_data_len(cpi, 41);
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::InvalidCpi(0)),
                "opcode {opcode}"
            );
        }
    }

    #[test]
    fn account_byte_reads_take_any_declared_account_and_a_u64_offset() {
        let mut builder = ProgramBuilder::new();
        let account = builder.account(0, None, None, 0);
        let offset = builder.const_u64(0);
        builder.read_account_bytes(account, offset, 8);
        assert_eq!(verify_builder(&builder).map(|_| ()), Ok(()), "a fixed account, unpinned");

        let mut builder = ProgramBuilder::new();
        let row = builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let offset = builder.const_u64(0);
        builder.for_each(0, |body| {
            body.read_account_bytes(row, offset, 8);
        });
        assert_eq!(verify_builder(&builder).map(|_| ()), Ok(()), "a row account in its loop");

        let mut builder = ProgramBuilder::new();
        let offset = builder.const_u64(0);
        builder.read_account_bytes(3, offset, 8);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidAccountConstraint(3)));

        let mut builder = ProgramBuilder::new();
        let account = builder.account(0, None, None, 0);
        let offset = builder.const_i64(0);
        builder.read_account_bytes(account, offset, 8);
        assert_eq!(verify_builder(&builder), Err(TemplateError::TypeMismatch));
    }

    #[test]
    fn introspection_and_byte_opcodes_reject_flags() {
        // None of them is a read opcode, so the dynamic-offset flag is never theirs to claim.
        for opcode in OP_INSTRUCTION_COUNT..=OP_BYTES_LEN {
            let (mut builder, _) = introspection_program(opcode);
            assert_eq!(verify_builder(&builder).map(|_| ()), Ok(()), "opcode {opcode} unflagged");
            let last = builder.instructions_mut().len() - 1;
            builder.instructions_mut()[last].flags = INSTRUCTION_FLAG_DYNAMIC_OFFSET;
            assert_eq!(
                verify_builder(&builder),
                Err(TemplateError::InvalidFlags(last)),
                "opcode {opcode}"
            );
        }
    }
```

`typed_register`, `verify_builder`, `ProgramBuilder::instructions_mut` and `set_cpi_max_data_len`
already exist. In the "unpinned" case the owner is pinned to the sysvar's address on purpose: an
owner pin does not count.

- [ ] **Step 2: Run the tests and confirm they fail.**

Run: `cargo test -p ballista-common --lib verify`
Expected: compile errors. The `OP_INSTRUCTION_*` constants, `OP_READ_*_BYTES`, `OP_BYTES_LEN`,
`INSTRUCTIONS_SYSVAR_ID` and the builder methods are not defined.

- [ ] **Step 3: The constants.** In `wire.rs`, after `MAX_RETURN_DATA_LEN`:

```rust

/// `Sysvar1nstructions1111111111111111111111111`. Introspection opcodes read the transaction's
/// instructions from this sysvar, which a template declares as a fixed account pinned to it.
pub const INSTRUCTIONS_SYSVAR_ID: [u8; 32] = [
    0x06, 0xa7, 0xd5, 0x17, 0x18, 0x7b, 0xd1, 0x66, 0x35, 0xda, 0xd4, 0x04, 0x55, 0xfd, 0xc2, 0xc0,
    0xc1, 0x24, 0xc6, 0x8f, 0x21, 0x56, 0x75, 0xa5, 0xdb, 0xba, 0xcb, 0x5f, 0x08, 0x00, 0x00, 0x00,
];
```

These are the bytes of pinocchio's `sysvars::instructions::INSTRUCTIONS_ID`; Task 4 tests that.
After `pub const OP_READ_I32: u8 = 60;`:

```rust
/// The number of instructions in the transaction. `a` is the Instructions sysvar account, as for
/// every opcode up to `OP_READ_INSTRUCTION_BYTES`.
pub const OP_INSTRUCTION_COUNT: u8 = 64;
/// The index of the instruction running this template.
pub const OP_INSTRUCTION_INDEX: u8 = 65;
/// The program of the instruction whose `u64` index is in register `b`.
pub const OP_INSTRUCTION_PROGRAM: u8 = 66;
/// How many accounts instruction `b` names.
pub const OP_INSTRUCTION_ACCOUNT_COUNT: u8 = 67;
/// The key of account `c` of instruction `b`, both `u64` registers.
pub const OP_INSTRUCTION_ACCOUNT: u8 = 68;
/// The flags of account `c` of instruction `b`: bit 0 signer, bit 1 writable.
pub const OP_INSTRUCTION_ACCOUNT_FLAGS: u8 = 69;
/// The data length of instruction `b`.
pub const OP_INSTRUCTION_DATA_LEN: u8 = 70;
/// A typed read from instruction `b`'s data at the `u64` offset in register `c`. The immediate is
/// the `OP_READ_*` opcode whose width and result type the read takes.
pub const OP_READ_INSTRUCTION_DATA: u8 = 71;
/// Exactly `immediate` bytes of instruction `b`'s data from the offset in register `c`, borrowed
/// from the sysvar rather than copied.
pub const OP_READ_INSTRUCTION_BYTES: u8 = 72;
/// Exactly `immediate` bytes of account `a`'s data from the `u64` offset in register `b`. The
/// account must be read-only in the transaction, so the bytes can be borrowed for the whole run.
pub const OP_READ_ACCOUNT_BYTES: u8 = 73;
/// The length of the `bytes` value in register `a`, as a `u64`.
pub const OP_BYTES_LEN: u8 = 74;
```

- [ ] **Step 4: The builder helpers.** In `builder.rs`, before `pub fn loop_index`:

```rust
    /// Emits one of the opcodes that read the Instructions sysvar in account `sysvar`, from
    /// `OP_INSTRUCTION_COUNT` to `OP_INSTRUCTION_DATA_LEN`. `index` and `position` are `u64`
    /// registers; pass `NO_INDEX` for an operand the opcode does not take.
    pub fn introspect(&mut self, opcode: u8, sysvar: u8, index: u8, position: u8) -> u8 {
        self.op(opcode, sysvar, index, position, 0)
    }

    /// A typed read from instruction `index`'s data at the `u64` offset in `offset`, with the
    /// width and result type of `read_opcode`, one of the `OP_READ_*` opcodes.
    pub fn read_instruction_data(
        &mut self,
        read_opcode: u8,
        sysvar: u8,
        index: u8,
        offset: u8,
    ) -> u8 {
        self.op(OP_READ_INSTRUCTION_DATA, sysvar, index, offset, read_opcode as u64)
    }

    /// Exactly `len` bytes of instruction `index`'s data from the `u64` offset in `offset`.
    pub fn read_instruction_bytes(&mut self, sysvar: u8, index: u8, offset: u8, len: u16) -> u8 {
        self.op(OP_READ_INSTRUCTION_BYTES, sysvar, index, offset, len as u64)
    }

    /// Exactly `len` bytes of `account`'s data from the `u64` offset in `offset`. The run fails
    /// unless the account is read-only in the transaction.
    pub fn read_account_bytes(&mut self, account: u8, offset: u8, len: u16) -> u8 {
        self.op(OP_READ_ACCOUNT_BYTES, account, offset, NO_INDEX, len as u64)
    }

    /// The length of the `bytes` value in `value`, as a `u64`.
    pub fn bytes_len(&mut self, value: u8) -> u8 {
        self.op(OP_BYTES_LEN, value, NO_INDEX, NO_INDEX, 0)
    }
```

The TypeScript compiler emits the same records: unused operands are `NO_INDEX` and unused
immediates are zero (Task 5 checks the operands it emits).

- [ ] **Step 5: The verifier rules.** In `verify.rs`, in `verify_instruction`, directly after the
  `OP_POW10` arm:

```rust
            OP_INSTRUCTION_COUNT | OP_INSTRUCTION_INDEX => {
                self.require_instructions_sysvar(instruction, instruction_index)?;
                self.write_register(registers, instruction.dst, scalar(VALUE_U64))?;
            }
            OP_INSTRUCTION_PROGRAM | OP_INSTRUCTION_ACCOUNT_COUNT | OP_INSTRUCTION_DATA_LEN => {
                self.require_instructions_sysvar(instruction, instruction_index)?;
                self.require_type(registers, instruction.b, VALUE_U64)?;
                let value_type = if instruction.opcode == OP_INSTRUCTION_PROGRAM {
                    VALUE_PUBKEY
                } else {
                    VALUE_U64
                };
                self.write_register(registers, instruction.dst, scalar(value_type))?;
            }
            OP_INSTRUCTION_ACCOUNT | OP_INSTRUCTION_ACCOUNT_FLAGS => {
                self.require_instructions_sysvar(instruction, instruction_index)?;
                self.require_type(registers, instruction.b, VALUE_U64)?;
                self.require_type(registers, instruction.c, VALUE_U64)?;
                let value_type = if instruction.opcode == OP_INSTRUCTION_ACCOUNT {
                    VALUE_PUBKEY
                } else {
                    VALUE_U64
                };
                self.write_register(registers, instruction.dst, scalar(value_type))?;
            }
            OP_READ_INSTRUCTION_DATA => {
                self.require_instructions_sysvar(instruction, instruction_index)?;
                self.require_type(registers, instruction.b, VALUE_U64)?;
                self.require_type(registers, instruction.c, VALUE_U64)?;
                // The immediate names a read opcode, as `RETURN_DATA`'s `a` does. `read_type` falls
                // back to `u64` for any other opcode, so the selector must be a read first.
                let selector = u8::try_from(instruction.immediate())
                    .ok()
                    .filter(|selector| read_width(*selector) != 0)
                    .ok_or(TemplateError::InvalidInstruction(instruction_index))?;
                self.write_register(registers, instruction.dst, scalar(read_type(selector)))?;
            }
            OP_READ_INSTRUCTION_BYTES => {
                self.require_instructions_sysvar(instruction, instruction_index)?;
                self.require_type(registers, instruction.b, VALUE_U64)?;
                self.require_type(registers, instruction.c, VALUE_U64)?;
                let len = byte_read_len(instruction, instruction_index)?;
                self.write_register(registers, instruction.dst, RegisterInfo::bytes(len))?;
            }
            OP_READ_ACCOUNT_BYTES => {
                self.require_account(instruction.a, in_loop)?;
                self.require_type(registers, instruction.b, VALUE_U64)?;
                let len = byte_read_len(instruction, instruction_index)?;
                self.write_register(registers, instruction.dst, RegisterInfo::bytes(len))?;
            }
            OP_BYTES_LEN => {
                if self.read_register(registers, instruction.a)?.value_type != VALUE_BYTES {
                    return Err(TemplateError::TypeMismatch);
                }
                self.write_register(registers, instruction.dst, scalar(VALUE_U64))?;
            }
```

  Directly before `fn require_account`:

```rust
    /// Introspection reads the Instructions sysvar, so `a` must be a fixed account pinned to its
    /// address. The executor borrows that account's data for the whole run on the strength of it.
    fn require_instructions_sysvar(
        &self,
        instruction: &InstructionRecord,
        instruction_index: usize,
    ) -> Result<(), TemplateError> {
        // `in_loop` is false, so a row account never matches, even inside a loop body.
        let pinned = self
            .account_constraint(instruction.a, false)
            .filter(|constraint| constraint.address_index != NO_INDEX)
            .and_then(|constraint| self.pubkeys.get(constraint.address_index as usize))
            .is_some_and(|address| address.bytes == INSTRUCTIONS_SYSVAR_ID);
        if !pinned {
            return Err(TemplateError::InvalidIntrospection(instruction_index));
        }
        Ok(())
    }
```

  Directly before `fn valid_range`:

```rust
/// The length a byte read's immediate names: 1 to `MAX_INPUT_BYTES`, the bound on every `bytes`
/// value, so the result fits wherever a `bytes` input would.
fn byte_read_len(
    instruction: &InstructionRecord,
    instruction_index: usize,
) -> Result<usize, TemplateError> {
    usize::try_from(instruction.immediate())
        .ok()
        .filter(|len| (1..=MAX_INPUT_BYTES).contains(len))
        .ok_or(TemplateError::InvalidInstruction(instruction_index))
}
```

  The flags byte needs no new code: `verify_record_header` already allows flags only on read
  opcodes, and none of the eleven is one.

- [ ] **Step 6: Keep the TypeScript opcode table in step.** `opcodes.test.ts` fails as soon as
  `wire.rs` has opcodes the compiler lacks. In `compiler.ts`, after `readI32: 60,` in `opcode`:

```ts
  instructionCount: 64,
  instructionIndex: 65,
  instructionProgram: 66,
  instructionAccountCount: 67,
  instructionAccount: 68,
  instructionAccountFlags: 69,
  instructionDataLength: 70,
  readInstructionData: 71,
  readInstructionBytes: 72,
  readAccountBytes: 73,
  bytesLength: 74,
```

  In `opcodes.test.ts`, after `readI32: 'OP_READ_I32',` in `rustName`:

```ts
  instructionCount: 'OP_INSTRUCTION_COUNT',
  instructionIndex: 'OP_INSTRUCTION_INDEX',
  instructionProgram: 'OP_INSTRUCTION_PROGRAM',
  instructionAccountCount: 'OP_INSTRUCTION_ACCOUNT_COUNT',
  instructionAccount: 'OP_INSTRUCTION_ACCOUNT',
  instructionAccountFlags: 'OP_INSTRUCTION_ACCOUNT_FLAGS',
  instructionDataLength: 'OP_INSTRUCTION_DATA_LEN',
  readInstructionData: 'OP_READ_INSTRUCTION_DATA',
  readInstructionBytes: 'OP_READ_INSTRUCTION_BYTES',
  readAccountBytes: 'OP_READ_ACCOUNT_BYTES',
  bytesLength: 'OP_BYTES_LEN',
```

- [ ] **Step 7: The Rust SDK's addresses.** In `clients/rust/src/lib.rs`, after
  `ASSOCIATED_TOKEN_PROGRAM_ID`:

```rust
/// The Instructions sysvar. Introspecting templates declare it as a fixed account pinned to this
/// address; pass it read-only, as `AccountMeta::new_readonly(INSTRUCTIONS_SYSVAR_ID, false)`.
pub const INSTRUCTIONS_SYSVAR_ID: Pubkey = pubkey!("Sysvar1nstructions1111111111111111111111111");
/// The Ed25519 signature-verification precompile.
pub const ED25519_PROGRAM_ID: Pubkey = pubkey!("Ed25519SigVerify111111111111111111111111111");
```

  Before `fn error_codes_decode_with_context` in its tests:

```rust
    #[test]
    fn well_known_addresses_match_the_shared_constants() {
        assert_eq!(
            INSTRUCTIONS_SYSVAR_ID.to_bytes(),
            ballista_common::template::INSTRUCTIONS_SYSVAR_ID
        );
        assert_eq!(INSTRUCTIONS_SYSVAR_ID, solana_program::sysvar::instructions::ID);
        assert_eq!(ED25519_PROGRAM_ID, solana_program::ed25519_program::ID);
    }
```

- [ ] **Step 8: Run the tests.**

```bash
cargo test -p ballista-common --lib
cargo test -p ballista-sdk
pnpm --dir clients/js exec vitest run src/opcodes.test.ts
```

Expected: PASS, including the seven new verifier tests, the extended opcode sweep and
`well_known_addresses_match_the_shared_constants`.

- [ ] **Step 9: Commit.**

```bash
git add common/src/template clients/rust/src/lib.rs clients/js/src/compiler.ts clients/js/src/opcodes.test.ts
git commit -m "Verify eleven introspection and byte opcodes against a pinned Instructions sysvar"
```

---

### Task 4: The executor

**Files:**
- Create: `programs/ballista/src/processor/introspect.rs`.
- Modify: `programs/ballista/src/processor/mod.rs`.
- Modify: `programs/ballista/src/processor/execute.rs`:
  - the `use super::math;` line;
  - `extended_instruction`'s inner match and its doc comment;
  - the test `opcodes_the_executor_does_not_run_fail_before_reading_operands`.

The router stays thin, per the Task 9 review:
- The count and the index are two bytes at either end of the sysvar, and `BYTES_LEN` reads one
  register. Those three run in `extended_instruction`'s inner match, through `#[inline(always)]`
  helpers.
- Parsing the sysvar and reading byte ranges run in two `#[inline(never)]` helpers, so their locals
  never widen the router's frame.
- Each helper takes the resolved account, the register slice and the record: four words, all
  passed in registers. The router resolves the account first, because a helper that also took the
  loop context would take words from the stack. On `7a5e15e`'s layout, the loads for those words
  moved to the router's entry and cost every math opcode about 5 CU.

- [ ] **Step 1: Declare the module.** In `processor/mod.rs`, before the `math` declarations:

```rust
#[cfg(not(feature = "spec-api"))]
mod introspect;
#[cfg(feature = "spec-api")]
pub mod introspect;

```

- [ ] **Step 2: Write the module's tests first.** Create
  `programs/ballista/src/processor/introspect.rs` with the header, the imports, and the tests; the
  functions follow in Step 4.
  - The tests build sysvar data exactly as `solana-instructions-sysvar`'s
    `construct_instructions_data` lays it out. Task 9 checks the same code against the real
    sysvar.
  - They also build `AccountView`s over an aligned header-plus-data buffer, the way the entrypoint
    hands accounts over, so the borrow flag can be observed.

```rust
//! Transaction introspection through the Instructions sysvar, and bounded byte reads from
//! instruction data and from read-only accounts.
//!
//! Nothing here copies bytes. A `bytes` result borrows the sysvar's data or the account's for the
//! rest of the run, which is sound only because neither can change while this instruction runs:
//! see `sysvar_data` and `read_only_data`.

use ballista_common::template::*;
use pinocchio::{
    sysvars::instructions::{Instructions, INSTRUCTIONS_ID},
    AccountView,
};

use super::execute::{get, read_value, set, RunError, RunResult, RuntimeValue};
use crate::error::BallistaError;

// (functions go here, Step 4)

#[cfg(test)]
mod tests {
    use super::*;
    use pinocchio::account::{RuntimeAccount, NOT_BORROWED};
    use pinocchio::Address;
    use RuntimeValue::{Bytes, Pubkey, Unset, U64};

    /// One instruction as the sysvar lists it: program, `(key, signer, writable)` accounts, data.
    type Listed<'a> = ([u8; 32], &'a [([u8; 32], bool, bool)], &'a [u8]);

    /// The Instructions sysvar's data for `instructions`, laid out as `construct_instructions_data`
    /// writes it, with `current` as the running instruction's index.
    fn sysvar(instructions: &[Listed<'_>], current: u16) -> Vec<u8> {
        let mut data = (instructions.len() as u16).to_le_bytes().to_vec();
        data.resize(2 + 2 * instructions.len(), 0);
        for (index, (program, accounts, bytes)) in instructions.iter().enumerate() {
            let start = data.len() as u16;
            data[2 + 2 * index..4 + 2 * index].copy_from_slice(&start.to_le_bytes());
            data.extend_from_slice(&(accounts.len() as u16).to_le_bytes());
            for (key, signer, writable) in *accounts {
                data.push(u8::from(*signer) | u8::from(*writable) << 1);
                data.extend_from_slice(key);
            }
            data.extend_from_slice(program);
            data.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            data.extend_from_slice(bytes);
        }
        data.extend_from_slice(&current.to_le_bytes());
        data
    }

    fn err(kind: BallistaError) -> RunError {
        RunError::Vm(kind)
    }

    /// A memo signed by `[1; 32]`, this template's run with a writable `[2; 32]`, and a bare memo.
    fn three_instructions() -> Vec<u8> {
        sysvar(
            &[
                ([5; 32], &[([1; 32], true, false)], b"before"),
                ([9; 32], &[([2; 32], false, true), ([1; 32], true, false)], &[0x2a]),
                ([5; 32], &[], b"after"),
            ],
            1,
        )
    }

    /// Runs `opcode` with the instruction index in r0 and the position or offset in r1.
    fn run<'a>(
        sysvar: &'a [u8],
        registers: &[RuntimeValue<'a>],
        opcode: u8,
        immediate: u64,
    ) -> RunResult<RuntimeValue<'a>> {
        introspect(sysvar, registers, &record(opcode, 3, 0, 0, 1, 0, immediate))
    }

    #[test]
    fn counts_and_indexes_come_from_the_sysvar() {
        let data = three_instructions();
        assert_eq!(run(&data, &[U64(0), U64(0)], OP_INSTRUCTION_COUNT, 0), Ok(U64(3)));
        assert_eq!(run(&data, &[U64(0), U64(0)], OP_INSTRUCTION_INDEX, 0), Ok(U64(1)));
    }

    #[test]
    fn instruction_fields_read_the_indexed_instruction() {
        let data = three_instructions();
        // r0 is the instruction index, r1 the account position or data offset.
        assert_eq!(run(&data, &[U64(0), U64(0)], OP_INSTRUCTION_PROGRAM, 0), Ok(Pubkey([5; 32])));
        assert_eq!(run(&data, &[U64(1), U64(0)], OP_INSTRUCTION_PROGRAM, 0), Ok(Pubkey([9; 32])));
        assert_eq!(run(&data, &[U64(1), U64(0)], OP_INSTRUCTION_ACCOUNT_COUNT, 0), Ok(U64(2)));
        assert_eq!(run(&data, &[U64(2), U64(0)], OP_INSTRUCTION_ACCOUNT_COUNT, 0), Ok(U64(0)));
        assert_eq!(run(&data, &[U64(1), U64(1)], OP_INSTRUCTION_ACCOUNT, 0), Ok(Pubkey([1; 32])));
        assert_eq!(run(&data, &[U64(1), U64(0)], OP_INSTRUCTION_ACCOUNT_FLAGS, 0), Ok(U64(0b10)));
        assert_eq!(run(&data, &[U64(1), U64(1)], OP_INSTRUCTION_ACCOUNT_FLAGS, 0), Ok(U64(0b01)));
        assert_eq!(run(&data, &[U64(0), U64(0)], OP_INSTRUCTION_DATA_LEN, 0), Ok(U64(6)));
        assert_eq!(run(&data, &[U64(2), U64(0)], OP_INSTRUCTION_DATA_LEN, 0), Ok(U64(5)));
    }

    #[test]
    fn instruction_data_reads_take_the_selected_width() {
        let data = three_instructions();
        // "before" is 62 65 66 6f 72 65.
        assert_eq!(
            run(&data, &[U64(0), U64(1)], OP_READ_INSTRUCTION_DATA, OP_READ_U8 as u64),
            Ok(U64(0x65))
        );
        assert_eq!(
            run(&data, &[U64(0), U64(0)], OP_READ_INSTRUCTION_DATA, OP_READ_U32 as u64),
            Ok(U64(0x6f66_6562))
        );
        // The last two bytes fit; one more does not.
        assert_eq!(
            run(&data, &[U64(0), U64(4)], OP_READ_INSTRUCTION_DATA, OP_READ_U16 as u64),
            Ok(U64(0x6572))
        );
        assert_eq!(
            run(&data, &[U64(0), U64(5)], OP_READ_INSTRUCTION_DATA, OP_READ_U16 as u64),
            Err(err(BallistaError::InstructionOutOfRange))
        );
        assert_eq!(
            run(&data, &[U64(0), U64(0)], OP_READ_INSTRUCTION_DATA, OP_ADD as u64),
            Err(err(BallistaError::InvalidTemplateProgram))
        );
    }

    #[test]
    fn instruction_byte_reads_borrow_the_sysvar() {
        let data = three_instructions();
        let value = run(&data, &[U64(2), U64(0)], OP_READ_INSTRUCTION_BYTES, 5).unwrap();
        let Bytes(bytes) = value else { panic!("{value:?}") };
        assert_eq!(bytes, b"after");
        // A slice of the sysvar itself, not a copy: its last byte sits just before the index.
        assert_eq!(bytes.as_ptr_range().end, data[data.len() - 2..].as_ptr());
        assert_eq!(
            run(&data, &[U64(2), U64(1)], OP_READ_INSTRUCTION_BYTES, 5),
            Err(err(BallistaError::InstructionOutOfRange))
        );
        assert_eq!(
            run(&data, &[U64(0), U64(u64::MAX)], OP_READ_INSTRUCTION_BYTES, 1),
            Err(err(BallistaError::InstructionOutOfRange))
        );
    }

    #[test]
    fn indexes_and_positions_outside_the_transaction_fail() {
        let data = three_instructions();
        let out_of_range = Err(err(BallistaError::InstructionOutOfRange));
        for opcode in [OP_INSTRUCTION_PROGRAM, OP_INSTRUCTION_ACCOUNT_COUNT, OP_INSTRUCTION_DATA_LEN] {
            assert_eq!(run(&data, &[U64(3), U64(0)], opcode, 0), out_of_range, "{opcode}");
            assert_eq!(run(&data, &[U64(u64::MAX), U64(0)], opcode, 0), out_of_range, "{opcode}");
        }
        assert_eq!(run(&data, &[U64(1), U64(2)], OP_INSTRUCTION_ACCOUNT, 0), out_of_range);
        assert_eq!(run(&data, &[U64(2), U64(0)], OP_INSTRUCTION_ACCOUNT_FLAGS, 0), out_of_range);
        // An operand that is not a set u64 is the template's fault, not the transaction's.
        assert_eq!(
            run(&data, &[Unset, U64(0)], OP_INSTRUCTION_PROGRAM, 0),
            Err(err(BallistaError::InvalidRegister))
        );
        assert_eq!(
            run(&data, &[Pubkey([0; 32]), U64(0)], OP_INSTRUCTION_PROGRAM, 0),
            Err(err(BallistaError::TypeMismatch))
        );
    }

    #[test]
    fn bytes_len_measures_a_bytes_register() {
        assert_eq!(bytes_len(&Bytes(&[1, 2, 3])), Ok(3));
        assert_eq!(bytes_len(&Bytes(&[])), Ok(0));
        assert_eq!(bytes_len(&U64(3)), Err(err(BallistaError::TypeMismatch)));
        assert_eq!(bytes_len(&Unset), Err(err(BallistaError::InvalidRegister)));
    }

    #[test]
    fn a_slice_is_reborrowed_only_from_inside_its_parent() {
        let whole = [1u8, 2, 3, 4];
        assert_eq!(reborrow(&whole, &whole[1..3]), Some(&whole[1..3]));
        let elsewhere = [1u8, 2];
        assert_eq!(reborrow(&whole[1..], &elsewhere), None);
    }

    /// An account laid out as the entrypoint hands it over: the runtime header, then its data.
    struct TestAccount {
        buffer: Vec<u64>,
    }

    impl TestAccount {
        fn new(address: [u8; 32], writable: bool, data: &[u8]) -> Self {
            let header = core::mem::size_of::<RuntimeAccount>();
            let mut buffer = vec![0u64; (header + data.len()).div_ceil(8)];
            let account = RuntimeAccount {
                borrow_state: NOT_BORROWED,
                is_signer: 0,
                is_writable: u8::from(writable),
                executable: 0,
                padding: [0; 4],
                address: Address::new_from_array(address),
                owner: Address::new_from_array([0; 32]),
                lamports: 0,
                data_len: data.len() as u64,
            };
            // SAFETY: the buffer is eight-aligned and large enough for the header and the data.
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

    #[test]
    fn account_bytes_come_only_from_read_only_accounts_and_hold_no_borrow() {
        // Read three bytes from the offset in r0 into r1.
        let read = record(OP_READ_ACCOUNT_BYTES, 1, 0, 0, NO_INDEX, 0, 3);

        let mut read_only = TestAccount::new([4; 32], false, &[10, 11, 12, 13, 14]);
        let accounts = [read_only.view()];
        let mut registers = vec![U64(1), Unset];
        assert_eq!(read_account_bytes(&accounts[0], &mut registers, &read), Ok(()));
        assert_eq!(registers[1], Bytes(&[11, 12, 13]));
        assert!(!accounts[0].is_borrowed(), "the borrow flag is untouched");

        // A range past the end.
        let mut registers = vec![U64(3), Unset];
        assert_eq!(
            read_account_bytes(&accounts[0], &mut registers, &read),
            Err(err(BallistaError::InstructionOutOfRange))
        );

        let mut writable = TestAccount::new([4; 32], true, &[10, 11, 12, 13, 14]);
        let accounts = [writable.view()];
        let mut registers = vec![U64(1), Unset];
        assert_eq!(
            read_account_bytes(&accounts[0], &mut registers, &read),
            Err(err(BallistaError::WritableAccountBytesRead))
        );
        assert_eq!(registers[1], Unset);
    }

    #[test]
    fn introspection_refuses_any_account_but_the_sysvar() {
        // Read the program of the instruction in r0 into r1.
        let program = record(OP_INSTRUCTION_PROGRAM, 1, 0, 0, NO_INDEX, 0, 0);
        let data = three_instructions();

        let mut real = TestAccount::new(INSTRUCTIONS_SYSVAR_ID, false, &data);
        let sysvar = real.view();
        assert_eq!(count_or_index(OP_INSTRUCTION_COUNT, &sysvar), Ok(3));
        assert_eq!(count_or_index(OP_INSTRUCTION_INDEX, &sysvar), Ok(1));
        let mut registers = vec![U64(2), Unset];
        assert_eq!(read_instruction(&sysvar, &mut registers, &program), Ok(()));
        assert_eq!(registers[1], Pubkey([5; 32]));
        assert!(!sysvar.is_borrowed(), "the borrow flag is untouched");

        let mut impostor = TestAccount::new([6; 32], false, &data);
        let impostor = impostor.view();
        assert_eq!(
            count_or_index(OP_INSTRUCTION_COUNT, &impostor),
            Err(err(BallistaError::InvalidRuntimeAccount))
        );
        assert_eq!(
            read_instruction(&impostor, &mut registers, &program),
            Err(err(BallistaError::InvalidRuntimeAccount))
        );
    }

    #[test]
    fn the_sysvar_address_matches_pinocchio() {
        assert_eq!(INSTRUCTIONS_ID.to_bytes(), INSTRUCTIONS_SYSVAR_ID);
    }
}
```

pinocchio reads the sysvar's offset table as aligned `u16`s. A whole `Vec<u8>` from the global
allocator is aligned well past two bytes, so the host tests hold; do not slice a test sysvar from
an odd offset. The real sysvar sits eight-aligned in the program's input.

- [ ] **Step 3: Run the tests and confirm they fail.**

Run: `cargo test -p ballista --lib introspect`
Expected: compile errors. `introspect`, `count_or_index`, `read_instruction`,
`read_account_bytes`, `bytes_len` and `reborrow` are not defined.

- [ ] **Step 4: Implement.** Replace `// (functions go here, Step 4)` with:

```rust
/// `INSTRUCTION_COUNT` or `INSTRUCTION_INDEX`: the two bytes at the front or the back of the
/// sysvar. No parsing, so the router runs these itself.
#[inline(always)]
pub fn count_or_index(opcode: u8, sysvar: &AccountView) -> RunResult<u64> {
    // SAFETY: `sysvar_data` checked this is the Instructions sysvar, whose layout this parses.
    let instructions = unsafe { Instructions::new_unchecked(sysvar_data(sysvar)?) };
    Ok(if opcode == OP_INSTRUCTION_COUNT {
        instructions.num_instructions() as u64
    } else {
        u64::from(instructions.load_current_index())
    })
}

/// `BYTES_LEN`: the length of the `bytes` value in `value`, the register operand `a`.
#[inline(always)]
pub fn bytes_len(value: &RuntimeValue<'_>) -> RunResult<u64> {
    match value {
        RuntimeValue::Bytes(bytes) => Ok(bytes.len() as u64),
        RuntimeValue::Unset => Err(BallistaError::InvalidRegister.into()),
        _ => Err(BallistaError::TypeMismatch.into()),
    }
}

/// Runs one of the opcodes that parse the Instructions sysvar, from `OP_INSTRUCTION_PROGRAM` to
/// `OP_READ_INSTRUCTION_BYTES`, against `sysvar`, the account `a` names.
#[inline(never)]
pub fn read_instruction<'data>(
    sysvar: &'data AccountView,
    registers: &mut [RuntimeValue<'data>],
    instruction: &InstructionRecord,
) -> RunResult<()> {
    let value = introspect(sysvar_data(sysvar)?, registers, instruction)?;
    set(registers, instruction.dst as usize, value)
}

/// Runs `READ_ACCOUNT_BYTES`: `immediate` bytes of `account`'s data from the offset in register
/// `b`.
#[inline(never)]
pub fn read_account_bytes<'data>(
    account: &'data AccountView,
    registers: &mut [RuntimeValue<'data>],
    instruction: &InstructionRecord,
) -> RunResult<()> {
    let offset = position(registers, instruction.b)?;
    let bytes = byte_range(read_only_data(account)?, offset, instruction.immediate())?;
    set(
        registers,
        instruction.dst as usize,
        RuntimeValue::Bytes(bytes),
    )
}

/// The value one sysvar opcode reads from `sysvar`, the Instructions sysvar's data. Public for
/// the host tests and formal specifications, which have no account to hand it.
pub fn introspect<'data>(
    sysvar: &'data [u8],
    registers: &[RuntimeValue<'data>],
    instruction: &InstructionRecord,
) -> RunResult<RuntimeValue<'data>> {
    // SAFETY: `sysvar` is the Instructions sysvar's data, which the runtime writes in exactly the
    // layout `Instructions` parses: `sysvar_data` checked the account's address.
    let instructions = unsafe { Instructions::new_unchecked(sysvar) };
    match instruction.opcode {
        OP_INSTRUCTION_COUNT => {
            return Ok(RuntimeValue::U64(instructions.num_instructions() as u64))
        }
        OP_INSTRUCTION_INDEX => {
            return Ok(RuntimeValue::U64(u64::from(
                instructions.load_current_index(),
            )))
        }
        _ => {}
    }
    // pinocchio checks the index and the account position; the data ranges are checked here.
    let index = position(registers, instruction.b)?;
    let introspected = instructions
        .load_instruction_at(index)
        .map_err(|_| out_of_range())?;
    Ok(match instruction.opcode {
        OP_INSTRUCTION_PROGRAM => RuntimeValue::Pubkey(introspected.get_program_id().to_bytes()),
        OP_INSTRUCTION_ACCOUNT_COUNT => RuntimeValue::U64(introspected.num_account_metas() as u64),
        OP_INSTRUCTION_ACCOUNT | OP_INSTRUCTION_ACCOUNT_FLAGS => {
            let account = introspected
                .get_instruction_account_at(position(registers, instruction.c)?)
                .map_err(|_| out_of_range())?;
            if instruction.opcode == OP_INSTRUCTION_ACCOUNT {
                RuntimeValue::Pubkey(account.key.to_bytes())
            } else {
                RuntimeValue::U64(
                    u64::from(account.is_signer()) | u64::from(account.is_writable()) << 1,
                )
            }
        }
        OP_INSTRUCTION_DATA_LEN => {
            RuntimeValue::U64(introspected.get_instruction_data().len() as u64)
        }
        OP_READ_INSTRUCTION_DATA => {
            // The verifier admits only read opcodes, which all fit a byte.
            let selector = instruction.immediate() as u8;
            let width = read_width(selector);
            if width == 0 {
                return Err(BallistaError::InvalidTemplateProgram.into());
            }
            let offset = position(registers, instruction.c)?;
            let data = introspected.get_instruction_data();
            // Checked here so a short read is out of range, not `read_value`'s account error.
            byte_range(data, offset, width as u64)?;
            read_value(selector, data, offset)?
        }
        OP_READ_INSTRUCTION_BYTES => {
            let data =
                reborrow(sysvar, introspected.get_instruction_data()).ok_or_else(out_of_range)?;
            RuntimeValue::Bytes(byte_range(
                data,
                position(registers, instruction.c)?,
                instruction.immediate(),
            )?)
        }
        _ => return Err(BallistaError::InvalidTemplateProgram.into()),
    })
}

/// The Instructions sysvar's data, borrowed for the rest of the run.
fn sysvar_data(account: &AccountView) -> RunResult<&[u8]> {
    // The verifier pinned this account to the sysvar, and the run checked the pin before the first
    // instruction. Checked again because the borrow below is sound for this one account only.
    if account.address() != &INSTRUCTIONS_ID {
        return Err(BallistaError::InvalidRuntimeAccount.into());
    }
    // SAFETY: the runtime builds this account's data from the transaction before any program runs,
    // and never resizes it. A sysvar is read-only in every transaction, so no program can write it
    // or borrow it mutably. The runtime itself rewrites only the trailing current-instruction
    // index, and while this instruction runs, inside any CPI it makes too, the index it writes is
    // this instruction's own: the bytes never change. `borrow_unchecked` leaves the borrow flag
    // alone, so holding the slice across a CPI cannot make that CPI's borrow check fail.
    Ok(unsafe { account.borrow_unchecked() })
}

/// The data of an account the transaction cannot write, borrowed for the rest of the run.
fn read_only_data(account: &AccountView) -> RunResult<&[u8]> {
    if account.is_writable() {
        return Err(BallistaError::WritableAccountBytesRead.into());
    }
    // SAFETY: the account is read-only in this transaction. No program, this one included, can
    // write its data or change its length, and after a CPI the runtime copies nothing back into a
    // read-only account, so the bytes stay as they are for the rest of the run. `borrow_unchecked`
    // leaves the borrow flag alone, so a later CPI that passes this account still passes its
    // borrow check.
    Ok(unsafe { account.borrow_unchecked() })
}

/// A `u64` register used as an instruction index, account position or byte offset.
fn position(registers: &[RuntimeValue<'_>], register: u8) -> RunResult<usize> {
    match get(registers, register)? {
        RuntimeValue::U64(value) => usize::try_from(value).map_err(|_| out_of_range()),
        _ => Err(BallistaError::TypeMismatch.into()),
    }
}

/// Exactly `len` bytes of `data` from `offset`, or `InstructionOutOfRange` when they are not all
/// there.
fn byte_range(data: &[u8], offset: usize, len: u64) -> RunResult<&[u8]> {
    let len = usize::try_from(len).map_err(|_| BallistaError::InvalidTemplateProgram)?;
    offset
        .checked_add(len)
        .and_then(|end| data.get(offset..end))
        .ok_or_else(out_of_range)
}

/// `part`, a slice inside `whole`, as a borrow of `whole`. pinocchio ties every slice it returns
/// to a borrow of its parser, which ends with the opcode; the bytes live as long as `whole`.
/// Bounds-checked, so a `part` from anywhere else gives `None` instead of an alias.
fn reborrow<'data>(whole: &'data [u8], part: &[u8]) -> Option<&'data [u8]> {
    let start = (part.as_ptr() as usize).checked_sub(whole.as_ptr() as usize)?;
    whole.get(start..start.checked_add(part.len())?)
}

fn out_of_range() -> RunError {
    BallistaError::InstructionOutOfRange.into()
}
```

Why these pieces:
- `introspect` handles the count and the index too, so the pure function covers every sysvar
  opcode for the tests and the specs, even though the router takes a shorter path for those two.
  Leave the duplication: sharing one helper between `count_or_index` and `introspect` measured
  10 CU more on Task 11's case.
- `get`, `set`, `read_value`, `RunError`, `RunResult` and `RuntimeValue` are already `pub` in
  `execute.rs`.
- `read_width` comes from `ballista_common::template`.

- [ ] **Step 5: Route the opcodes.** In `execute.rs`, change `use super::math;` to
  `use super::{introspect, math};`. Then add these arms to `extended_instruction`'s inner match,
  directly before its final `_` arm, which fails with `InvalidTemplateProgram` before reading any
  operand. Use the form that matches the shape Task 0 recorded; the two differ only in how an arm
  writes its destination.

  **(a) The `(machine, instruction, loop_context)` shape**, where every arm writes its own
  destination and evaluates to `RunResult<()>`:

```rust
        // Introspection names a fixed account, so it resolves without the loop context. The count
        // and the index are two bytes at either end of the sysvar and need no parsing.
        OP_INSTRUCTION_COUNT | OP_INSTRUCTION_INDEX => {
            let sysvar = resolve(machine.program, machine.accounts, instruction.a, None)?;
            let value = introspect::count_or_index(instruction.opcode, sysvar)?;
            set(machine.registers, instruction.dst as usize, RuntimeValue::U64(value))
        }
        // Parsing the sysvar and reading byte ranges run in their own frames. Each helper takes
        // four words, all passed in registers: one that took the loop context as well would take
        // words from the stack, and their loads would run on entry here, for every opcode.
        OP_INSTRUCTION_PROGRAM..=OP_READ_INSTRUCTION_BYTES => {
            let sysvar = resolve(machine.program, machine.accounts, instruction.a, None)?;
            introspect::read_instruction(sysvar, machine.registers, instruction)
        }
        OP_READ_ACCOUNT_BYTES => {
            let account = resolve(machine.program, machine.accounts, instruction.a, loop_context)?;
            introspect::read_account_bytes(account, machine.registers, instruction)
        }
        OP_BYTES_LEN => {
            let length = introspect::bytes_len(operand(machine.registers, instruction.a)?)?;
            set(machine.registers, instruction.dst as usize, RuntimeValue::U64(length))
        }
```

  **(b) The `7a5e15e` shape**, where arms evaluate to a value that one `set` after the match
  stores, and `read_account` for `READ_I32` returns early:

```rust
        // Introspection names a fixed account, so it resolves without the loop context. The count
        // and the index are two bytes at either end of the sysvar and need no parsing.
        OP_INSTRUCTION_COUNT | OP_INSTRUCTION_INDEX => {
            let sysvar = resolve(program, accounts, instruction.a, None)?;
            RuntimeValue::U64(introspect::count_or_index(instruction.opcode, sysvar)?)
        }
        // Parsing the sysvar and reading byte ranges run in their own frames. Each helper takes
        // four words, all passed in registers: one that took the loop context as well would take
        // words from the stack, and their loads would run on entry here, for every opcode.
        OP_INSTRUCTION_PROGRAM..=OP_READ_INSTRUCTION_BYTES => {
            let sysvar = resolve(program, accounts, instruction.a, None)?;
            return introspect::read_instruction(sysvar, registers, instruction);
        }
        OP_READ_ACCOUNT_BYTES => {
            let account = resolve(program, accounts, instruction.a, loop_context)?;
            return introspect::read_account_bytes(account, registers, instruction);
        }
        OP_BYTES_LEN => {
            RuntimeValue::U64(introspect::bytes_len(operand(registers, instruction.a)?)?)
        }
```

  Form (b) is what the prototype of this plan ran and measured. Form (a) was type-checked inside a
  `Machine`-shaped probe of the router.
  - Extend `extended_instruction`'s doc comment to say it also runs the introspection and byte
    opcodes.
  - If the outer match's `_` arm carries a comment listing what reaches the helper, add them
    there.
  - Do not add an arm, guard or `if` to `execute_instruction`'s outer match.

- [ ] **Step 6: Keep the unrun-opcode probe off this phase's opcodes.** In
  `opcodes_the_executor_does_not_run_fail_before_reading_operands`, replace the opcode list's
  `OP_READ_I32 + 1` with `OP_BYTES_LEN + 1, 0xfe`, so the list reads:

```rust
        for opcode in [0, 39, OP_FOREACH, OP_BYTES_LEN + 1, 0xfe, u8::MAX] {
```

  61 to 63 belong to phases 2 and 3, and 64 to 74 to this one. 75 and `0xfe` stay unassigned.

- [ ] **Step 7: Run the tests.**

Run: `cargo test -p ballista --lib`
Expected: PASS: the ten `introspect` tests, `the_sysvar_address_matches_pinocchio`, and every
existing executor test.

- [ ] **Step 8: Build for SBF.**

Run: `cargo build-sbf --manifest-path programs/ballista/Cargo.toml 2>&1 | grep -i -E "stack|frame|error" || true`
Expected: no output. Both helpers are small: `read_instruction` holds one parsed instruction and a
value, and `read_account_bytes` a slice.

- [ ] **Step 9: Commit.**

```bash
git add programs/ballista/src/processor
git commit -m "Read the transaction's instructions and borrow read-only byte ranges without copying"
```

---

### Task 5: TypeScript expressions

**Files:**
- `clients/js/src/schema.ts`:
  - after `ReadTypeSchema` (21–22);
  - after `const label` (8);
  - the `Expression` type (143–145);
  - `ExpressionSchema` (233–240);
  - after `binary` (433–434);
  - the `expression` object (496–497).
- `clients/js/src/helpers.ts`: after `ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES`.
- `clients/js/src/compiler.ts`:
  - the imports (1–16);
  - `readWidth` (172–182);
  - `compileExpression`, before the `select` branch (778);
  - before `requirePinnedForRead` (847).
- `clients/js/src/compiler.test.ts`.

- [ ] **Step 1: Write the failing tests.** In `compiler.test.ts`, add
  `INSTRUCTIONS_SYSVAR_ADDRESS_BYTES` and `ED25519_PROGRAM_ADDRESS_BYTES` to the import from
  `./index.js`, then append:

```ts
describe('introspection expressions', () => {
  const sysvar = account.fixed('instructions');
  const compileWith = (steps: Step[], accounts: TemplateInput['accounts'] = {}) =>
    compileTemplate(
      defineTemplate({
        inputs: { index: { type: 'u64' }, offset: { type: 'u64' } },
        accounts: { instructions: { address: INSTRUCTIONS_SYSVAR_ADDRESS_BYTES }, ...accounts },
        steps,
      }),
    );
  /** The a, b, c and immediate of every record with `wanted`'s opcode. */
  const operandsOf = (compiled: CompiledTemplate, wanted: number) =>
    records(compiled)
      .filter((record) => record[0] === wanted)
      .map((record) => [record[2], record[3], record[4], readU64(record, 6)]);

  test('each expression lowers to its opcode, with the sysvar in a and u64 registers in b and c', () => {
    const index = expression.input('index');
    const offset = expression.input('offset');
    const compiled = compileWith([
      step.let('count', expression.instructionCount(sysvar)),
      step.let('current', expression.currentInstructionIndex(sysvar)),
      step.let('program', expression.instructionProgram(sysvar, index)),
      step.let('accounts', expression.instructionAccountCount(sysvar, index)),
      step.let('key', expression.instructionAccount(sysvar, index, offset)),
      step.let('flags', expression.instructionAccountFlags(sysvar, index, offset)),
      step.let('length', expression.instructionDataLength(sysvar, index)),
      step.let('word', expression.instructionData(sysvar, index, offset, 'i32')),
      step.let('bytes', expression.instructionDataBytes(sysvar, index, offset, 12)),
      step.require(expression.equal(expression.bytesLength(expression.variable('bytes')), expression.u64(12))),
    ]);
    // Inputs load first: `index` into r0, `offset` into r1. Account 0 is the sysvar.
    const none = 0xff;
    expect(operandsOf(compiled, opcode.instructionCount)).toEqual([[0, none, none, 0n]]);
    expect(operandsOf(compiled, opcode.instructionIndex)).toEqual([[0, none, none, 0n]]);
    expect(operandsOf(compiled, opcode.instructionProgram)).toEqual([[0, 0, none, 0n]]);
    expect(operandsOf(compiled, opcode.instructionAccountCount)).toEqual([[0, 0, none, 0n]]);
    expect(operandsOf(compiled, opcode.instructionAccount)).toEqual([[0, 0, 1, 0n]]);
    expect(operandsOf(compiled, opcode.instructionAccountFlags)).toEqual([[0, 0, 1, 0n]]);
    expect(operandsOf(compiled, opcode.instructionDataLength)).toEqual([[0, 0, none, 0n]]);
    expect(operandsOf(compiled, opcode.readInstructionData)).toEqual([[0, 0, 1, BigInt(opcode.readI32)]]);
    expect(operandsOf(compiled, opcode.readInstructionBytes)).toEqual([[0, 0, 1, 12n]]);
    expect(operandsOf(compiled, opcode.bytesLength)).toHaveLength(1);
  });

  test('a number for an index, position or offset becomes a shared u64 constant', () => {
    const compiled = compileWith([
      step.require(
        expression.equal(
          expression.instructionData(sysvar, 2, 0, 'u8'),
          expression.instructionData(sysvar, 2, 1, 'u8'),
        ),
      ),
    ]);
    expect(records(compiled).filter((record) => record[0] === opcode.constU64)).toHaveLength(3);
  });

  test('results are typed: keys are pubkeys, an i32 read is an i64, byte reads are that long', () => {
    expect(() =>
      compileWith([
        step.require(
          expression.equal(expression.instructionProgram(sysvar, 0), expression.pubkey(ED25519_PROGRAM_ADDRESS_BYTES)),
        ),
        step.require(expression.lessThan(expression.instructionData(sysvar, 0, 0, 'i32'), expression.i64(0))),
      ]),
    ).not.toThrow();
    expect(() =>
      compileWith([step.require(expression.equal(expression.instructionAccount(sysvar, 0, 0), expression.u64(0)))]),
    ).toThrow(/matching types/);
    // A byte read forwarded to a CPI declares exactly its length.
    const compiled = compileWith(
      [
        step.invoke({
          program: account.fixed('program'),
          accounts: [],
          data: [data.encode('bytes', expression.instructionDataBytes(sysvar, 0, 0, 40))],
        }),
      ],
      { program: { executable: true, address: address(9) } },
    );
    expect(compiled.stats.maxCpiDataLength).toBe(40);
  });

  test('the sysvar must be a fixed account pinned to its address', () => {
    for (const accounts of [{ other: {} }, { other: { address: address(3) } }]) {
      expect(() =>
        compileTemplate(
          defineTemplate({
            accounts,
            steps: [step.require(expression.equal(expression.instructionCount(account.fixed('other')), expression.u64(1)))],
          }),
        ),
      ).toThrow(/Instructions sysvar/);
    }
    expect(() =>
      compileTemplate(
        defineTemplate({
          accounts: {},
          batch: { maxIterations: 1, row: { rowSysvar: { address: INSTRUCTIONS_SYSVAR_ADDRESS_BYTES } } },
          steps: [
            step.forEach([
              step.require(
                expression.equal(expression.instructionCount(account.iteration('rowSysvar')), expression.u64(1)),
              ),
            ]),
          ],
        }),
      ),
    ).toThrow(/Instructions sysvar/);
    expect(() =>
      compileWith([
        step.require(expression.equal(expression.instructionDataLength(sysvar, expression.i64(0)), expression.u64(1))),
      ]),
    ).toThrow(/instruction index requires u64/);
  });

  test('accountDataBytes reads a pinned, read-only account', () => {
    const read = (constraint: TemplateInput['accounts'][string]) =>
      compileTemplate(
        defineTemplate({
          accounts: { mint: constraint },
          steps: [
            step.require(
              expression.equal(expression.accountDataBytes(account.fixed('mint'), 44, 1), expression.bytes(Uint8Array.of(6))),
            ),
          ],
        }),
      );
    const compiled = read({ owner: TOKEN_PROGRAM_ADDRESS_BYTES });
    expect(operandsOf(compiled, opcode.readAccountBytes)).toEqual([[0, 0, 0xff, 1n]]);
    expect(() => read({})).toThrow(/pins neither owner nor address/);
    expect(() => read({ owner: TOKEN_PROGRAM_ADDRESS_BYTES, writable: true })).toThrow(/declared writable/);
  });

  test('bytesLength takes bytes; byte reads take 1 to 1024 bytes', () => {
    expect(() => compileWith([step.let('n', expression.bytesLength(expression.input('index')))])).toThrow(
      /bytesLength requires bytes/,
    );
    for (const length of [0, 1025, 1.5]) {
      expect(() => compileWith([step.let('b', expression.instructionDataBytes(sysvar, 0, 0, length))])).toThrow();
    }
  });

  test('isSigner and isWritable test one flag bit', () => {
    const compiled = compileWith([
      step.require(expression.instructionAccountIsSigner(sysvar, 0, 1)),
      step.require(expression.not(expression.instructionAccountIsWritable(sysvar, 0, 1))),
    ]);
    // Each flag is its own read, masked by its own bit.
    expect(operandsOf(compiled, opcode.instructionAccountFlags)).toHaveLength(2);
    expect(records(compiled).filter((record) => record[0] === opcode.bitAnd)).toHaveLength(2);
  });
});
```

`records`, `readU64`, `address`, `CompiledTemplate`, `Step`, `TemplateInput` and `opcode` are
already in `compiler.test.ts`, from the math tests and before.

- [ ] **Step 2: Run them and confirm they fail.**

Run: `pnpm --dir clients/js exec vitest run src/compiler.test.ts -t "introspection expressions"`
Expected: FAIL. `INSTRUCTIONS_SYSVAR_ADDRESS_BYTES` is not exported and `expression.instructionCount`
does not exist.

- [ ] **Step 3: Extend the schema** in `schema.ts`.
  - Move the read widths here, so `helpers.ts` can bound `field` without importing the compiler.
    Replace the `ReadTypeSchema` comment and add the table after the `ReadType` line:

```ts
/** Widths a template can read from account data, return data, or instruction data. */
export const ReadTypeSchema = z.enum(['bool', 'u8', 'u16', 'u32', 'i32', 'u64', 'i64', 'u128', 'pubkey']);
export type ReadType = z.infer<typeof ReadTypeSchema>;

/** Bytes each read type occupies. */
export const readWidth: Record<ReadType, number> = {
  bool: 1,
  u8: 1,
  u16: 2,
  u32: 4,
  i32: 4,
  u64: 8,
  i64: 8,
  u128: 16,
  pubkey: 32,
};
```

  - After `const label = …;`:

```ts
/** A byte read's length: 1 to 1,024, the limit on every `bytes` value. */
const byteReadLength = z.number().int().min(1).max(1_024);
```

  - In the `Expression` type, replace the last member,
    `| { kind: 'cast'; to: 'u64' | 'i64' | 'u128'; value: Expression };`, with:

```ts
  | { kind: 'cast'; to: 'u64' | 'i64' | 'u128'; value: Expression }
  /** How many instructions the transaction holds, from the Instructions sysvar `sysvar` names. */
  | { kind: 'instructionCount'; sysvar: AccountReference }
  /** The index of the instruction running this template. */
  | { kind: 'currentInstructionIndex'; sysvar: AccountReference }
  | {
      /** A field of the transaction's instruction at `index`. */
      kind: 'instruction';
      sysvar: AccountReference;
      index: Expression;
      field: 'program' | 'accountCount' | 'dataLength';
    }
  | {
      /** Account `position` of instruction `index`: its key, or its flags (bit 0 signer, bit 1 writable). */
      kind: 'instructionAccount';
      sysvar: AccountReference;
      index: Expression;
      position: Expression;
      field: 'key' | 'flags';
    }
  | {
      /** A typed read from instruction `index`'s data at a `u64` offset. */
      kind: 'instructionData';
      sysvar: AccountReference;
      index: Expression;
      offset: Expression;
      type: ReadType;
    }
  | {
      /** Exactly `length` bytes of instruction `index`'s data from a `u64` offset. */
      kind: 'instructionDataBytes';
      sysvar: AccountReference;
      index: Expression;
      offset: Expression;
      length: number;
    }
  | {
      /** Exactly `length` bytes of a read-only account's data from a `u64` offset. */
      kind: 'accountDataBytes';
      account: AccountReference;
      offset: Expression;
      length: number;
    }
  | { kind: 'bytesLength'; value: Expression };
```

  - In `ExpressionSchema`, after the `cast` object and before `]),`:

```ts
    z.object({ kind: z.literal('instructionCount'), sysvar: AccountReferenceSchema }).strict(),
    z.object({ kind: z.literal('currentInstructionIndex'), sysvar: AccountReferenceSchema }).strict(),
    z
      .object({
        kind: z.literal('instruction'),
        sysvar: AccountReferenceSchema,
        index: ExpressionSchema,
        field: z.enum(['program', 'accountCount', 'dataLength']),
      })
      .strict(),
    z
      .object({
        kind: z.literal('instructionAccount'),
        sysvar: AccountReferenceSchema,
        index: ExpressionSchema,
        position: ExpressionSchema,
        field: z.enum(['key', 'flags']),
      })
      .strict(),
    z
      .object({
        kind: z.literal('instructionData'),
        sysvar: AccountReferenceSchema,
        index: ExpressionSchema,
        offset: ExpressionSchema,
        type: ReadTypeSchema,
      })
      .strict(),
    z
      .object({
        kind: z.literal('instructionDataBytes'),
        sysvar: AccountReferenceSchema,
        index: ExpressionSchema,
        offset: ExpressionSchema,
        length: byteReadLength,
      })
      .strict(),
    z
      .object({
        kind: z.literal('accountDataBytes'),
        account: AccountReferenceSchema,
        offset: ExpressionSchema,
        length: byteReadLength,
      })
      .strict(),
    z.object({ kind: z.literal('bytesLength'), value: ExpressionSchema }).strict(),
```

  - After `const binary = …;`:

```ts
/** An index, position or offset: a number becomes a `u64` constant. */
const u64Operand = (value: number | Expression): Expression =>
  typeof value === 'number' ? literal({ type: 'u64', value: BigInt(value) }) : value;
const instructionField = (field: Extract<Expression, { kind: 'instruction' }>['field']) =>
  (sysvar: AccountReference, index: number | Expression): Expression => ({
    kind: 'instruction',
    sysvar,
    index: u64Operand(index),
    field,
  });
/** Whether a flag bit of account `position` of instruction `index` is set. */
const instructionAccountFlag = (bit: bigint) =>
  (sysvar: AccountReference, index: number | Expression, position: number | Expression): Expression => ({
    kind: 'binary',
    op: 'notEqual',
    left: {
      kind: 'binary',
      op: 'bitAnd',
      left: {
        kind: 'instructionAccount',
        sysvar,
        index: u64Operand(index),
        position: u64Operand(position),
        field: 'flags',
      },
      right: literal({ type: 'u64', value: bit }),
    },
    right: literal({ type: 'u64', value: 0n }),
  });
```

  - In `expression`, after `cast: …,`:

```ts
  instructionCount: (sysvar: AccountReference): Expression => ({ kind: 'instructionCount', sysvar }),
  currentInstructionIndex: (sysvar: AccountReference): Expression => ({ kind: 'currentInstructionIndex', sysvar }),
  instructionProgram: instructionField('program'),
  instructionAccountCount: instructionField('accountCount'),
  instructionDataLength: instructionField('dataLength'),
  instructionAccount: (
    sysvar: AccountReference,
    index: number | Expression,
    position: number | Expression,
  ): Expression => ({
    kind: 'instructionAccount',
    sysvar,
    index: u64Operand(index),
    position: u64Operand(position),
    field: 'key',
  }),
  /** Bit 0 is set when the account signs the instruction, bit 1 when it is writable. */
  instructionAccountFlags: (
    sysvar: AccountReference,
    index: number | Expression,
    position: number | Expression,
  ): Expression => ({
    kind: 'instructionAccount',
    sysvar,
    index: u64Operand(index),
    position: u64Operand(position),
    field: 'flags',
  }),
  instructionAccountIsSigner: instructionAccountFlag(1n),
  instructionAccountIsWritable: instructionAccountFlag(2n),
  instructionData: (
    sysvar: AccountReference,
    index: number | Expression,
    offset: number | Expression,
    type: ReadType,
  ): Expression => ({ kind: 'instructionData', sysvar, index: u64Operand(index), offset: u64Operand(offset), type }),
  instructionDataBytes: (
    sysvar: AccountReference,
    index: number | Expression,
    offset: number | Expression,
    length: number,
  ): Expression => ({
    kind: 'instructionDataBytes',
    sysvar,
    index: u64Operand(index),
    offset: u64Operand(offset),
    length,
  }),
  accountDataBytes: (accountReference: AccountReference, offset: number | Expression, length: number): Expression => ({
    kind: 'accountDataBytes',
    account: accountReference,
    offset: u64Operand(offset),
    length,
  }),
  bytesLength: (value: Expression): Expression => ({ kind: 'bytesLength', value }),
```

  A number turns into a `u64` literal at construction, so every node's index, position and offset
  is an `Expression`. The compiler hoists the literal and shares it like any other constant.
  `collectLiterals` and `collectInputNames` walk plain objects, so they find literals and inputs
  inside the new kinds unchanged.

- [ ] **Step 4: The addresses.** In `helpers.ts`, after `ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES`:

```ts
/**
 * `Sysvar1nstructions1111111111111111111111111`. Declare an account pinned to this address to read
 * the transaction's instructions; every introspection expression names that account.
 */
export const INSTRUCTIONS_SYSVAR_ADDRESS_BYTES = Uint8Array.of(
  6, 167, 213, 23, 24, 123, 209, 102, 53, 218, 212, 4, 85, 253, 194, 192, 193, 36, 198, 143, 33, 86, 117,
  165, 219, 186, 203, 95, 8, 0, 0, 0,
);
/** `Ed25519SigVerify111111111111111111111111111`, the Ed25519 signature-verification precompile. */
export const ED25519_PROGRAM_ADDRESS_BYTES = Uint8Array.of(
  3, 125, 70, 214, 124, 147, 251, 190, 18, 249, 66, 143, 131, 141, 64, 255, 5, 112, 116, 73, 39, 244, 138,
  100, 252, 202, 112, 68, 128, 0, 0, 0,
);
```

  Both are the base58 decodings of those names. The sysvar's bytes equal the Rust
  `INSTRUCTIONS_SYSVAR_ID`, which Task 3's Rust SDK test ties to `solana_program`.

- [ ] **Step 5: Extend the compiler** in `compiler.ts`.
  - Imports: add `import { INSTRUCTIONS_SYSVAR_ADDRESS_BYTES } from './helpers.js';` above the
    `./schema.js` import, and add `readWidth,` to that import (after `TemplateSchema,`).
    `helpers.ts` imports only `schema.ts`, so there is no cycle.
  - Delete the local `const readWidth: Record<ReadType, number> = { … };` block (172–182). The
    imported one has the same entries, so its three uses need no change.
  - In `compileExpression`, directly before `if (current.kind === 'select') {`:

```ts
    if (current.kind === 'instructionCount' || current.kind === 'currentInstructionIndex') {
      const sysvar = this.encodeSysvar(current.sysvar);
      const operation = current.kind === 'instructionCount' ? opcode.instructionCount : opcode.instructionIndex;
      return this.emit(operation, 'u64', 0, sysvar);
    }
    if (current.kind === 'instruction') {
      const sysvar = this.encodeSysvar(current.sysvar);
      const index = this.compileExpression(current.index, inLoop, bindings);
      requireType(index, 'u64', 'instruction index');
      const fields = {
        program: [opcode.instructionProgram, 'pubkey'],
        accountCount: [opcode.instructionAccountCount, 'u64'],
        dataLength: [opcode.instructionDataLength, 'u64'],
      } as const;
      const [operation, type] = fields[current.field];
      return this.emit(operation, type, 0, sysvar, index.register);
    }
    if (current.kind === 'instructionAccount') {
      const sysvar = this.encodeSysvar(current.sysvar);
      const index = this.compileExpression(current.index, inLoop, bindings);
      const position = this.compileExpression(current.position, inLoop, bindings);
      requireType(index, 'u64', 'instruction index');
      requireType(position, 'u64', 'instruction account position');
      return current.field === 'key'
        ? this.emit(opcode.instructionAccount, 'pubkey', 0, sysvar, index.register, position.register)
        : this.emit(opcode.instructionAccountFlags, 'u64', 0, sysvar, index.register, position.register);
    }
    if (current.kind === 'instructionData' || current.kind === 'instructionDataBytes') {
      const sysvar = this.encodeSysvar(current.sysvar);
      const index = this.compileExpression(current.index, inLoop, bindings);
      const offset = this.compileExpression(current.offset, inLoop, bindings);
      requireType(index, 'u64', 'instruction index');
      requireType(offset, 'u64', `${current.kind} offset`);
      if (current.kind === 'instructionData') {
        const type = readResultType[current.type];
        const selector = BigInt(readOpcode[current.type]);
        return this.emit(opcode.readInstructionData, type, 0, sysvar, index.register, offset.register, selector);
      }
      return this.emit(
        opcode.readInstructionBytes,
        'bytes',
        current.length,
        sysvar,
        index.register,
        offset.register,
        BigInt(current.length),
      );
    }
    if (current.kind === 'accountDataBytes') {
      const accountReference = this.encodeAccountReference(current.account, inLoop);
      const constraint = this.constraintFor(current.account, inLoop);
      this.requirePinnedForRead(current.account, constraint);
      if (constraint.writable) {
        throw new TypeError(
          `accountDataBytes reads only accounts the transaction cannot write; ${current.account.name} is declared writable`,
        );
      }
      const offset = this.compileExpression(current.offset, inLoop, bindings);
      requireType(offset, 'u64', 'accountDataBytes offset');
      return this.emit(
        opcode.readAccountBytes,
        'bytes',
        current.length,
        accountReference,
        offset.register,
        NO_INDEX,
        BigInt(current.length),
      );
    }
    if (current.kind === 'bytesLength') {
      const value = this.compileExpression(current.value, inLoop, bindings);
      requireType(value, 'bytes', 'bytesLength');
      return this.emit(opcode.bytesLength, 'u64', 0, value.register);
    }
```

  - Directly before the `requirePinnedForRead` doc comment:

```ts
  /** Introspection reads the Instructions sysvar through a fixed account pinned to its address. */
  encodeSysvar(reference: AccountReference): number {
    const constraint = reference.kind === 'account' ? this.constraintFor(reference, false) : undefined;
    if (!constraint?.address || !equalBytes(constraint.address, INSTRUCTIONS_SYSVAR_ADDRESS_BYTES)) {
      throw new TypeError(
        `Account ${reference.name} must be a fixed account pinned to the Instructions sysvar (INSTRUCTIONS_SYSVAR_ADDRESS_BYTES)`,
      );
    }
    return this.encodeAccountReference(reference, false);
  }
```

  The byte-read results carry `maxLength` equal to their length, so a CPI that forwards them
  declares exactly those bytes, the same bound the verifier computes.

- [ ] **Step 6: Run the checks.**

Run: `pnpm --dir clients/js check`
Expected: PASS. That covers `tsc` for the source and the examples, and every vitest file,
including "introspection expressions".

- [ ] **Step 7: Commit.**

```bash
git add clients/js/src/schema.ts clients/js/src/compiler.ts clients/js/src/helpers.ts clients/js/src/compiler.test.ts
git commit -m "Author the introspection and byte opcodes from TypeScript"
```

---

### Task 6: The Ed25519 signed-message helper

**Files:** `clients/js/src/helpers.ts` (its import, and the end of the file) and
`clients/js/src/compiler.test.ts`.

These are the Ed25519 precompile's facts the helper relies on. They were read from
`agave-precompiles` `src/ed25519.rs`, whose `verify` and `get_data_slice` are identical in 4.1.1
(what Mollusk 0.14 resolves) and 4.3.0. `solana-ed25519-program` 3.0.0 writes the same layout.
- The data is a `u8` signature count and a padding byte, then per signature seven little-endian
  `u16` fields, in this order: `signature_offset`, `signature_instruction_index`,
  `public_key_offset`, `public_key_instruction_index`, `message_data_offset`, `message_data_size`,
  `message_instruction_index`.
- The first signature's fields therefore sit at bytes 2, 4, 6, 8, 10, 12 and 14.
- An instruction index of `u16::MAX` means the precompile instruction's own data.
- A count of zero with data of two bytes or fewer verifies nothing, which is why the helper
  requires a count of exactly one.
- `new_ed25519_instruction_with_signature` puts the key at 16, the signature at 48 and the
  message at 112, with all three indexes `u16::MAX`. The helper reads the offsets rather than
  assuming that layout.
- The program ID is `Ed25519SigVerify111111111111111111111111111`.

- [ ] **Step 1: Write the failing tests.** In `compiler.test.ts`, add `ed25519Signature` to the
  import from `./index.js`. Next to `records`, add:

```ts
/** The blob's length, a little-endian u16 at byte 18 of the header; the blob ends the payload. */
function blobLength(compiled: CompiledTemplate): number {
  return compiled.bytes[18]! | (compiled.bytes[19]! << 8);
}
```

  Then append:

```ts
describe('ed25519Signature', () => {
  const sysvar = account.fixed('instructions');
  const quote = ed25519Signature({
    sysvar,
    index: expression.subtract(expression.currentInstructionIndex(sysvar), expression.u64(1)),
    signer: expression.accountField(account.fixed('maker'), 'key'),
    messageLength: 40,
    name: 'quote',
  });
  const compileQuote = (steps: Step[]) =>
    compileTemplate(
      defineTemplate({
        accounts: { instructions: { address: INSTRUCTIONS_SYSVAR_ADDRESS_BYTES }, maker: {} },
        steps,
      }),
    );

  test('its steps check the program, the header, and the key, and bind the index and the message', () => {
    const compiled = compileQuote([
      ...quote.steps,
      step.require(expression.greaterThan(quote.field(0, 'u64'), expression.u64(0)), 'pricePositive'),
    ]);
    const labels = new Set(compiled.sourceMap.map((entry) => entry.label));
    for (const label of ['quoteIsEd25519', 'quoteIsOneSelfContainedSignature', 'quoteIsBySigner']) {
      expect(labels.has(label), label).toBe(true);
    }
    // The index is computed once, and every read of the Ed25519 instruction reuses it.
    expect(records(compiled).filter((record) => record[0] === opcode.instructionIndex)).toHaveLength(1);
    // The header, the key offset and the key, the message offset, and the field.
    const reads = records(compiled).filter((record) => record[0] === opcode.readInstructionData);
    expect(reads.map((record) => readU64(record, 6))).toEqual(
      [opcode.readU128, opcode.readU16, opcode.readPubkey, opcode.readU16, opcode.readU64].map(BigInt),
    );
  });

  test('the header check masks the count, the three instruction indexes and the message size', () => {
    const compiled = compileQuote(quote.steps);
    // The mask and the expected value are the two u128 constants, loaded from the blob.
    const blob = compiled.bytes.slice(compiled.bytes.length - blobLength(compiled));
    const constants = records(compiled)
      .filter((record) => record[0] === opcode.constU128)
      .map((record) => {
        const offset = Number(readU64(record, 6) & 0xffff_ffffn);
        return blob.slice(offset, offset + 16).reduceRight((value, byte) => (value << 8n) | BigInt(byte), 0n);
      });
    const field = (offset: number, width: number, value: bigint) =>
      [((1n << BigInt(width * 8)) - 1n) << BigInt(offset * 8), value << BigInt(offset * 8)] as const;
    const parts = [field(0, 1, 1n), field(4, 2, 0xffffn), field(8, 2, 0xffffn), field(12, 2, 40n), field(14, 2, 0xffffn)];
    expect(constants).toEqual([
      parts.reduce((mask, [part]) => mask | part, 0n),
      parts.reduce((expected, [, part]) => expected | part, 0n),
    ]);
  });

  test('fields must lie inside the signed message', () => {
    expect(() => quote.field(32, 'u64')).not.toThrow();
    expect(() => quote.field(33, 'u64')).toThrow(/inside the 40-byte signed message/);
    expect(() => quote.field(9, 'pubkey')).toThrow(/inside/);
    expect(() => quote.field(-1, 'u8')).toThrow(/inside/);
  });

  test('a field read without the steps does not compile', () => {
    expect(() => compileQuote([step.require(expression.greaterThan(quote.field(0, 'u64'), expression.u64(0)))])).toThrow(
      /Unknown variable: quote/,
    );
  });
});
```

- [ ] **Step 2: Run them and confirm they fail.**

Run: `pnpm --dir clients/js exec vitest run src/compiler.test.ts -t ed25519Signature`
Expected: FAIL, because `ed25519Signature` is not exported.

- [ ] **Step 3: Implement the helper.** In `helpers.ts`, replace the first line's import with:

```ts
import {
  data,
  expression,
  readWidth,
  step,
  type AccountReference,
  type Expression,
  type ReadType,
  type Step,
} from './schema.js';
```

  Then append to the end of the file:

```ts
/**
 * The first 16 bytes of an Ed25519 precompile instruction with one signature, from agave
 * `precompiles/src/ed25519.rs`: a `u8` signature count and a padding byte, then the signature's
 * seven little-endian `u16` offsets. Byte offsets.
 */
const ED25519_HEADER = {
  signatureCount: 0,
  signatureInstructionIndex: 4,
  publicKeyOffset: 6,
  publicKeyInstructionIndex: 8,
  messageDataOffset: 10,
  messageDataSize: 12,
  messageInstructionIndex: 14,
} as const;

/** An instruction-index field of `u16::MAX`: the Ed25519 instruction's own data. */
const ED25519_THIS_INSTRUCTION = 0xffffn;

/**
 * A mask and the value the masked header must equal, from `[byte offset, width, value]` fields of
 * a little-endian header.
 */
function headerMask(fields: readonly (readonly [offset: number, width: number, value: bigint])[]) {
  let mask = 0n;
  let expected = 0n;
  for (const [offset, width, value] of fields) {
    const shift = BigInt(offset * 8);
    mask |= ((1n << BigInt(width * 8)) - 1n) << shift;
    expected |= value << shift;
  }
  return { mask, expected };
}

export interface Ed25519Signature {
  /**
   * Requirements that bind the precompile's verified signature to `signer` and to a message of
   * `messageLength` bytes, and the bindings `field` reads through. Put them before any step that
   * uses `field`: without them `field` would read unverified bytes, so it does not compile.
   */
  steps: Step[];
  /** The value at `offset` in the signed message. The whole read must lie inside the message. */
  field(offset: number, type: ReadType): Expression;
}

/**
 * Binds an Ed25519 signature that the transaction's precompile instruction verified to this
 * template's inputs.
 *
 * The Ed25519 program verifies its signatures as part of the transaction, so a transaction whose
 * signature is invalid fails and nothing the template did survives. What the precompile does not
 * say is whose signature it checked, or over which bytes. The steps returned here require that
 * instruction `index` is the Ed25519 program, holds exactly one signature, takes the signature,
 * the key and the message from its own data, was signed by `signer`, and signed exactly
 * `messageLength` bytes. `field` then reads the signed message.
 *
 * The count, the three instruction indexes and the message size all sit in the instruction's
 * first 16 bytes, so one masked `u128` comparison checks them together: five separate
 * comparisons would take a dozen more of the template's 64 registers.
 *
 * `name` prefixes the step labels and the two variables the steps bind, `<name>Instruction` and
 * `<name>Message`, so one template can check more than one signature.
 */
export function ed25519Signature(input: {
  sysvar: AccountReference;
  /** The Ed25519 instruction's index in the transaction, as a `u64`. */
  index: Expression;
  /** The public key the signature must be by, as a `pubkey`. */
  signer: Expression;
  messageLength: number;
  name?: string;
}): Ed25519Signature {
  const name = input.name ?? 'signature';
  if (!Number.isInteger(input.messageLength) || input.messageLength < 1 || input.messageLength > 0xffff) {
    throw new RangeError('messageLength must be from 1 to 65535 bytes');
  }
  const instruction = expression.variable(`${name}Instruction`);
  const message = expression.variable(`${name}Message`);
  const offsetField = (offset: number) => expression.instructionData(input.sysvar, instruction, offset, 'u16');
  const header = headerMask([
    [ED25519_HEADER.signatureCount, 1, 1n],
    [ED25519_HEADER.signatureInstructionIndex, 2, ED25519_THIS_INSTRUCTION],
    [ED25519_HEADER.publicKeyInstructionIndex, 2, ED25519_THIS_INSTRUCTION],
    [ED25519_HEADER.messageDataSize, 2, BigInt(input.messageLength)],
    [ED25519_HEADER.messageInstructionIndex, 2, ED25519_THIS_INSTRUCTION],
  ]);
  return {
    steps: [
      step.let(`${name}Instruction`, input.index),
      step.require(
        expression.equal(
          expression.instructionProgram(input.sysvar, instruction),
          expression.pubkey(ED25519_PROGRAM_ADDRESS_BYTES),
        ),
        `${name}IsEd25519`,
      ),
      step.require(
        expression.equal(
          expression.bitAnd(expression.instructionData(input.sysvar, instruction, 0, 'u128'), expression.u128(header.mask)),
          expression.u128(header.expected),
        ),
        `${name}IsOneSelfContainedSignature`,
      ),
      step.require(
        expression.equal(
          expression.instructionData(input.sysvar, instruction, offsetField(ED25519_HEADER.publicKeyOffset), 'pubkey'),
          input.signer,
        ),
        `${name}IsBySigner`,
      ),
      step.let(`${name}Message`, offsetField(ED25519_HEADER.messageDataOffset)),
    ],
    field(offset, type) {
      if (!Number.isInteger(offset) || offset < 0 || offset + readWidth[type] > input.messageLength) {
        throw new RangeError(`${type} at ${offset} does not lie inside the ${input.messageLength}-byte signed message`);
      }
      const at = offset === 0 ? message : expression.add(message, expression.u64(offset));
      return expression.instructionData(input.sysvar, instruction, at, type);
    },
  };
}
```

Why the checks are enough:
- The header comparison requires a count of one and self-referencing indexes for the signature,
  the key and the message. The precompile therefore verified the signature over the message bytes
  at `message_data_offset`, with the key at `public_key_offset`, all in this instruction.
- `IsBySigner` compares the key at that offset with `signer`.
- `field` reads only inside `[message_data_offset, message_data_offset + messageLength)`, which the
  header pins to exactly the signed bytes. A read that would reach past them does not compile.

The signature's own offset needs no check: the precompile verified whatever bytes it names.

- [ ] **Step 4: Run the tests.**

Run: `pnpm --dir clients/js check`
Expected: PASS, including the four `ed25519Signature` tests.

- [ ] **Step 5: Commit.**

```bash
git add clients/js/src/helpers.ts clients/js/src/compiler.test.ts
git commit -m "Bind a precompile-verified Ed25519 signature to a template with ed25519Signature"
```

---

### Task 7: The introspection fixture

**Files:**
- `clients/js/src/fixtures.test.ts`: the imports, before `systemPrograms`, and the `fixtures` map
  before `'pinned-mint-read'`.
- `common/src/template/verify.rs`: `every_shared_fixture_parses_and_verifies` (2421–2478).
- Generated: `fixtures/introspection.hex`, `fixtures/manifest.json`.

The fixture is the middle of three instructions in Task 9's transaction, between a memo "before"
signed by `memoSigner` and a memo "after" with no accounts. Its inputs point it at the first memo,
so Task 9 can move them to reach each failure.

- [ ] **Step 1: Add the fixture.** In `fixtures.test.ts`, add `INSTRUCTIONS_SYSVAR_ADDRESS_BYTES`
  to the import from `./index.js`. Before `const systemPrograms = {`:

```ts
/** `MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr`, the SPL Memo program the Mollusk suite loads. */
const MEMO_PROGRAM_ADDRESS_BYTES = Uint8Array.of(
  5, 74, 83, 90, 153, 41, 33, 6, 77, 36, 232, 113, 96, 218, 56, 124, 124, 53, 181, 221, 188, 146, 187, 129,
  228, 31, 168, 64, 65, 5, 68, 141,
);

```

  In `fixtures`, before `'pinned-mint-read': () =>`:

```ts
  // Run by the Mollusk suite as the second of three instructions, between two memos: "before",
  // signed by `memoSigner`, and "after", with no accounts. The inputs point it at the first memo.
  introspection: () => {
    const sysvar = account.fixed('instructions');
    const neighbour = expression.input('neighbour');
    const position = expression.input('position');
    return defineTemplate({
      inputs: { neighbour: { type: 'u64' }, position: { type: 'u64' }, dataOffset: { type: 'u64' } },
      accounts: {
        instructions: { address: INSTRUCTIONS_SYSVAR_ADDRESS_BYTES },
        memoSigner: { signer: true },
        mint: { owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 82 },
      },
      steps: [
        step.require(expression.equal(expression.instructionCount(sysvar), expression.u64(3)), 'threeInstructions'),
        step.require(expression.equal(expression.currentInstructionIndex(sysvar), expression.u64(1)), 'runsSecond'),
        step.require(
          expression.equal(expression.instructionProgram(sysvar, neighbour), expression.pubkey(MEMO_PROGRAM_ADDRESS_BYTES)),
          'neighbourIsMemo',
        ),
        step.require(expression.equal(expression.instructionAccountCount(sysvar, neighbour), expression.u64(1)), 'oneAccount'),
        step.require(
          expression.equal(
            expression.instructionAccount(sysvar, neighbour, position),
            expression.accountField(account.fixed('memoSigner'), 'key'),
          ),
          'memoNamesItsSigner',
        ),
        step.require(expression.instructionAccountIsSigner(sysvar, neighbour, position), 'memoIsSigned'),
        step.require(expression.not(expression.instructionAccountIsWritable(sysvar, neighbour, position)), 'signerIsReadOnly'),
        step.require(expression.equal(expression.instructionDataLength(sysvar, neighbour), expression.u64(6)), 'memoIsSixBytes'),
        // "before" starts 62 65 66 6f: "befo" as a little-endian u32.
        step.require(
          expression.equal(expression.instructionData(sysvar, neighbour, 0, 'u32'), expression.u64(0x6f66_6562)),
          'memoSaysBefore',
        ),
        step.let('after', expression.instructionDataBytes(sysvar, 2, 0, 5)),
        step.require(
          expression.equal(expression.variable('after'), expression.bytes(new TextEncoder().encode('after'))),
          'lastMemoSaysAfter',
        ),
        step.require(expression.equal(expression.bytesLength(expression.variable('after')), expression.u64(5)), 'fiveBytes'),
        step.require(
          expression.equal(
            expression.accountDataBytes(account.fixed('mint'), expression.input('dataOffset'), 1),
            expression.bytes(Uint8Array.of(6)),
          ),
          'mintHasSixDecimals',
        ),
      ],
    });
  },

```

  Every opcode from 64 to 74 appears:
  - the flags twice, through `instructionAccountIsSigner` and `instructionAccountIsWritable`;
  - `BYTES_LEN` once;
  - `READ_ACCOUNT_BYTES` on the mint, at run-time offset `dataOffset` (44, the decimals).

- [ ] **Step 2: Generate it.**

Run: `pnpm fixtures`
Expected: `fixtures/introspection.hex` is created and `fixtures/manifest.json` gains its entry,
source map included; Task 9 reads labels from it. No other fixture changes.

- [ ] **Step 3: Verify it in Rust.** In `every_shared_fixture_parses_and_verifies`, change
  `[(&str, &str); 15]` to `[(&str, &str); 16]` and add, after the `math-ops` line:

```rust
            ("introspection", include_str!("../../../fixtures/introspection.hex")),
```

Run: `cargo test -p ballista-common every_shared_fixture_parses_and_verifies`
Expected: PASS. The TypeScript compiler and the Rust verifier agree on every new opcode's
operands.

- [ ] **Step 4: Commit.**

```bash
git add clients/js/src/fixtures.test.ts fixtures common/src/template/verify.rs
git commit -m "Add an introspection fixture that reads its neighbours in the transaction"
```

---

### Task 8: The signed-quote settlement example

**Files:**
- Create: `clients/js/examples/protocols/signed-quote-settlement.ts`.
- Modify: `clients/js/examples/protocols/shared.ts`, after `TOKEN_ACCOUNT_MINT_OFFSET`.
- Modify: `clients/js/examples/protocols/index.ts`.
- Modify: `clients/js/src/protocol-examples.test.ts` (26).
- Modify: `clients/js/src/protocol-semantics.test.ts`: the imports, `dependsOn`, and a new
  `describe`.
- Modify: `clients/js/src/fixtures.test.ts`: import the example and add it as a fixture.
- Modify: `common/src/template/verify.rs`: the fixture list and the protocol count.
- Generated: `fixtures/protocol-examples.json`, `fixtures/signed-quote-settlement.hex`,
  `fixtures/manifest.json`.

**The shape.**
- A maker signs a 120-byte quote: a price, the most it will sell, an expiry, the one taker the
  quote is for, and the two mints.
- The taker puts the Ed25519 instruction carrying the signature directly before this template's
  run.
- The template binds the signature to the maker, then holds the fill to the quote: before the
  expiry, for this taker, no more than the maximum, in the signed mints, paid into an account the
  maker owns, and at the signed price rounded up.
- Two `transfer`s settle it: the taker pays, the maker delivers.
- The maker's authority co-signs, because Ballista never controls the maker's tokens. The template
  is what lets that co-signer sign without checking the terms.

It has a docs page to come; the docs are out of scope here.

- [ ] **Step 1: Write the failing tests.** In `protocol-examples.test.ts`, change
  `expect(entries.length).toBe(12);` to `toBe(13)`. In `protocol-semantics.test.ts`:
  - Add `import { isDeepStrictEqual } from 'node:util';` and a blank line above the
    `@solana/kit` import.
  - Change the `./index.js` import to:

```ts
import {
  ED25519_PROGRAM_ADDRESS_BYTES,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  expression,
  type AccountReference,
  type Expression,
  type ReadType,
  type Step,
  type Template,
} from './index.js';
```

  - Add `signedQuoteSettlement,` after `pythFreshPriceGate,` in the import from
    `../examples/protocols/index.js`, and below that import add:

```ts
import { QUOTE, signedQuote } from '../examples/protocols/signed-quote-settlement.js';
```

  - Add `TOKEN_ACCOUNT_OWNER_OFFSET,` after `TOKEN_ACCOUNT_MINT_OFFSET,` in the import from
    `../examples/protocols/shared.js`.
  - In `dependsOn`, after the `case 'powerOfTen':` return, add:

```ts
    case 'instruction':
      return recurse(expression.index);
    case 'instructionAccount':
      return recurse(expression.index) || recurse(expression.position);
    case 'instructionData':
    case 'instructionDataBytes':
      return recurse(expression.index) || recurse(expression.offset);
    case 'accountDataBytes':
      return recurse(expression.offset);
    case 'bytesLength':
      return recurse(expression.value);
```

  - Append:

```ts
describe('the signed-quote settlement', () => {
  const bindings = bindingsOf(signedQuoteSettlement);
  const is = (target: Expression) => (candidate: Expression) => isDeepStrictEqual(candidate, target);
  const signed = (offset: number, type: ReadType) => is(signedQuote.field(offset, type));
  const requirement = (label: string) => requireLabeled(signedQuoteSettlement, label).condition;
  const [takerPays, makerDelivers] = invokesOf(signedQuoteSettlement, 'tokenProgram') as [Invoke, Invoke];
  const amountOf = (call: Invoke) => {
    const part = call.data[1];
    return part?.kind === 'encoded' && part.encoding === 'u64' ? part.value : undefined;
  };

  test('takes the quote from the Ed25519 program, signed by the maker', () => {
    expect(
      dependsOn(requirement('quoteIsEd25519'), bindings, is(expression.pubkey(ED25519_PROGRAM_ADDRESS_BYTES))),
    ).toBe(true);
    expect(dependsOn(requirement('quoteIsBySigner'), bindings, accountKey('maker'))).toBe(true);
  });

  test('the taker pays the signed price for what it takes', () => {
    expect(takerPays.label).toBe('takerPays');
    expect(takerPays.accounts.map((entry) => nameOf(entry.account))).toEqual([
      'takerQuoteAccount',
      'makerQuoteAccount',
      'taker',
    ]);
    const payment = amountOf(takerPays)!;
    expect(dependsOn(payment, bindings, signed(QUOTE.price, 'u64'))).toBe(true);
    expect(dependsOn(payment, bindings, is(expression.input('amount')))).toBe(true);
  });

  test('the maker delivers what the taker takes, within the signed size, to the signed taker, before expiry', () => {
    expect(makerDelivers.label).toBe('makerDelivers');
    expect(makerDelivers.accounts.map((entry) => nameOf(entry.account))).toEqual([
      'makerBaseAccount',
      'takerBaseAccount',
      'maker',
    ]);
    expect(amountOf(makerDelivers)).toEqual(expression.input('amount'));
    expect(dependsOn(requirement('withinTheQuotedSize'), bindings, signed(QUOTE.maxAmount, 'u64'))).toBe(true);
    expect(dependsOn(requirement('quoteIsForThisTaker'), bindings, signed(QUOTE.taker, 'pubkey'))).toBe(true);
    expect(dependsOn(requirement('quoteIsForThisTaker'), bindings, accountKey('taker'))).toBe(true);
    expect(dependsOn(requirement('quoteHasNotExpired'), bindings, signed(QUOTE.expiry, 'i64'))).toBe(true);
    expect(dependsOn(requirement('quoteHasNotExpired'), bindings, is(expression.clockUnixTimestamp()))).toBe(true);
  });

  test('settles only in the signed mints, into an account the maker owns', () => {
    const paysIn = requirement('paysInTheQuotedMint');
    expect(dependsOn(paysIn, bindings, signed(QUOTE.quoteMint, 'pubkey'))).toBe(true);
    expect(dependsOn(paysIn, bindings, reads('takerQuoteAccount', TOKEN_ACCOUNT_MINT_OFFSET))).toBe(true);
    const delivers = requirement('deliversTheQuotedMint');
    expect(dependsOn(delivers, bindings, signed(QUOTE.baseMint, 'pubkey'))).toBe(true);
    expect(dependsOn(delivers, bindings, reads('makerBaseAccount', TOKEN_ACCOUNT_MINT_OFFSET))).toBe(true);
    expect(
      dependsOn(requirement('paymentReachesTheMaker'), bindings, reads('makerQuoteAccount', TOKEN_ACCOUNT_OWNER_OFFSET)),
    ).toBe(true);
  });

  test('every field lies inside the signed message', () => {
    expect(() => signedQuote.field(QUOTE.quoteMint + 1, 'pubkey')).toThrow(/inside/);
    expect(() => signedQuote.field(QUOTE.length - 7, 'u64')).toThrow(/inside/);
  });
});
```

  `bindingsOf`, `requireLabeled`, `invokesOf`, `reads`, `accountKey`, `nameOf` and `Invoke` are
  already in the file. `isDeepStrictEqual` compares `bigint`s and `Uint8Array`s by value, so a
  field read matches only the read `signedQuote.field` builds.

- [ ] **Step 2: Run them and confirm they fail.**

Run: `pnpm --dir clients/js exec vitest run src/protocol-semantics.test.ts src/protocol-examples.test.ts`
Expected: FAIL, because `../examples/protocols/signed-quote-settlement.js` does not exist.

- [ ] **Step 3: The owner offset.** In `shared.ts`, after `TOKEN_ACCOUNT_MINT_OFFSET`:

```ts

/** SPL Token account: the owner, whose signature moves its tokens, follows the mint. */
export const TOKEN_ACCOUNT_OWNER_OFFSET = 32;
```

- [ ] **Step 4: The example.** Create
  `clients/js/examples/protocols/signed-quote-settlement.ts`:

```ts
/**
 * Settle a maker's signed quote: the taker pays the quoted price, the maker delivers, and neither
 * side can stretch the quote past what the maker signed.
 *
 * The maker signs a 120-byte quote off chain: a price, the most it will sell, an expiry, the one
 * taker the quote is for, and the two mints. The taker puts the Ed25519 precompile instruction
 * that carries the signature directly before this template's run. The precompile verifies the
 * signature as part of the transaction, so a bad one fails it. The template checks that the
 * signature is the maker's, over a quote of this shape, and then holds the fill to the quote:
 * before the expiry, for this taker, no more than the maximum, in these mints, paid into an
 * account the maker owns, at the signed price rounded up in the maker's favour.
 *
 * Ballista does not control the maker's tokens: the maker's authority co-signs the transaction.
 * Because the template enforces the terms, the service that co-signs checks only that the
 * transaction runs this template and nothing else that could spend the maker's accounts. Ballista
 * keeps no state, so it cannot count fills. A quote can be settled again until it expires unless
 * the co-signer refuses a second settlement of the same quote.
 */
import {
  INSTRUCTIONS_SYSVAR_ADDRESS_BYTES,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  defineTemplate,
  ed25519Signature,
  expression,
  step,
  tokenTransfer,
} from '../../src/index.js';
import { TOKEN_ACCOUNT_LENGTH, TOKEN_ACCOUNT_MINT_OFFSET, TOKEN_ACCOUNT_OWNER_OFFSET } from './shared.js';

/** The signed quote. Integers are little-endian; keys are their 32 raw bytes. */
export const QUOTE = {
  length: 120,
  /** Quote-token base units per `PRICE_SCALE` base-token base units. */
  price: 0,
  /** The most base-token base units the maker delivers. */
  maxAmount: 8,
  /** The last Unix timestamp at which the quote can settle. */
  expiry: 16,
  /** The one wallet that can take the quote. */
  taker: 24,
  /** The mint the maker delivers. */
  baseMint: 56,
  /** The mint the taker pays in. */
  quoteMint: 88,
} as const;

/** Prices carry six decimals: a price of 1,000,000 is one quote unit per base unit. */
export const PRICE_SCALE = 1_000_000n;

const instructions = account.fixed('instructions');
const taker = account.fixed('taker');
const maker = account.fixed('maker');

/** The maker's signature, in the instruction directly before this template's run. */
export const signedQuote = ed25519Signature({
  sysvar: instructions,
  index: expression.subtract(expression.currentInstructionIndex(instructions), expression.u64(1)),
  signer: expression.accountField(maker, 'key'),
  messageLength: QUOTE.length,
  name: 'quote',
});

const tokenAccount = { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: TOKEN_ACCOUNT_LENGTH };

export const signedQuoteSettlement = defineTemplate({
  inputs: {
    /** Base-token base units to take, up to the quoted maximum. */
    amount: { type: 'u64' },
  },
  accounts: {
    instructions: { address: INSTRUCTIONS_SYSVAR_ADDRESS_BYTES },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    taker: { signer: true },
    maker: { signer: true },
    /** Pays, in the quote mint. */
    takerQuoteAccount: tokenAccount,
    /** Is paid, in the quote mint. */
    makerQuoteAccount: tokenAccount,
    /** Delivers, in the base mint. */
    makerBaseAccount: tokenAccount,
    /** Receives, in the base mint. */
    takerBaseAccount: tokenAccount,
  },
  steps: [
    ...signedQuote.steps,

    step.require(
      expression.lessThanOrEqual(expression.clockUnixTimestamp(), signedQuote.field(QUOTE.expiry, 'i64')),
      'quoteHasNotExpired',
    ),
    step.require(
      expression.equal(signedQuote.field(QUOTE.taker, 'pubkey'), expression.accountField(taker, 'key')),
      'quoteIsForThisTaker',
    ),
    step.require(
      expression.lessThanOrEqual(expression.input('amount'), signedQuote.field(QUOTE.maxAmount, 'u64')),
      'withinTheQuotedSize',
    ),

    // A token `transfer` moves only between two accounts of one mint, so pinning one side of each
    // leg pins both.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('takerQuoteAccount'), TOKEN_ACCOUNT_MINT_OFFSET, 'pubkey'),
        signedQuote.field(QUOTE.quoteMint, 'pubkey'),
      ),
      'paysInTheQuotedMint',
    ),
    step.require(
      expression.equal(
        expression.accountData(account.fixed('makerBaseAccount'), TOKEN_ACCOUNT_MINT_OFFSET, 'pubkey'),
        signedQuote.field(QUOTE.baseMint, 'pubkey'),
      ),
      'deliversTheQuotedMint',
    ),
    // The payment reaches an account the maker owns, not one the taker picked.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('makerQuoteAccount'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountField(maker, 'key'),
      ),
      'paymentReachesTheMaker',
    ),

    step.let(
      'payment',
      expression.multiplyDivide(
        expression.input('amount'),
        signedQuote.field(QUOTE.price, 'u64'),
        expression.u64(PRICE_SCALE),
        'up',
      ),
      'priceTheFill',
    ),
    tokenTransfer({
      tokenProgram: account.fixed('tokenProgram'),
      source: account.fixed('takerQuoteAccount'),
      destination: account.fixed('makerQuoteAccount'),
      authority: taker,
      amount: expression.variable('payment'),
      label: 'takerPays',
    }),
    tokenTransfer({
      tokenProgram: account.fixed('tokenProgram'),
      source: account.fixed('makerBaseAccount'),
      destination: account.fixed('takerBaseAccount'),
      authority: maker,
      amount: expression.input('amount'),
      label: 'makerDelivers',
    }),
  ],
});

export const compiled = compileTemplate(signedQuoteSettlement);
```

  It compiles to 61 instructions and 50 registers of the 64 allowed. With five separate header
  comparisons in the helper, it took 60.

- [ ] **Step 5: Export it.** In `examples/protocols/index.ts`, after the `pythFreshPriceGate` line:

```ts
export { signedQuoteSettlement } from './signed-quote-settlement.js';
```

- [ ] **Step 6: Make it a fixture too**, so the Mollusk suite can run it and read its source map.
  In `fixtures.test.ts`, add at the top of the imports:

```ts
import { signedQuoteSettlement } from '../examples/protocols/signed-quote-settlement.js';
```

  In `fixtures`, after the `introspection` entry:

```ts
  'signed-quote-settlement': () => signedQuoteSettlement,

```

- [ ] **Step 7: Regenerate, run the tests, and verify.** Regenerate first:
  `protocol-examples.test.ts` compares the compiled payloads with the recorded file.

```bash
pnpm fixtures
pnpm --dir clients/js exec vitest run src/protocol-semantics.test.ts src/protocol-examples.test.ts
```

Expected:
- `fixtures/protocol-examples.json` gains `signedQuoteSettlement`.
- `fixtures/signed-quote-settlement.hex` is created, and the manifest gains its entry.
- The vitest run passes.

  In `every_shared_fixture_parses_and_verifies`:
  - change `[(&str, &str); 16]` to `[(&str, &str); 17]`;
  - add after the `introspection` line:

```rust
            (
                "signed-quote-settlement",
                include_str!("../../../fixtures/signed-quote-settlement.hex"),
            ),
```

  - change `assert_eq!(examples, 12, "every protocol example is verified");` to `13`.

```bash
cargo test -p ballista-common every_shared_fixture_parses_and_verifies
pnpm --dir clients/js check
```

Expected: PASS.

- [ ] **Step 8: Commit.**

```bash
git add clients/js/examples/protocols clients/js/src/protocol-semantics.test.ts \
  clients/js/src/protocol-examples.test.ts clients/js/src/fixtures.test.ts fixtures common/src/template/verify.rs
git commit -m "Settle a maker's Ed25519-signed quote with token transfers"
```

---

### Task 9: End to end under Mollusk

**Files:** `tests/ballista/Cargo.toml`, `tests/ballista/Cargo.lock`, `tests/ballista/src/lib.rs`.

**The harness.**
- `MolluskContext::process_transaction_instructions` runs several instructions as one
  transaction.
- Mollusk builds the Instructions sysvar from the whole transaction whenever the account list does
  not supply it; `MolluskContext` never loads that account from its store.
- The transaction context rewrites the sysvar's current index as each top-level instruction
  starts, so `INSTRUCTION_INDEX` sees the run's real position.
- Pass the sysvar as `AccountMeta::new_readonly(sysvar::instructions::id(), false)` and do not
  insert an account for it.

**The results.**
- A transaction returns `TransactionResult`, whose `program_result` is a `TransactionProgramResult`:
  `Success`, `Failure(index, ProgramError)` or `UnknownError(index, InstructionError)`.
- `custom_code` and `decode_kind` read an `InstructionResult`, so this task adds a
  transaction-side reader. It also reads the failing program counter's step label from the
  manifest `pnpm fixtures` writes.

**The precompile.** Mollusk verifies Ed25519 only with its `precompiles` feature.
- Without it, the precompile instruction fails whatever its signature.
- With it, `agave-precompiles` verifies the instruction in transaction order.
- Signing uses `ed25519-dalek` 1.0.1, the version `agave-precompiles` 4.1.1 depends on, so no new
  crate enters the graph beyond what the feature brings.

- [ ] **Step 1: Dependencies.** In `tests/ballista/Cargo.toml`:
  - change the Mollusk line to
    `mollusk-svm = { version = "=0.14.0", features = ["inner-instructions", "precompiles"] }`;
  - add `ed25519-dalek = "=1.0.1"` before `solana-account`;
  - add `solana-ed25519-program = "=3.0.0"` before `solana-fee-calculator`.

- [ ] **Step 2: Resolve.**

Run: `cargo tree --manifest-path tests/ballista/Cargo.toml -i agave-precompiles | head -3`
Expected: `agave-precompiles v4.1.1`. Cargo adds about 30 packages to `tests/ballista/Cargo.lock`,
among them `agave-feature-set` 4.1.1, `openssl`, and the secp256k1 and secp256r1 programs.

If it fails to select a version, the only candidate it could see was 4.3.0: this happens offline
when 4.1.1 is not in the local registry. 4.3.0 requires `agave-feature-set` 4.3.0, which conflicts
with the locked 4.1.1 crates. Run it online, or pin it:

```bash
cargo update --manifest-path tests/ballista/Cargo.toml -p agave-precompiles --precise 4.1.1
```

`agave-precompiles` links OpenSSL through `openssl-sys`, without the vendored feature.
- On macOS, Homebrew's `openssl@3` is found automatically.
- GitHub's `ubuntu-latest` image ships `libssl-dev`. If the SBF job ever fails building
  `openssl-sys`, add `sudo apt-get install -y libssl-dev pkg-config` before its
  `cargo test --manifest-path tests/ballista/Cargo.toml` step in `.github/workflows/ci.yml`.

- [ ] **Step 3: The shared helpers.** In `tests/ballista/src/lib.rs`, inside `mod tests`:
  - Add `use mollusk_svm::result::types::{TransactionProgramResult, TransactionResult};` above the
    `use mollusk_svm::{…}` line.
  - Change `use solana_sdk_ids::system_program;` to `use solana_sdk_ids::{system_program, sysvar};`.
  - In `fixture()`, after the `"math-ops"` arm, add:

```rust
            "introspection" => include_str!("../../../fixtures/introspection.hex"),
            "signed-quote-settlement" => {
                include_str!("../../../fixtures/signed-quote-settlement.hex")
            }
```

  - Above `decode_kind`, add:

```rust
    /// The instruction a transaction failed at and its custom error code, if it failed with one.
    fn transaction_code(result: &TransactionResult) -> Option<(usize, u32)> {
        match &result.program_result {
            TransactionProgramResult::Failure(
                index,
                solana_program_error::ProgramError::Custom(code),
            ) => Some((*index, *code)),
            _ => None,
        }
    }

    /// The instruction a transaction failed at, whatever it failed with.
    fn failed_instruction(result: &TransactionResult) -> Option<usize> {
        match &result.program_result {
            TransactionProgramResult::Success => None,
            TransactionProgramResult::Failure(index, _)
            | TransactionProgramResult::UnknownError(index, _) => Some(*index),
        }
    }

    /// Where a transaction failed inside a compiler fixture's run: the instruction, the error kind,
    /// and the label of the step that emitted the failing program counter, from the source map
    /// `pnpm fixtures` records.
    fn fixture_failure(result: &TransactionResult, name: &str) -> Option<(usize, u32, String)> {
        let (index, code) = transaction_code(result)?;
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../fixtures/manifest.json")).expect("manifest");
        let label = manifest[name]["sourceMap"]
            .as_array()?
            .iter()
            .find(|entry| entry["pc"] == code >> 16)?["label"]
            .as_str()?
            .to_owned();
        Some((index, code & 0xffff, label))
    }
```

- [ ] **Step 4: The introspection test.** Add, after
  `typescript_math_fixture_computes_exact_results`:

```rust
    /// The introspection opcodes as the TypeScript SDK compiles them, run as the middle of three
    /// instructions, between two memos, against the Instructions sysvar Mollusk builds from the
    /// whole transaction.
    #[test]
    fn typescript_introspection_fixture_reads_the_transaction() {
        let creator = Pubkey::new_unique();
        let signer = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let mut accounts = funded_accounts([creator, signer], 10_000_000_000);
        accounts.insert(
            mint,
            token::create_account_for_mint(Mint {
                mint_authority: COption::None,
                supply: 0,
                decimals: 6,
                is_initialized: true,
                freeze_authority: COption::None,
            }),
        );
        let context = context(accounts);
        let payload = fixture("introspection");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 91, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 91);
        let before = Instruction {
            program_id: memo::ID,
            accounts: vec![AccountMeta::new_readonly(signer, true)],
            data: b"before".to_vec(),
        };
        let after = Instruction {
            program_id: memo::ID,
            accounts: vec![],
            data: b"after".to_vec(),
        };
        let run = |neighbour: u64, position: u64, data_offset: u64, writable_mint: bool| {
            let mut inputs = Vec::new();
            for value in [neighbour, position, data_offset] {
                inputs.extend_from_slice(&value.to_le_bytes());
            }
            let mint_meta = if writable_mint {
                AccountMeta::new(mint, false)
            } else {
                AccountMeta::new_readonly(mint, false)
            };
            let ballista = run_instruction(
                template,
                vec![
                    AccountMeta::new_readonly(sysvar::instructions::id(), false),
                    AccountMeta::new_readonly(signer, true),
                    mint_meta,
                ],
                &inputs,
            );
            context.process_transaction_instructions(&[before.clone(), ballista, after.clone()])
        };

        let read = run(0, 0, 44, false);
        assert!(read.program_result.is_ok(), "{read:#?}");
        let memos = context.process_transaction_instructions(&[before.clone(), after.clone()]);
        eprintln!(
            "introspection fixture compute units: {}",
            read.compute_units_consumed - memos.compute_units_consumed
        );

        // Every failure is the run's, instruction 1, at the step whose read is out of range.
        let failure = |label: &str, kind: u32| Some((1, kind, label.to_owned()));
        assert_eq!(
            fixture_failure(&run(3, 0, 44, false), "introspection"),
            failure("neighbourIsMemo", 6023),
            "there is no fourth instruction"
        );
        assert_eq!(
            fixture_failure(&run(0, 1, 44, false), "introspection"),
            failure("memoNamesItsSigner", 6023),
            "the memo names one account"
        );
        assert_eq!(
            fixture_failure(&run(0, 0, 82, false), "introspection"),
            failure("mintHasSixDecimals", 6023),
            "a mint is 82 bytes"
        );
        assert_eq!(
            fixture_failure(&run(0, 0, 44, true), "introspection"),
            failure("mintHasSixDecimals", 6024),
            "a writable account's bytes are not lent"
        );
        // The last memo exists but names no accounts, so the checks on it fail as requirements.
        assert_eq!(
            fixture_failure(&run(2, 0, 44, false), "introspection"),
            failure("oneAccount", 6015)
        );
    }
```

  The memo program is SPL Memo v3 (`memo::ID`), which the `context` helper already loads.
  - It requires every account it names to sign, which `signer` does in both instructions.
  - Mollusk builds the sysvar's account flags from each instruction's own metas, so memo 0's
    signer reads as signed and read-only.

- [ ] **Step 5: The signed-quote test.** Add after it:

```rust
    /// A deterministic Ed25519 key pair and its address.
    fn ed25519_keypair(seed: u8) -> (ed25519_dalek::Keypair, Pubkey) {
        let secret = ed25519_dalek::SecretKey::from_bytes(&[seed; 32]).expect("a 32-byte secret");
        let public = ed25519_dalek::PublicKey::from(&secret);
        let address = Pubkey::new_from_array(public.to_bytes());
        (ed25519_dalek::Keypair { secret, public }, address)
    }

    /// An Ed25519 precompile instruction with `count` copies of one signature over `message`,
    /// laid out as `new_ed25519_instruction_with_signature` lays out one: the offsets, then the
    /// key, the signature and the message. Every offset names `index` as the instruction holding
    /// its bytes; `u16::MAX` is the precompile instruction itself.
    fn ed25519_instruction(
        keypair: &ed25519_dalek::Keypair,
        message: &[u8],
        count: u8,
        index: u16,
    ) -> Instruction {
        use ed25519_dalek::Signer;
        let signature = keypair.sign(message).to_bytes();
        let key_offset = 2 + 14 * count as u16;
        let signature_offset = key_offset + 32;
        let message_offset = signature_offset + 64;
        let mut data = vec![count, 0];
        for _ in 0..count {
            for field in [
                signature_offset,
                index,
                key_offset,
                index,
                message_offset,
                message.len() as u16,
                index,
            ] {
                data.extend_from_slice(&field.to_le_bytes());
            }
        }
        data.extend_from_slice(&keypair.public.to_bytes());
        data.extend_from_slice(&signature);
        data.extend_from_slice(message);
        Instruction {
            program_id: solana_sdk_ids::ed25519_program::id(),
            accounts: vec![],
            data,
        }
    }

    /// The 120-byte quote `signed-quote-settlement.ts` reads.
    fn quote_message(
        price: u64,
        max_amount: u64,
        expiry: i64,
        taker: &Pubkey,
        base_mint: &Pubkey,
        quote_mint: &Pubkey,
    ) -> Vec<u8> {
        let mut message = Vec::with_capacity(120);
        message.extend_from_slice(&price.to_le_bytes());
        message.extend_from_slice(&max_amount.to_le_bytes());
        message.extend_from_slice(&expiry.to_le_bytes());
        for key in [taker, base_mint, quote_mint] {
            message.extend_from_slice(key.as_ref());
        }
        message
    }

    /// The signed-quote example as the TypeScript SDK compiles it, settling a quote whose
    /// signature the real Ed25519 precompile verifies. A tampered signature fails in the
    /// precompile, before Ballista runs; a valid one reaches the template, which binds it to the
    /// maker, the taker, the size, the expiry and the mints.
    #[test]
    fn signed_quote_settles_only_as_the_maker_signed() {
        let creator = Pubkey::new_unique();
        let (maker_key, maker) = ed25519_keypair(7);
        let (stranger_key, _) = ed25519_keypair(8);
        let taker = Pubkey::new_unique();
        let other_taker = Pubkey::new_unique();
        let (base_mint, quote_mint) = (Pubkey::new_unique(), Pubkey::new_unique());
        let taker_quote = Pubkey::new_unique();
        let maker_quote = Pubkey::new_unique();
        let maker_base = Pubkey::new_unique();
        let taker_base = Pubkey::new_unique();
        let diverted = Pubkey::new_unique();
        let mut accounts = funded_accounts([creator, maker, taker, other_taker], 10_000_000_000);
        for mint in [base_mint, quote_mint] {
            accounts.insert(
                mint,
                token::create_account_for_mint(Mint {
                    mint_authority: COption::None,
                    supply: 1_000_000_000_000,
                    decimals: 6,
                    is_initialized: true,
                    freeze_authority: COption::None,
                }),
            );
        }
        for (address, mint, owner, amount) in [
            (taker_quote, quote_mint, taker, 100_000_000),
            (maker_quote, quote_mint, maker, 0),
            (maker_base, base_mint, maker, 10_000_000),
            (taker_base, base_mint, taker, 0),
            (diverted, quote_mint, taker, 0),
        ] {
            accounts.insert(
                address,
                token::create_account_for_token_account(token_account_state(mint, owner, amount)),
            );
        }
        let mut context = context(accounts);
        context.mollusk.sysvars.clock.unix_timestamp = 1_800_000_000;
        let payload = fixture("signed-quote-settlement");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 92, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 92);
        let settle = |instructions: Vec<Instruction>, taker: Pubkey, payee: Pubkey, amount: u64| {
            let run = run_instruction(
                template,
                vec![
                    AccountMeta::new_readonly(sysvar::instructions::id(), false),
                    AccountMeta::new_readonly(token::ID, false),
                    AccountMeta::new_readonly(taker, true),
                    AccountMeta::new_readonly(maker, true),
                    AccountMeta::new(taker_quote, false),
                    AccountMeta::new(payee, false),
                    AccountMeta::new(maker_base, false),
                    AccountMeta::new(taker_base, false),
                ],
                &amount.to_le_bytes(),
            );
            let mut transaction = instructions;
            transaction.push(run);
            context.process_transaction_instructions(&transaction)
        };
        let expiry = 1_800_000_060;
        // 2.5 quote units per base unit, at most 4 base units.
        let message = quote_message(2_500_000, 4_000_000, expiry, &taker, &base_mint, &quote_mint);
        let quote = ed25519_instruction(&maker_key, &message, 1, u16::MAX);

        // A tampered signature fails in the precompile, instruction 0; Ballista never runs.
        let mut tampered = quote.clone();
        tampered.data[2 + 14 + 32] ^= 1;
        let rejected = settle(vec![tampered], taker, maker_quote, 3_000_001);
        assert_eq!(failed_instruction(&rejected), Some(0), "{rejected:#?}");

        // Each broken binding fails the run, instruction 1, at its own requirement.
        let failure = |label: &str| Some((1, 6015, label.to_owned()));
        let refused = |instructions: Vec<Instruction>, taker: Pubkey, payee: Pubkey, amount: u64| {
            fixture_failure(&settle(instructions, taker, payee, amount), "signed-quote-settlement")
        };
        let by_stranger = ed25519_instruction(&stranger_key, &message, 1, u16::MAX);
        assert_eq!(
            refused(vec![by_stranger], taker, maker_quote, 1),
            failure("quoteIsBySigner")
        );
        let twice = ed25519_instruction(&maker_key, &message, 2, u16::MAX);
        assert_eq!(
            refused(vec![twice], taker, maker_quote, 1),
            failure("quoteIsOneSelfContainedSignature")
        );
        // Index 0 is the precompile instruction here too, so it verifies; the template still
        // wants the explicit `u16::MAX`.
        let by_index = ed25519_instruction(&maker_key, &message, 1, 0);
        assert_eq!(
            refused(vec![by_index], taker, maker_quote, 1),
            failure("quoteIsOneSelfContainedSignature")
        );
        let short = ed25519_instruction(&maker_key, &message[..119], 1, u16::MAX);
        assert_eq!(
            refused(vec![short], taker, maker_quote, 1),
            failure("quoteIsOneSelfContainedSignature")
        );
        // With a memo in between, the run is instruction 2 and the one before it is the memo.
        let memo = Instruction {
            program_id: memo::ID,
            accounts: vec![],
            data: b"between".to_vec(),
        };
        assert_eq!(
            refused(vec![quote.clone(), memo], taker, maker_quote, 1),
            Some((2, 6015, "quoteIsEd25519".to_owned()))
        );
        let stale =
            quote_message(2_500_000, 4_000_000, 1_799_999_999, &taker, &base_mint, &quote_mint);
        let stale = ed25519_instruction(&maker_key, &stale, 1, u16::MAX);
        assert_eq!(
            refused(vec![stale], taker, maker_quote, 1),
            failure("quoteHasNotExpired")
        );
        assert_eq!(
            refused(vec![quote.clone()], other_taker, maker_quote, 1),
            failure("quoteIsForThisTaker")
        );
        assert_eq!(
            refused(vec![quote.clone()], taker, maker_quote, 4_000_001),
            failure("withinTheQuotedSize")
        );
        let other_market =
            quote_message(2_500_000, 4_000_000, expiry, &taker, &base_mint, &base_mint);
        let other_market = ed25519_instruction(&maker_key, &other_market, 1, u16::MAX);
        assert_eq!(
            refused(vec![other_market], taker, maker_quote, 1),
            failure("paysInTheQuotedMint")
        );
        assert_eq!(
            refused(vec![quote.clone()], taker, diverted, 1),
            failure("paymentReachesTheMaker")
        );

        // The real thing: 3.000001 base units at 2.5, rounded up in the maker's favour.
        let settled = settle(vec![quote], taker, maker_quote, 3_000_001);
        assert!(settled.program_result.is_ok(), "{settled:#?}");
        eprintln!("signed-quote settlement compute units: {}", settled.compute_units_consumed);
        assert_eq!(token_amount(&context, taker_quote), 100_000_000 - 7_500_003);
        assert_eq!(token_amount(&context, maker_quote), 7_500_003);
        assert_eq!(token_amount(&context, maker_base), 10_000_000 - 3_000_001);
        assert_eq!(token_amount(&context, taker_base), 3_000_001);
    }
```

What each case pins:
- **Tampered.** Byte `2 + 14 + 32` is the signature's first byte. Its failure index is 0, the
  precompile: the run never starts.
- **Two copies, index 0, and a message one byte short.** Each is a signature the precompile
  accepts, and each fails the header check.
  - Mollusk hands the precompile only its own data, so index 0 means itself here, as it does on
    chain for an instruction at position 0.
- **The payment.** 3,000,001 × 2,500,000 ÷ 1,000,000 = 7,500,002.5, which rounds up to 7,500,003.
- `maker` doubles as an Ed25519 public key and a signer. Mollusk does not check transaction
  signatures, only `is_signer`.

- [ ] **Step 6: Build and run both tests.**

```bash
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
cargo test --manifest-path tests/ballista/Cargo.toml -- \
  typescript_introspection_fixture_reads_the_transaction signed_quote_settles_only_as_the_maker_signed --nocapture \
  2>&1 | grep -E "compute units:|test result|panicked"
```

Expected: `test result: ok. 2 passed`, and two compute-unit lines. The prototype of this plan, on
`7a5e15e`'s layout, printed 4,784 for the introspection fixture's run and 9,548 for a full
settlement, both transfers included.

  The whole suite still has one known failure until Task 11:
  `compute_units_stay_under_their_ceiling` reports `create template, payroll 30 rows` a couple of
  units over its ceiling, the cost of Task 3's verifier arms.

- [ ] **Step 7: Commit.**

```bash
git add tests/ballista/Cargo.toml tests/ballista/Cargo.lock tests/ballista/src/lib.rs
git commit -m "Run the introspection fixture and a real Ed25519-signed settlement under Mollusk"
```

---

### Task 10: Formal specifications

**Files:** `certora/ballista-specs/src/rules/typing.rs` (`value_dependent`, 23–32). The error-code
rules were extended in Tasks 1 and 2.

Can the two new runtime errors arise from values? Yes, so they belong in `value_dependent`.
- `rule_verified_account_reads_preserve_register_typing` runs against `SPEC_PROGRAM_WITH_ACCOUNT`,
  whose one fixed account is unconstrained. The verifier accepts `READ_ACCOUNT_BYTES` against it:
  `require_account` passes, and `b` can be a `u64` register.
- The nondeterministic account may be writable, which gives `WritableAccountBytesRead`. The offset
  may point past its data, which gives `InstructionOutOfRange`. Both depend on the values the
  instruction sees, not its shape.
- The nine sysvar opcodes never reach the executor in either rule. Neither spec program pins the
  Instructions sysvar, so the verifier rejects them with `InvalidIntrospection`. An index past the
  transaction would be value-dependent too.
- `BYTES_LEN` reaches the pure rule when a register is typed `bytes`, and it cannot fail there.
- `writes_destination` needs no change: all eleven opcodes write `dst`.

- [ ] **Step 1: Extend `value_dependent`.** Replace the function and its doc comment with:

```rust
/// Errors an accepted instruction may raise because of the values it sees, not its shape.
///
/// `READ_ACCOUNT_BYTES` reaches the executor in the account rule, whose one account is
/// unconstrained: the account may be writable in the transaction, and the offset register may
/// point past its data. The introspection opcodes never do, since neither spec program pins the
/// Instructions sysvar, but an index past the transaction is value-dependent too.
fn value_dependent(kind: BallistaError) -> bool {
    matches!(
        kind,
        BallistaError::ArithmeticOverflow
            | BallistaError::DivisionByZero
            | BallistaError::RequirementFailed
            | BallistaError::InvalidPdaDerivation
            | BallistaError::InstructionOutOfRange
            | BallistaError::WritableAccountBytesRead
    )
}
```

  If phase 2 has already added `LoopCountExceeded` to this list, keep it.

- [ ] **Step 2: Typecheck the specs and their host tests.**

```bash
cargo check -p ballista-specs --features rt --manifest-path certora/Cargo.toml
cargo test -p ballista-specs --features rt --manifest-path certora/Cargo.toml
```

Expected: PASS. `util.rs`'s tests check that the spec programs still match `ProgramBuilder`. They
do, because the wire format did not change.

- [ ] **Step 3: Check the frames.** `cargo certora-sbf` builds for SBPF version 0 and reports any
  function whose stack frame exceeds 4 KiB.

```bash
cd certora/ballista-specs
cargo certora-sbf --tools-version v1.53 2>&1 | grep -i -E "stack|frame" || echo "no frame warnings"
cd ../..
```

Expected: `no frame warnings`. The prototype reported none. Frame by frame:
- `read_instruction` holds one parsed instruction and a `RuntimeValue`.
- `read_account_bytes` holds a slice.
- The router's new arms hold a reference and a `u64`.

- [ ] **Step 4: Commit.**

```bash
git add certora/ballista-specs/src/rules/typing.rs
git commit -m "Count out-of-range and writable byte reads as value-dependent in the typing rule"
```

---

### Task 11: Compute units

**Files:**
- `tests/ballista/src/cases.rs`: the imports, `cases()`, and a new case function.
- `fixtures/cu-ceilings.json`, by hand.
- `benches/CHANGELOG.md`.
- `programs/ballista/src/processor/math.rs`, only if Step 4 finds a de-inlined math helper.

The new opcodes live behind `extended_instruction`, so no existing run should move. Three things
still can:
- **The upload case.** The verifier's larger match costs the `create template` case a few units.
- **The router.** Every math opcode and `READ_I32` runs through it too, so a larger inner match
  can move their register allocation.
- **Inlining.** LLVM decides it by function size, so a helper `extended_instruction` used to
  inline can become a call.

What the prototype of this plan measured, on `7a5e15e`'s layout:
- **Unchanged.** Every `run, …` case and every cookbook example.
- **Upload.** `create template, payroll 30 rows`: 4,446 → 4,448, from the verifier's new arms.
- **Math.** The TypeScript math fixture's exact run: 4,859 → 4,849. That needed
  `#[inline(always)]` on `math::remainder`; without it LLVM stopped inlining `remainder`, and the
  fixture cost 4,897.
- **New.** `run, introspection, no cpi`: 3,647.

Your numbers will differ after phase 1's refactor of the router. The procedure is what matters.

- [ ] **Step 1: The ratchet case.** In `cases.rs`:
  - Replace the `ballista_common::template` import with:

```rust
use ballista_common::template::{
    ProgramBuilder, Segment, TemplateAccountHeader, ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER,
    ACCOUNT_WRITABLE, DATA_REG_U64, INSTRUCTIONS_SYSVAR_ID, NO_INDEX, OP_ADD, OP_AND, OP_EQ, OP_GTE,
    OP_INSTRUCTION_ACCOUNT, OP_INSTRUCTION_ACCOUNT_COUNT, OP_INSTRUCTION_ACCOUNT_FLAGS,
    OP_INSTRUCTION_COUNT, OP_INSTRUCTION_DATA_LEN, OP_INSTRUCTION_INDEX, OP_INSTRUCTION_PROGRAM,
    OP_LTE, OP_READ_U64, OP_READ_U8, VALUE_U64,
};
```

  - Change `use solana_sdk_ids::system_program;` to `use solana_sdk_ids::{system_program, sysvar};`.
  - In `cases()`, after the `create template` line, add
    `("run, introspection, no cpi", introspection(creator, 10)),`.
  - Append:

```rust
/// Every introspection and byte opcode once, against the run's own instruction. Alone in its
/// transaction it is instruction 0, the template is its first account, and its data is the
/// `IX_RUN` discriminator and then the one `u64` input, which the template compares with the same
/// eight bytes read from an account.
fn introspection(creator: Pubkey, template_id: u16) -> Case {
    let (template, _) = Pubkey::find_program_address(
        &[TEMPLATE_SEED, creator.as_ref(), &template_id.to_le_bytes()],
        &ID,
    );
    let mut builder = ProgramBuilder::new();
    let instructions = builder.account(0, Some(INSTRUCTIONS_SYSVAR_ID), None, 0);
    let oracle = builder.account(0, None, None, 128);
    builder.input(VALUE_U64, 0);
    let zero = builder.const_u64(0);
    let one = builder.const_u64(1);
    let at_price = builder.const_u64(64);
    let current = builder.introspect(OP_INSTRUCTION_INDEX, instructions, NO_INDEX, NO_INDEX);
    let input = builder.read_instruction_bytes(instructions, current, one, 8);
    let checks = [
        (
            builder.introspect(OP_INSTRUCTION_COUNT, instructions, NO_INDEX, NO_INDEX),
            builder.const_u64(1),
        ),
        (
            builder.introspect(OP_INSTRUCTION_PROGRAM, instructions, current, NO_INDEX),
            builder.const_pubkey(ID.to_bytes()),
        ),
        (
            builder.introspect(OP_INSTRUCTION_ACCOUNT_COUNT, instructions, current, NO_INDEX),
            builder.const_u64(3),
        ),
        (
            builder.introspect(OP_INSTRUCTION_ACCOUNT, instructions, current, zero),
            builder.const_pubkey(template.to_bytes()),
        ),
        (
            builder.introspect(OP_INSTRUCTION_ACCOUNT_FLAGS, instructions, current, zero),
            zero,
        ),
        (
            builder.introspect(OP_INSTRUCTION_DATA_LEN, instructions, current, NO_INDEX),
            builder.const_u64(9),
        ),
        (
            builder.read_instruction_data(OP_READ_U8, instructions, current, zero),
            builder.const_u64(IX_RUN as u64),
        ),
        (input, builder.read_account_bytes(oracle, at_price, 8)),
        (builder.bytes_len(input), builder.const_u64(8)),
    ];
    for (value, expected) in checks {
        let same = builder.binary(OP_EQ, value, expected);
        builder.require(same);
    }
    let (oracle_key, oracle_account) = with_data(template_id * 100 + 1);
    let mut case = run_case(
        creator,
        template_id,
        builder.build().expect("builds"),
        vec![(AccountMeta::new_readonly(oracle_key, false), oracle_account)],
        7u64.to_le_bytes().to_vec(),
    );
    // Mollusk builds the Instructions sysvar from the instruction when the case does not supply it.
    case.instruction
        .accounts
        .insert(1, AccountMeta::new_readonly(sysvar::instructions::id(), false));
    case
}
```

  `with_data` puts `7u64` at bytes 64..72 of a 128-byte account, and the run's input is `7u64`, so
  the two 8-byte ranges match. The sysvar meta goes after the template: fixed account 0 is the
  sysvar and 1 the oracle. The ceiling harness runs each case with `Mollusk::process_instruction`,
  which builds the sysvar from that one instruction.

- [ ] **Step 2: Build and run the ratchet.**

```bash
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
cargo test --manifest-path tests/ballista/Cargo.toml compute_units_stay_under_their_ceiling -- --nocapture 2>&1 \
  | grep -E "no ceiling|over by|to spare|test result"
```

Expected: FAIL. The output has a `run, introspection, no cpi: … CU, no ceiling recorded` line with
the new case's figure, and a `create template, payroll 30 rows: … over by …` line, unless your
build happens to absorb the verifier's arms.

- [ ] **Step 3: Measure every case against the base.** A table in the commit message needs before
  and after figures from one machine and one toolchain.
  1. Build the base program in a scratch worktree.
  2. Add three temporary print lines.
  3. Run both builds.
  4. Revert the prints.

  Never use `git stash`.

```bash
BASE=$(git merge-base HEAD claude/runtime-extensions)
git worktree add --detach ../introspection-base "$BASE"
cargo build-sbf --manifest-path ../introspection-base/programs/ballista/Cargo.toml
cp ../introspection-base/target/deploy/ballista.so target/deploy/ballista-before.so
cp target/deploy/ballista.so target/deploy/ballista-after.so
```

  Add these three lines, then run the loop:
  - In `tests/ballista/src/ceilings.rs`, after `let units = result.compute_units_consumed;`, add
    `eprintln!("CU\t{name}\t{units}");`.
  - In `tests/ballista/src/benchmarks.rs`, after `let ballista_units = run.compute_units_consumed;`,
    add `eprintln!("EX\t{name}\t{ballista_units}");`.
  - In `tests/ballista/src/lib.rs`, add
    `eprintln!("MATH\texact\t{}", exact.compute_units_consumed);` after
    `assert!(exact.program_result.is_ok(), "{exact:#?}");` in
    `typescript_math_fixture_computes_exact_results`. The file has two such lines; use the one
    after `let exact = run(1_000_003, 7, 3, 0xabcd, 18);`.

```bash
for label in before after; do
  cp target/deploy/ballista-$label.so target/deploy/ballista.so
  cargo test --manifest-path tests/ballista/Cargo.toml -- \
    compute_units_stay_under_their_ceiling measure_every_example typescript_math_fixture_computes_exact_results \
    --nocapture --test-threads=1 2>&1 | grep -E $'^(CU|EX|MATH)\t' | sort > target/cu-$label.tsv
done
join -t "$(printf '\t')" -a 2 -e - -o 0,1.2,2.2 \
  <(awk -F'\t' '{print $1"/"$2"\t"$3}' target/cu-before.tsv | sort) \
  <(awk -F'\t' '{print $1"/"$2"\t"$3}' target/cu-after.tsv | sort) \
  | awk -F'\t' '{ d = ($2 == "-") ? "new" : sprintf("%+d", $3 - $2); printf "%-48s %8s %8s %6s\n", $1, $2, $3, d }'
cp target/deploy/ballista-after.so target/deploy/ballista.so
git checkout -- tests/ballista/src/ceilings.rs tests/ballista/src/benchmarks.rs tests/ballista/src/lib.rs
```

  How to read the table:
  - With the base build, the new case cannot run: it is last in `cases()` and fails its assert
    after every other case has printed. So it shows only in `after`, marked `new`.
  - Paste the table into the commit message.
  - `git checkout --` restores only files this task does not otherwise edit. Task 9 committed
    `lib.rs`.

- [ ] **Step 4: Compare the functions that are out of line, then remove the base worktree.**

```bash
OBJDUMP=$(ls -d ~/.cache/solana/*/platform-tools/llvm/bin/llvm-objdump | tail -1)
for tree in ../introspection-base .; do
  echo "== $tree"
  $OBJDUMP -d --no-show-raw-insn $tree/target/sbpf-solana-solana/release/ballista.so \
    | grep -E '^[0-9a-f]+ <.*(math|introspect|extended_instruction).*>:$' \
    | sed -E 's/17h[0-9a-f]{16}E>:$//; s/^[0-9a-f]+ <//'
done
git worktree remove ../introspection-base
```

  - Expected after this plan: the base list plus `introspect::read_instruction` and
    `introspect::read_account_bytes`.
  - If `math::remainder`, `math::shift`, `math::bitwise` or `math::pow10` appears only in the new
    build, the grown router pushed it past LLVM's inlining threshold. Every opcode that reaches it
    now pays a call.
  - Add `#[inline(always)]` to that function in `math.rs` (the prototype needed it on
    `fn remainder`), rebuild, and repeat Steps 3 and 4, recreating the base worktree, until the
    math row is back at or below its base.

- [ ] **Step 5: Raise the ceilings by hand**, to the exact measured figures. In
  `fixtures/cu-ceilings.json`:
  - Set `create template, payroll 30 rows` to its new figure.
  - Add `"run, introspection, no cpi": <figure>` in its sorted place, between `run, empty template`
    and `run, one system transfer`.
  - Change nothing else.
  - If any other case or example rose, stop and find out why before raising it: see phase 1's
    Task 9, and Step 4 above.

  Do not run `pnpm cu:ceilings` to do this. It only lowers and adds, but editing by hand keeps the
  review honest.

- [ ] **Step 6: Record it in the compute-unit changelog.** `benches/CHANGELOG.md` asks that a
  change that makes something slower say why there, in the same commit. Insert this section
  directly before `## Pending: integrated, not merged`, with your figures:

```md
## Runtime extensions

### 2026-09-27 · Introspection and byte opcodes · `claude/runtime-introspection`
- **Change:** eleven opcodes, 64 to 74, reach the executor through `extended_instruction`'s inner
  match, whose outer dispatch is untouched. The count, the index and `BYTES_LEN` run there; the
  sysvar parsing and both byte reads run in two `#[inline(never)]` helpers that take four words
  each, all in registers.
- **Measured:**
  - Uploading the 30-row payroll: 4,446 → 4,448, the verifier's new arms.
  - A run using all eleven (`run, introspection, no cpi`): 3,647, now ratcheted.
  - The TypeScript math fixture: 4,859 → 4,849.
  - Every other case and every cookbook example: unchanged.
- **Checked:** the introspection fixture and a signed-quote settlement under Mollusk; every host
  and Mollusk test.
- **Watch:** a helper that takes the loop context as an argument takes words from the stack, and
  their loads move to `extended_instruction`'s entry, where every math opcode pays for them. The
  larger router also stopped LLVM inlining `math::remainder`, which `#[inline(always)]` now pins.
```

  Also add this bullet to `## Tried and rejected`, as its last, directly after the
  **PDA derivation** bullet with no blank line between them:

```md
- **Introspection dispatch** (math fixture 4,859 at base, on `7a5e15e`'s layout):
  - Three arms whose helpers took the loop context: +50, from the stack-passed words' loads on
    the router's entry.
  - One `64..=74` arm into one helper: +47, and +42 on an introspecting run.
  - The router's fallback arm into the helper: +47, and +53 on a settlement.
  - Sharing the count-and-index code between the router and the parser: +10 on the ratchet case.
```

- [ ] **Step 7: Run the whole suite.**

Run: `cargo test --manifest-path tests/ballista/Cargo.toml`
Expected: PASS, every test including both ceiling tests.

- [ ] **Step 8: Commit, with the table.**

```bash
git add tests/ballista/src/cases.rs fixtures/cu-ceilings.json benches/CHANGELOG.md programs/ballista/src/processor/math.rs
git commit -m "Ratchet the introspection path and raise the upload ceiling the verifier's arms cost

create template, payroll 30 rows: 4446 -> <new>, the verifier's new match arms.
run, introspection, no cpi: <new>, a new case that runs all eleven opcodes.

<Step 3's table>"
```

  Replace the three placeholders with the measured figures before committing. If `math.rs` did
  not change, `git add` ignores it.

---

### Task 12: Phase verification

- [ ] **Step 1: Run everything CI runs.**

```bash
pnpm install --frozen-lockfile
pnpm fixtures && git diff --exit-code fixtures
pnpm check
pnpm test
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
cargo test --manifest-path tests/ballista/Cargo.toml
cargo test -p ballista-common --test no_panic --test generated --features proptest
cargo check -p ballista-specs --features rt --manifest-path certora/Cargo.toml
cargo test -p ballista-specs --features rt --manifest-path certora/Cargo.toml
cargo clippy -p ballista -p ballista-common --all-targets 2>&1 | grep -c -E "^(warning|error)"
git diff --check
```

Expected: every command exits 0, and the clippy count equals Task 0's. On `dedc168`, a dry run of
this plan passed:
- 45 program tests, 42 `ballista-common` library tests and 5 SDK tests;
- 109 vitest tests, with 5 skipped;
- 39 Mollusk tests.

- [ ] **Step 2: Report.**
  - List the commits.
  - Give the test counts, and the compute-unit table from Task 11.
  - Name anything that moved and why.
  - Say whether Task 13 is still to do: this branch merges only after it.

---

### Task 13: Rebase onto phases 2 and 3

Do this once loops (phase 2) and output (phase 3) have merged into `claude/runtime-extensions`,
and skip it if Task 1 was skipped. Task 1's reservation commit is superseded by those phases'
real definitions and is dropped here. Every other commit keeps its content.

- [ ] **Step 1: Confirm the base has both phases.**

```bash
git log --oneline -5 claude/runtime-extensions
git show claude/runtime-extensions:common/src/template/wire.rs | grep -n "LoopCountExceeded\|InvalidLoop\|InvalidOutput\|OP_[A-Z_]*: u8 = 6[1-3];"
```

Expected: the three names and opcodes 61 to 63. If they are missing, stop and wait.

- [ ] **Step 2: Rebase, dropping the reservation.**

```bash
git rebase claude/runtime-extensions
```

  - **The plan commit** replays cleanly.
  - **The reservation commit** conflicts, because upstream now defines the same three names. Drop
    it with `git rebase --skip`. Upstream's definitions of 6022, 6129 and 6130 win, whatever their
    doc comments or payloads.
  - **Every later commit** may conflict where both sides appended to the same list. Resolve each
    by keeping upstream's entries and adding this branch's after them, in numeric order. Where
    they collide:
    - `wire.rs`: the opcodes (61–63 upstream, then 64–74) and `RUNTIME_ERROR_NAMES`, whose length
      is the count of names, 25. Then `TemplateError` and `code` (`InvalidIntrospection` after
      `InvalidOutput`, whatever shape upstream gave it) and `VERIFIER_ERROR_NAMES`, length 32.
    - `error.rs`: the enum and its test list.
    - `errors.ts`; the first-unused assertions in `errors.test.ts` and `clients/rust/src/lib.rs`
      (keep this branch's 6025 and 6132).
    - `certora/.../errors.rs` (`pick!` list and index match); `typing.rs` `value_dependent`
      (keep both sides' kinds).
    - `verify.rs`:
      - the arms after `OP_POW10`;
      - the sweep table, where the probe stays at `OP_BYTES_LEN + 1`;
      - the fixture array, whose length is the number of entries;
      - the protocol count, which only this phase raises, so it stays 13.
    - `execute.rs`: `extended_instruction`'s inner match, and the unrun-opcode list. That list
      must not contain an opcode either side now runs; `[0, 39, OP_FOREACH, OP_BYTES_LEN + 1,
      0xfe, u8::MAX]` is safe.
    - `compiler.ts`'s `opcode` table and `opcodes.test.ts`'s `rustName`.
    - `fixtures.test.ts`'s `fixtures` map, and `tests/ballista/src/lib.rs`'s `fixture()` arms and
      imports.
    - `benches/CHANGELOG.md`: keep both sections.
    - **Generated fixtures** (`fixtures/*.hex`, `manifest.json`, `protocol-examples.json`, the
      error-name `.txt` files): take either side, run `pnpm fixtures` once the sources are
      resolved, and `git add fixtures` before `git rebase --continue`.
    - **`tests/ballista/Cargo.lock`**: take upstream's, then run
      `cargo tree --manifest-path tests/ballista/Cargo.toml -i agave-precompiles` to re-add this
      branch's packages, and `git add` it.
    - **`fixtures/cu-ceilings.json` and `example-ceilings.json`**: take upstream's figures and
      re-add `"run, introspection, no cpi"`. Step 4 re-measures.
  - Never resolve a conflict with `git stash`, and never pass `-X ours` or `-X theirs` for the
    whole rebase.

- [ ] **Step 3: Check the tables once more.**

```bash
grep -c '"LoopCountExceeded"' common/src/template/wire.rs
grep -n "pub const OP_" common/src/template/wire.rs | sed -n '/= 6[0-9];\|= 7[0-4];/p'
cargo test -p ballista --lib error
```

Expected: `1`, then opcodes 60 to 74 each exactly once, then PASS.

- [ ] **Step 4: Re-verify and re-measure.** Run Task 12, Step 1. Loops and output changed the
  dispatch, so repeat Task 11, Steps 2 to 5, against the new base.
  - If this branch's ceilings moved, commit the change on its own, with its table:
    `Re-measure the introspection ceilings on top of loops and output`.
  - Then report the rebased branch as ready for review, as in Task 12, Step 2.
