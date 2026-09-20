# Row Inputs and Account Groups Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a batch carry a row of inputs per iteration, let a template forward caller-sized account groups to CPIs, and name the completed bytecode version 1.

**Architecture:** Both features consume header bytes the current format requires to be zero (`reserved[0]`, `reserved[1]`) and the CPI descriptor's `reserved0`. Row inputs extend the inputs table and the run data; groups add a length prefix to the run data and a tail of runtime accounts after the batch rows. The executor learns the layout before it parses inputs, the verifier gains a handful of rules, and both compilers emit the same bytes for the shared fixtures. Spec: `docs/superpowers/specs/2026-09-20-row-inputs-and-dynamic-account-groups-design.md`.

**Tech Stack:** Rust (pinocchio program, `common` crate with zerocopy records, Mollusk 0.14 tests, proptest), TypeScript SDK (zod schema, vitest), VitePress docs, Certora specs.

---

## File structure

| File | Responsibility in this plan |
| --- | --- |
| `common/src/template/wire.rs` | Constants, header fields, descriptor field, parse, error tables |
| `common/src/template/verify.rs` | Static rules for row inputs and groups |
| `common/src/template/builder.rs` | Rust authoring API for both features |
| `common/src/template/generate.rs` | Property-test coverage of both features |
| `programs/ballista/src/processor/execute.rs` | Run layout, input parsing, row loads, group forwarding |
| `programs/ballista/src/error.rs` | Runtime error 6021 |
| `tests/ballista/src/lib.rs` | Mollusk scenarios |
| `clients/js/src/{schema,compiler,instructions,errors,kit}.ts` | Schema, emission, run encoding |
| `clients/js/src/{compiler,fixtures,kit}.test.ts` | Tests and shared fixtures |
| `clients/rust/src/lib.rs` | `RunInputs::groups` |
| `docs/guide/batching.md`, `docs/guide/account-groups.md`, `docs/reference/{limits,wire-format}.md`, `docs/guide/errors-and-events.md`, `docs/.vitepress/config.mts` | Documentation |
| `certora/ballista-specs/src/rules/*.rs` | Rules follow the new `validate_runtime_accounts` signature; constants regenerated |

Test commands used throughout:

```bash
cargo test -p ballista-common                                  # wire, verify, builder unit tests
cargo test -p ballista --lib                                   # executor and error unit tests
cargo build-sbf --manifest-path programs/ballista/Cargo.toml   # SBF binary the Mollusk suite loads
cargo test --manifest-path tests/ballista/Cargo.toml           # Mollusk suite
cargo test -p ballista-common --features proptest --test no_panic --test generated
pnpm --dir clients/js check && pnpm --dir clients/js test      # TypeScript
UPDATE_FIXTURES=1 pnpm --dir clients/js test fixtures          # regenerate fixtures/
cargo test -p ballista-sdk                                     # Rust client
```

---

### Task 1: Wire format

**Files:** Modify `common/src/template/wire.rs`, `fixtures/runtime-error-names.txt`, `fixtures/verifier-error-names.txt`, `programs/ballista/src/error.rs`.

- [ ] **Step 1: Constants.** In `wire.rs` change `MAX_RUNTIME_ACCOUNTS` to `120` and add:

```rust
/// Row inputs a batch may declare; each is carried once per iteration in the run data.
pub const MAX_ROW_INPUTS: usize = 8;
/// Parsed input values per run: fixed inputs plus `iterations × row inputs`.
pub const MAX_INPUT_VALUES: usize = 256;
/// Caller-sized account groups a template may declare.
pub const MAX_ACCOUNT_GROUPS: usize = 8;
/// `LOAD_INPUT` operand bit selecting a row input of the current iteration.
pub const ITERATION_INPUT_BIT: u8 = ITERATION_ACCOUNT_BIT;
```

- [ ] **Step 2: Header.** Replace `reserved: [u8; 3]` with `row_input_count: u8, account_group_count: u8, reserved: [u8; 1]`. Extend `ProgramHeader::new` with `row_input_count: u8, account_group_count: u8` after `batch_min_iterations`. Add accessors `row_input_count()`, `account_group_count()`, `total_input_count()` (`input_count + row_input_count`). `reserved()` returns `&[u8; 1]`.
- [ ] **Step 3: Descriptor.** In `CpiDescriptor` rename `reserved0` to `account_group` and add `pub fn account_group(&self) -> Option<usize>` returning `None` for `NO_INDEX`.
- [ ] **Step 4: Parse.** In `ProgramView::parse` read `header.total_input_count()` input records and check `header.reserved() != &[0; 1]`.
- [ ] **Step 5: Input lookup.** Add to `ProgramView`:

```rust
/// The descriptor a `LOAD_INPUT` operand names: a fixed input, or inside a loop a row input.
pub fn input_descriptor(&self, reference: u8, in_loop: bool) -> Option<&InputDescriptor> {
    if reference & ITERATION_INPUT_BIT == 0 {
        return self.inputs.get(reference as usize).filter(|_| (reference as usize) < self.header.input_count());
    }
    if !in_loop { return None; }
    let offset = (reference & !ITERATION_INPUT_BIT) as usize;
    if offset >= self.header.row_input_count() { return None; }
    self.inputs.get(self.header.input_count() + offset)
}
```

- [ ] **Step 6: Error tables.** Append `"CpiAccountLimitExceeded"` to `RUNTIME_ERROR_NAMES` (22 entries) and `TooManyAccountGroups` (code 28) to `TemplateError`, `code()`, and `VERIFIER_ERROR_NAMES` (29 entries). Append the same names to the two fixture files. In `programs/ballista/src/error.rs` add `CpiAccountLimitExceeded` (6021) to `BallistaError` and to the test's variant list.
- [ ] **Step 7: Tests.** `cargo test -p ballista-common && cargo test -p ballista --lib` pass; the header is still 24 bytes (existing size assertion).
- [ ] **Step 8: Commit** `Extend the wire format with row inputs and account groups`.

### Task 2: Verifier

**Files:** Modify `common/src/template/verify.rs`.

- [ ] **Step 1: Header rules** after the batch checks in `verify()`:

```rust
if header.row_input_count() > MAX_ROW_INPUTS || header.total_input_count() > MAX_INPUTS {
    return Err(TemplateError::TooManyInputs);
}
if header.row_input_count() != 0 && header.batch_stride() == 0 {
    return Err(TemplateError::InvalidBatch);
}
let input_values = header.input_count()
    .checked_add(header.row_input_count().checked_mul(header.batch_max_iterations()).ok_or(TemplateError::CountOverflow)?)
    .ok_or(TemplateError::CountOverflow)?;
if input_values > MAX_INPUT_VALUES { return Err(TemplateError::TooManyInputs); }
if header.account_group_count() > MAX_ACCOUNT_GROUPS { return Err(TemplateError::TooManyAccountGroups); }
```

- [ ] **Step 2: Row loads.** In `verify_instruction`, `OP_LOAD_INPUT` resolves through `self.input_descriptor(instruction.a, in_loop)`; `None` is `InvalidInstruction(index)`.
- [ ] **Step 3: Descriptor groups.** In `verify_cpi_shape`, replace the `reserved0 != 0` test with: `account_group` must be `NO_INDEX` or below `header.account_group_count()`, else `InvalidCpi(index)`.
- [ ] **Step 4: Tests** in the `verify.rs` test module using `ProgramBuilder` (Task 3 methods): row input accepted inside the loop, rejected outside, rejected without a batch, offset past the row rejected, 9 row inputs rejected, 33 total inputs rejected, `4 + 60 × 8` values rejected, 9 groups rejected, descriptor group `8` with count `8` rejected, group `0` with count `1` accepted.
- [ ] **Step 5: Commit** `Verify row inputs and account groups`.

### Task 3: Builder and generator

**Files:** Modify `common/src/template/builder.rs`, `common/src/template/generate.rs`, `common/tests/generated.rs`.

- [ ] **Step 1: Builder API.**

```rust
/// Declares one input of the batch row and returns its iteration reference.
pub fn row_input(&mut self, value_type: u8, max_len: u16) -> u8 { /* push to self.row_inputs; ITERATION_INPUT_BIT | offset */ }
/// Declares `count` caller-sized account groups.
pub fn account_groups(&mut self, count: u8) -> &mut Self
/// Declares a CPI that forwards account group `group` after its declared accounts.
pub fn cpi_with_group(&mut self, program: u8, accounts: &[(u8, u8)], segments: &[Segment], group: u8) -> u8
```

`load_input` accepts either reference kind unchanged. `build()` writes fixed inputs then row inputs, and the two new header fields.
- [ ] **Step 2: Generator.** `GeneratedProgram` gains `iterations: usize`, `row_inputs: usize`, `group_lengths: Vec<u8>`. Choices: 0 to 2 row inputs when a batch exists (scalar types only), loaded inside the loop like fixed inputs; 0 to 2 groups with lengths 0 to 2; a generated CPI (if any exist) may name a group. `run_inputs` is built as prefix, fixed values, then `iterations × row` values. `common/tests/generated.rs` asserts `stats`/header counts for the new fields.
- [ ] **Step 3: Tests.** `cargo test -p ballista-common --features proptest --test no_panic --test generated` passes.
- [ ] **Step 4: Commit** `Build and generate row inputs and account groups`.

### Task 4: Executor

**Files:** Modify `programs/ballista/src/processor/execute.rs`.

- [ ] **Step 1: Layout.** Add

```rust
/// Where the runtime accounts of one run fall: fixed, batch rows, then the account groups.
pub struct RunLayout { pub iterations: usize, pub declared: usize, pub group_starts: [u16; MAX_ACCOUNT_GROUPS], pub group_lens: [u8; MAX_ACCOUNT_GROUPS] }
/// Splits the group-length prefix off the run data.
pub fn parse_group_prefix<'d>(program: &ProgramView<'_>, data: &'d [u8]) -> RunResult<([u8; MAX_ACCOUNT_GROUPS], &'d [u8])>
```

A short prefix is `VmAt(InvalidRunInputs, 0)`.
- [ ] **Step 2: Accounts.** `validate_runtime_accounts(program, accounts, group_lens) -> RunResult<RunLayout>`: total `≤ MAX_RUNTIME_ACCOUNTS`; `group_total = Σ lens`; `remainder = accounts.len() − fixed − group_total` (underflow is `InvalidAccountRange(accounts.len())`); stride 0 requires remainder 0, else remainder must be a whole number of rows within the bounds; constraints are checked for `declared = fixed + iterations × stride` accounts only; group starts are prefix sums from `declared`.
- [ ] **Step 3: Inputs.** `parse_inputs(program, data, iterations)` parses `input_count` fixed descriptors then `iterations` copies of the row descriptors, failing with the running value index. `run()` becomes: prefix → layout → inputs → execute; `Scratch` stores the layout.
- [ ] **Step 4: Row loads.** In `execute_instruction`:

```rust
OP_LOAD_INPUT => {
    let index = if instruction.a & ITERATION_INPUT_BIT == 0 {
        instruction.a as usize
    } else {
        let (iteration, _) = loop_context.ok_or(BallistaError::InvalidTemplateProgram)?;
        let offset = (instruction.a & !ITERATION_INPUT_BIT) as usize;
        if offset >= program.header.row_input_count() { return Err(BallistaError::InvalidTemplateProgram.into()); }
        program.header.input_count() + iteration * program.header.row_input_count() + offset
    };
    let value = *inputs.get(index).ok_or(BallistaError::InvalidTemplateProgram)?;
    set(registers, dst, value)?;
}
```

- [ ] **Step 5: Groups in CPIs.** In `invoke_cpi`, after the declared records, if `descriptor.account_group()` is `Some(group)`: check `account_len + group_len ≤ MAX_CPI_ACCOUNTS` else `VmAt(CpiAccountLimitExceeded, total)`; push each group account as `InstructionAccount::new(address, account.is_writable(), false)`.
- [ ] **Step 6: Tests** in the executor test module: prefix parsing (missing, present, empty groups), layout with stride and groups, row load inside and outside a loop, `parse_inputs` with two rows and a short second row (`InvalidRunInputs` with the value index), group forwarding builds metas with signer cleared, cap exceeded.
- [ ] **Step 7: Commit** `Execute row inputs and forward account groups`.

### Task 5: Mollusk scenarios

**Files:** Modify `tests/ballista/src/lib.rs`.

- [ ] **Step 1** `row_inputs_pay_a_different_amount_per_recipient`: three rows, amounts 1_000/2_000/3_000, balances checked per recipient; a run with two rows of inputs for three rows fails with `InvalidRunInputs`.
- [ ] **Step 2** `account_groups_are_forwarded_after_declared_accounts`: a System transfer CPI with `cpi_with_group(..., 0)`, one group holding two extra accounts (the System Program ignores them); prefix `[2]`; balances move. Same template with prefix `[0]` and no extra accounts succeeds. Prefix `[1]` with no extra accounts fails with `InvalidAccountRange`.
- [ ] **Step 3** `group_and_batch_coexist`: stride-1 batch plus one group; rows and group both resolved.
- [ ] **Step 4** `group_accounts_never_sign`: the runner is passed in the group as a signer and the CPI declares no signer; the System transfer fails with `MissingRequiredSignature` from the callee (proves the flag was cleared).
- [ ] **Step 5** `cpi_account_cap_counts_the_group`: 4 declared + group of 61 → error 6021 with context 65.
- [ ] **Step 6** Rename `sixty_runtime_accounts_are_the_ceiling` to `one_hundred_twenty_runtime_accounts_are_the_ceiling` and use 120/121; the entrypoint in `programs/ballista/src/lib.rs` becomes `entrypoint!(process_instruction, 128)`.
- [ ] **Step 7** Build SBF, run the suite, commit `Cover row inputs and account groups end to end`.

### Task 6: TypeScript SDK

**Files:** Modify `clients/js/src/schema.ts`, `compiler.ts`, `instructions.ts`, `kit.ts`, `errors.ts`, `index.ts`; tests `compiler.test.ts`, `kit.test.ts`, `fixtures.test.ts`.

- [ ] **Step 1: Schema.** `batch.rowInputs: namedInputs.default({})` (1 to 8 when present, requires batch), `accountGroups: z.array(identifier).max(8).default([])`, invoke `accountGroup: identifier.optional()`, expression `{ kind: 'rowInput'; name }` with helper `expression.rowInput(name)`. Refinements: total inputs ≤ 32; `inputs + maxIterations × rowInputs ≤ 256`; `accountGroup` names must be declared; account cap 120.
- [ ] **Step 2: Compiler.** Header bytes 21 and 22 from the counts; input records fixed then row; `rowInput` compiles to `loadInput` with `ITERATION_ACCOUNT_BIT | index`, only inside `forEach`; descriptor byte 1 is the group index or `0xff`; `CompiledTemplate` gains `rowInputOrder`, `accountGroupOrder`; stats gain `rowInputs`, `accountGroups`.
- [ ] **Step 3: Run encoding.** `encodeRunInputs(compiled, values, options?: { rows?: Record<string, RunInputValue>[]; groupLengths?: number[] })` writes the prefix, fixed values, then rows; `buildRunInstruction` gains `batchInputs?` (must match `batchRows.length` when the template has row inputs) and `accountGroups?: Record<string, AccountBinding[]>` appended after rows with `writable` from a per-binding flag (`AccountBinding` gains optional `writable`); cap 120. `kit.ts` passes the new fields through.
- [ ] **Step 4: Errors.** Append `CpiAccountLimitExceeded` and `TooManyAccountGroups`.
- [ ] **Step 5: Tests and fixtures.** New fixtures `payroll-row-amounts` and `group-forward-transfer`; `UPDATE_FIXTURES=1` regenerates `fixtures/`; the Rust fixture test (Task 5 file, `typescript_compiled_fixtures_run_end_to_end`) gains both names and runs them.
- [ ] **Step 6: Commit** `Compile row inputs and account groups in the TypeScript SDK`.

### Task 7: Rust client and examples

**Files:** Modify `clients/rust/src/lib.rs`, `clients/rust/examples/*.rs` where they build payroll runs.

- [ ] **Step 1** `RunInputs::groups(mut self, lengths: &[u8]) -> Self` asserting it is called on an empty encoder; doc: row values follow fixed values in iteration order.
- [ ] **Step 2** Doctest encoding `[2, 0]` prefix then a `u64`.
- [ ] **Step 3** Commit `Encode account group lengths in the Rust client`.

### Task 8: Documentation

- [ ] `docs/guide/batching.md`: row inputs section; payroll example pays a different amount per row in both languages.
- [ ] `docs/guide/account-groups.md` (new, sidebar under Build templates): what a group is, the three-swap template, the signer rule, limits.
- [ ] `docs/reference/limits.md`: 120 slots, 8 row inputs, 256 values, 8 groups. `docs/reference/wire-format.md`: header bytes 21/22, descriptor byte 1, run data layout. `docs/guide/errors-and-events.md`: 6021, verifier code 6128.
- [ ] Commit `Document row inputs and account groups`.

### Task 9: Version 1

- [ ] `TEMPLATE_PROGRAM_VERSION = 1`, magic `BVM1` (Rust `wire.rs`, TS compiler header bytes, `no_panic.rs` representative program, docs), template PDA seed `template` (`programs/ballista/src/utils/pda.rs`, `clients/rust/src/lib.rs`, `clients/js/src/kit.ts`, `tests/ballista/src/lib.rs`, docs), crate and package versions `1.0.0`, docs nav `1.0`, TS schema `version: z.literal(1)`.
- [ ] Regenerate fixtures; Rust suites pass; regenerate Certora constants via `cargo test -p ballista-specs --features rt` (the tests print replacements); update Certora rules for the new `validate_runtime_accounts` signature and rule list.
- [ ] Commit `Name the completed bytecode version 1`.

### Task 10: Prover follow-up (time-boxed)

- [ ] Restrict `rule_i64_arithmetic_is_checked` to add/sub/mul/min/max and add `rule_i64_division_reports_zero_and_overflow` that checks only the error conditions (the prover has no model of `__divdi3`).
- [ ] Run once without `-solanaTACMathInt` for the u128 rule; keep whichever setting proves more.
- [ ] Record results in `docs/guide/formal-verification.md` and `certora/README.md`.
