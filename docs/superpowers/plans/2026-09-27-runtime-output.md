# Runtime output: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to carry out this plan task by task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add two opcodes to the Ballista VM. `EMIT` (62) logs data with `sol_log_data`, and
`SET_RETURN_DATA` (63) sets the run's return data. Both encode their parts exactly as CPI data is
encoded. Each is verified at finalize, executed on chain, and authorable from TypeScript
(`step.emit`, `step.setReturnData`) and from the Rust `ProgramBuilder`. Each is covered end to end
under Mollusk: the log is read back through a log collector, and the return data is read both
directly and by a nested template through `RETURN_DATA`.

**Architecture:**
- **Addressing.** Both opcodes name a run of the shared data-segment table in their immediate, the
  way `DERIVE_PDA` names its seeds. Neither writes a register.
- **Length.** The verifier sums the segments' worst-case widths with the rule that PDA seeds and CPI
  data already use, and caps the sum at 1,024 bytes (`MAX_RETURN_DATA_LEN`).
- **Placement.** The runtime clears return data at the start of every program invocation, CPIs
  included (Agave `invoke_context.rs`, `process_executable_chain`).
  - So `SET_RETURN_DATA` may appear once, outside every loop, with no `INVOKE` at any later index.
  - `EMIT` may appear anywhere, loops included.
- **Buffer.** The executor encodes into a dedicated buffer in `Scratch`, never into `Scratch::data`.
  - Inside a batch, `data` can hold a loop-invariant CPI payload that the next row sends without
    re-encoding it.
  - The buffer is `None` until the run's first output allocates it at 1,024 bytes.
- **Dispatch.** The dispatch loop's outer match does not change. Both opcodes reach one
  `#[inline(never)]` helper, `write_output`, through the fallback arm of phase 1's
  `extended_instruction`, so that match keeps its shape too.
- **TypeScript.** The compiler emits the same records as `ProgramBuilder::emit_data` and
  `ProgramBuilder::set_return_data`, and it enforces the placement rules at compile time.

**Tech stack:**
- Rust (no_std SBF program on pinocchio 0.11, shared `ballista-common` crate).
- TypeScript SDK (Zod, vitest).
- Mollusk 0.14 integration suite; Certora specs.

**Base and dependencies:**
- **Phase 1 (math) must be finished** on `claude/runtime-extensions`.
  - That includes its dispatch refactor: `extended_instruction` takes
    `(machine: &mut Machine, instruction, loop_context)`, and its fallback arm returns
    `InvalidTemplateProgram`.
  - Task 0 checks this, and Task 8, the first task to need it, checks again.
- **This phase depends on phase 2 (loops).** The numbers are fixed across the three parallel
  phases:
  - Phase 2 owns opcode 61 (`REPEAT`) and verifier code 6129 (`InvalidLoop`).
  - This phase owns opcodes 62–63 and verifier code 6130 (`InvalidOutput`).
  - Phase 4 owns opcodes 64–74 and verifier code 6131.
  - Tasks 1–4 touch no error table and can run while phase 2 is still in progress.
  - Task 5 rebases this branch onto the finished loops branch. Tasks 6–12 need it.
  - **Never merge this branch ahead of phase 2.**
- **Branch and paths.** The branch is `claude/runtime-output`, in the worktree
  `.claude/worktrees/runtime-output`.
  - Paths below are relative to the worktree root.
  - Line numbers are from `claude/runtime-extensions` at `dedc168`, and they will drift. Search for
    the quoted code.
- **Out of scope:**
  - Documentation. Phase 5 updates `language.md`, `wire-format.md`, `errors-and-events.md` and the
    other pages.
  - Any change to the run event, the program header, or the bytecode version.

**Conventions:**
- Keep the surrounding style: doc comments that explain why, `#[inline(never)]` for heavy helpers
  reached from the dispatch loop, and `RunResult`/`BallistaError` for failures.
- Commit after each task. Every commit message ends with a `Co-Authored-By` trailer that names the
  model writing the commit. The commands below show `Claude Opus 5.5`; an implementer running as
  another model writes its own name.
- Never use `git stash`: the stash is shared with every other worktree. To set work aside, commit
  it.
- **Compute-unit ceilings** are raised only by hand, to the exact measured values. The commit
  message carries a before/after table, and `benches/CHANGELOG.md` gets an entry. Never run a script
  that rewrites every ceiling.

---

## What the prototype settled

A prototype of this plan ran on phase 1's tip, in a scratch copy of the repository:
- phase 2's error code was stubbed at 6129;
- phase 1's `Machine`-shaped `extended_instruction` was simulated.

The code in this plan is the code that passed there. Every host suite, the SBF build, the Mollusk
suite, and the Certora typecheck and frame check were green. It was measured against phase 1's
ceilings:

| Choice | What the alternative cost |
| --- | --- |
| The buffer is `Option<&'data mut Vec<u8>>`, leaked on first use | Every run pays to initialize the field. A null reference is 1 CU. An `Option<Vec<u8>>` is 2, because its `None` is a 64-bit immediate. An empty `Vec<u8>` is 3–4 |
| Outputs encode through their own sink type, `OutputSink` | Through the CPI data's `Vec<u8>` sink, the segment encoder gained a second caller, and the compiler stopped inlining it into `invoke_cpi`. Every invocation then cost about 50 CU more, and `index-weighted-rewards` cost about 1,600 more. `#[inline(always)]` on both encoders still cost about 13 per invocation |
| One helper, `write_output`, behind `extended_instruction`'s fallback arm | Arms of their own in that match cost every math opcode about 7 CU. Two helpers, one per opcode, gave the encoder two callers again, and each output cost about 65 CU more |
| Result | +1 CU on every run and every cookbook example, and +1 on the math fixture. A new case that logs 20 bytes and returns 16 costs 1,505 |

**Verifier.** The verifier checks the new records strictly, because new opcodes can afford it:
- `dst`, `a`, `b` and `c` must be `NO_INDEX`;
- the segment range must be non-empty and in bounds;
- every segment must be in canonical form.

Canonical form is the PDA-seed rule: a literal names no register, and a register segment has no
offset or length. `verify_pda_seed_segment` already implements that rule; this plan renames it
`verify_segment` and reuses it.

**A compiler bug, fixed in Task 3.** A `pda` expression inside `invoke` data pushes its seed segments
into the middle of the invocation's own run of segments. When the widths happen to match, the
verifier accepts the result, and the CPI then sends the seed instead of the derived address. The
prototype built such a template and confirmed that it verifies.

---

## File map

| File | Change |
| --- | --- |
| `common/src/template/wire.rs` | `OP_EMIT = 62`, `OP_SET_RETURN_DATA = 63`; `TemplateError::InvalidOutput` (index 30, code 6130) and its name; tests |
| `common/src/template/builder.rs` | `emit_data`, `set_return_data`; test |
| `common/src/template/verify.rs` | `verify_output`; `SET_RETURN_DATA` placement; `verify_pda_seed_segment` renamed `verify_segment`; tests; fixture list |
| `common/src/template/generate.rs` | An `EMIT` arm, a `SET_RETURN_DATA` at the end of the root, and `output_part` |
| `programs/ballista/src/processor/execute.rs` | The `Scratch::output` buffer; `write_output` behind `extended_instruction`'s fallback arm; `OutputSink`; tests; the unknown-opcode sweep |
| `programs/ballista/src/profile.rs`, `programs/ballista/src/lib.rs` | Comments: a profiling build's record replaces the run's return data |
| `clients/js/src/compiler.ts` | Opcode entries; `compileDataParts`; `compileOutput`; placement rules |
| `clients/js/src/schema.ts` | `emit` and `setReturnData` step kinds and constructors |
| `clients/js/src/opcodes.test.ts` | Parity entries |
| `clients/js/src/compiler.test.ts` | A segment-table helper; "data segments" and "output steps" tests |
| `clients/js/src/errors.ts`, `clients/js/src/errors.test.ts` | `InvalidOutput`; the first unused code becomes 6131 |
| `clients/js/src/fixtures.test.ts` | The `output` fixture |
| `clients/rust/src/lib.rs` | The decode test covers 6130 and 6131 |
| `fixtures/output.hex`, `fixtures/manifest.json`, `fixtures/verifier-error-names.txt` | Regenerated by `pnpm fixtures` |
| `fixtures/cu-ceilings.json`, `fixtures/example-ceilings.json` | Raised by hand; one new case |
| `tests/ballista/Cargo.toml`, `tests/ballista/Cargo.lock` | Direct `base64` and `solana-svm-log-collector` dependencies |
| `tests/ballista/src/lib.rs` | Log helper; the event test reads its log; output fixture, nested return data, create-time rejection; the math fixture prints its units |
| `tests/ballista/src/cases.rs` | Ceiling case `run, log and return 16 bytes` |
| `certora/ballista-specs/src/rules/util.rs` | `writes_destination` excludes the outputs; test; ABI comment |
| `certora/ballista-specs/src/rules/errors.rs` | `InvalidOutput` in the verifier-code rule |
| `certora/ballista-specs/envs/cvlr_inlining.txt`, `cvlr_summaries.txt` | `OutputSink::push_bytes` stays external, as the other sinks do |
| `benches/CHANGELOG.md` | An entry for the output buffer's cost |

---

### Task 0: Worktree and baseline

- [ ] **Step 1: Check that phase 1 is finished and has the `Machine`-shaped helper.** Run this, and
  Step 2, from the repository root.

```bash
git -C .claude/worktrees/runtime-extensions status --short
git -C .claude/worktrees/runtime-extensions log --oneline -3
grep -n -A4 "^fn extended_instruction" .claude/worktrees/runtime-extensions/programs/ballista/src/processor/execute.rs
grep -n -A8 "^struct Machine" .claude/worktrees/runtime-extensions/programs/ballista/src/processor/execute.rs
```

Expected:
- A clean tree.
- Phase 1's last task committed.
- A helper signature that begins `fn extended_instruction<'data>(` and whose next line is
  `machine: &mut Machine<'_, 'data>,`.
- A `Machine` that still has the fields `program`, `registers` and `scratch`.

Tasks 1–7 do not touch the executor. If the helper still takes
`program, accounts, registers, instruction, loop_context`, phase 1 has not landed its refactor yet.
You can go on, but Task 8 checks again and stops if the refactor is still missing.

- [ ] **Step 2: Create the worktree.** Use superpowers:using-git-worktrees.
  - If the loops branch is already finished (Task 5, Step 1 shows how to tell), create the branch
    from `claude/runtime-loops` instead. Task 5 then skips its rebase (Steps 2 and 3) but keeps the
    rest.
  - Otherwise, create it from phase 1:

```bash
git worktree add .claude/worktrees/runtime-output -b claude/runtime-output claude/runtime-extensions
```

- [ ] **Step 3: Install and build.**

```bash
cd .claude/worktrees/runtime-output
pnpm install --frozen-lockfile
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
```

- [ ] **Step 4: Run the baseline. Everything must be green before any change.**

```bash
pnpm fixtures && git diff --exit-code fixtures
pnpm check
pnpm test
cargo test --manifest-path tests/ballista/Cargo.toml
cargo check -p ballista-specs --features rt --manifest-path certora/Cargo.toml
cargo clippy -p ballista -p ballista-common --all-targets 2>&1 | grep -E "^(warning|error)" | sort | uniq -c
```

Expected: every command exits 0. Keep the clippy summary; Task 12 compares against it.

If anything fails on the untouched base, stop and report it. Do not fix unrelated failures silently.

- [ ] **Step 5: Commit this plan.**

```bash
mkdir -p docs/superpowers/plans
cp /private/tmp/claude-501/-Users-jacob-Documents-ballista/06470e69-8486-4adc-b478-7e3c20bf99aa/scratchpad/plans/2026-09-27-runtime-output.md docs/superpowers/plans/
git add docs/superpowers/plans/2026-09-27-runtime-output.md
git commit -F- <<'EOF'
Plan the runtime output opcodes

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 1: Opcode numbers in Rust and TypeScript

**Files:**
- Modify: `common/src/template/wire.rs`, directly after `pub const OP_READ_I32: u8 = 60;` (line
  159).
- Modify: `clients/js/src/compiler.ts`, the `opcode` table, after `readI32: 60,` (line 107).
- Modify: `clients/js/src/opcodes.test.ts`, the `rustName` table.

- [ ] **Step 1: Write the failing test.** In `opcodes.test.ts`, add these lines to `rustName` after
  `readI32: 'OP_READ_I32',`:

```ts
  emit: 'OP_EMIT',
  setReturnData: 'OP_SET_RETURN_DATA',
```

- [ ] **Step 2: Run it and confirm that it fails.**

Run: `pnpm --dir clients/js exec vitest run src/opcodes.test.ts`

Expected: FAIL with
`AssertionError: expected [ 'OP_ACCOUNT_DATA_LEN', …(58) ] to deeply equal [ 'OP_ACCOUNT_DATA_LEN', …(60) ]`.
If you branched from the loops branch, both counts are one higher. Vitest does not type-check, so
the two new keys fail here as a runtime assertion rather than a type error.

- [ ] **Step 3: Add the constants.** In `wire.rs`, directly after `OP_READ_I32`:

```rust
/// Encodes the data segments the immediate names, as CPI data is encoded, and logs the bytes with
/// `sol_log_data` as one field. Writes no register.
pub const OP_EMIT: u8 = 62;
/// Encodes the data segments the immediate names and sets the bytes as the run's return data.
/// Allowed once, outside every loop, after the last invoke. Writes no register.
pub const OP_SET_RETURN_DATA: u8 = 63;
```

In `compiler.ts`, after `readI32: 60,`:

```ts
  emit: 62,
  setReturnData: 63,
```

- [ ] **Step 4: Run it again.**

Run: `pnpm --dir clients/js exec vitest run src/opcodes.test.ts && cargo check -p ballista-common`

Expected: PASS, then `Finished`.

The verifier still rejects 62 and 63 as unknown opcodes until Task 7. The executor still fails them
until Task 8.

- [ ] **Step 5: Commit.**

```bash
git add common/src/template/wire.rs clients/js/src/compiler.ts clients/js/src/opcodes.test.ts
git commit -F- <<'EOF'
Number the output opcodes in Rust and TypeScript

EMIT is 62 and SET_RETURN_DATA is 63. The loops phase owns 61 and the
introspection phase 64 to 74.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 2: Builder helpers

**Files:** modify `common/src/template/builder.rs`:
- the methods, directly before `/// Mutable access to the emitted instructions, for negative tests.`
  (line 390);
- the test, directly before `fn oversized_payloads_are_rejected_at_build_time` (line 613).

`ProgramBuilder::emit` already appends a raw record, so the `EMIT` helper is called `emit_data`.

- [ ] **Step 1: Write the failing test.** In the `tests` module of `builder.rs`, before
  `#[test] fn oversized_payloads_are_rejected_at_build_time`:

```rust
    #[test]
    fn output_helpers_push_their_parts_and_write_no_register() {
        let mut builder = ProgramBuilder::new();
        let amount = builder.const_u64(7);
        let flag = builder.const_bool(true);
        let tag = builder.blob(b"TAG");
        let logged = builder.emit_data(&[
            Segment::Literal(tag),
            Segment::Register(DATA_REG_U64, amount),
        ]);
        let returned = builder.set_return_data(&[Segment::Register(DATA_REG_BOOL, flag)]);
        assert_eq!((logged, returned), (2, 3));

        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        assert_eq!(program.header.register_count(), 2, "outputs allocate no register");
        let emit = &program.instructions[logged];
        assert_eq!(emit.opcode, OP_EMIT);
        assert_eq!(
            [emit.dst, emit.a, emit.b, emit.c, emit.flags],
            [NO_INDEX, NO_INDEX, NO_INDEX, NO_INDEX, 0]
        );
        assert_eq!(emit.blob_range(), (0, 2), "segments 0 and 1");
        let set = &program.instructions[returned];
        assert_eq!(set.opcode, OP_SET_RETURN_DATA);
        assert_eq!(set.dst, NO_INDEX);
        assert_eq!(set.blob_range(), (2, 1), "segment 2");
        let kinds: Vec<(u8, u8)> = program
            .data_segments
            .iter()
            .map(|segment| (segment.kind, segment.register))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (DATA_LITERAL, NO_INDEX),
                (DATA_REG_U64, amount),
                (DATA_REG_BOOL, flag)
            ]
        );
    }
```

- [ ] **Step 2: Run it and confirm that it fails.**

Run: `cargo test -p ballista-common --lib builder`

Expected: compile errors, ``error[E0599]: no method named `emit_data` found for struct `ProgramBuilder` ``,
and the same for `set_return_data`.

- [ ] **Step 3: Implement.** Directly before
  `/// Mutable access to the emitted instructions, for negative tests.`:

```rust
    /// Pushes `parts` and emits an `EMIT`, which logs their encoding as one `Program data:` field.
    /// Returns the instruction's index. (`emit` itself appends a raw record.)
    pub fn emit_data(&mut self, parts: &[Segment]) -> usize {
        self.output(OP_EMIT, parts)
    }

    /// Pushes `parts` and emits a `SET_RETURN_DATA`, which makes their encoding the run's return
    /// data. Returns the instruction's index.
    pub fn set_return_data(&mut self, parts: &[Segment]) -> usize {
        self.output(OP_SET_RETURN_DATA, parts)
    }

    fn output(&mut self, opcode: u8, parts: &[Segment]) -> usize {
        let start = self.segments.len() as u16;
        for part in parts {
            self.push_segment(*part);
        }
        self.emit(record(
            opcode,
            NO_INDEX,
            NO_INDEX,
            NO_INDEX,
            NO_INDEX,
            0,
            range_immediate(start, parts.len() as u16),
        ))
    }
```

- [ ] **Step 4: Run the tests.**

Run: `cargo test -p ballista-common --lib builder`

Expected: PASS, 4 tests, including `output_helpers_push_their_parts_and_write_no_register`.

- [ ] **Step 5: Commit.**

```bash
git add common/src/template/builder.rs
git commit -F- <<'EOF'
Build output instructions with ProgramBuilder

emit_data and set_return_data push their parts as data segments and
name them by range, the way derive_pda names its seeds. Neither writes
a register.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 3: Keep a step's data segments contiguous

**Files:**
- Modify: `clients/js/src/compiler.ts`:
  - `compileInvoke` (the loop at line 562);
  - a new method, `compileDataParts`, before `compileDataPart` (line 609).
- Test: `clients/js/src/compiler.test.ts`, directly before `describe('math expressions', () => {`
  (line 870).

This fixes the live bug described under "What the prototype settled". The output steps in Task 4
use the same helper.

- [ ] **Step 1: Write the failing test.** In `compiler.test.ts`, directly before
  `describe('math expressions', () => {`:

```ts
/** The data segments (kind and register) and each CPI descriptor's segment range, from the bytes. */
function segmentTables(compiled: CompiledTemplate) {
  const view = new DataView(compiled.bytes.buffer, compiled.bytes.byteOffset);
  const cpiStart = instructionOffset(compiled, compiled.stats.instructions);
  const cpiAccounts = view.getUint16(12, true);
  const segmentStart = cpiStart + compiled.stats.cpis * 12 + cpiAccounts * 2;
  const segments = Array.from({ length: view.getUint16(14, true) }, (_, index) => ({
    kind: compiled.bytes[segmentStart + index * 8]!,
    register: compiled.bytes[segmentStart + index * 8 + 1]!,
  }));
  const cpis = Array.from({ length: compiled.stats.cpis }, (_, index) => ({
    start: view.getUint16(cpiStart + index * 12 + 6, true),
    length: compiled.bytes[cpiStart + index * 12 + 5]!,
  }));
  return { segments, cpis };
}

describe('data segments', () => {
  test('an invocation part that derives a PDA leaves the invocation its own segments', () => {
    const compiled = compileTemplate(
      defineTemplate({
        accounts: { program: { executable: true, address: address(9) }, owner: {} },
        steps: [
          step.invoke({
            program: account.fixed('program'),
            accounts: [],
            data: [
              data.encode(
                'pubkey',
                expression.pda(account.fixed('program'), [expression.accountField(account.fixed('owner'), 'key')]),
              ),
            ],
          }),
        ],
      }),
    );
    const { segments, cpis } = segmentTables(compiled);
    // Register 0 is the owner's key, the PDA's only seed; register 1 is the derived address.
    expect(segments.slice(cpis[0]!.start, cpis[0]!.start + cpis[0]!.length)).toEqual([{ kind: 7, register: 1 }]);
  });
});
```

- [ ] **Step 2: Run it and confirm that it fails.**

Run: `pnpm --dir clients/js exec vitest run src/compiler.test.ts -t "data segments"`

Expected: FAIL with
`AssertionError: expected [ { kind: 7, register: +0 } ] to deeply equal [ { kind: 7, register: 1 } ]`.
The invocation's range names the seed.

- [ ] **Step 3: Implement.** In `compiler.ts`, add this method directly before `compileDataPart`:

```ts
  /**
   * Compiles a step's data parts, then appends their segments as one contiguous run and returns
   * where it starts. A part can push segments of its own while it compiles (a `pda` expression's
   * seeds), so appending each part's segment as soon as it compiled would leave those seeds inside
   * the step's range, and the step would encode a seed where it meant the part.
   */
  compileDataParts(parts: DataPart[], inLoop: boolean, bindings: Bindings): { segmentStart: number; maxLength: number } {
    const compiled = parts.map((part) => this.compileDataPart(part, inLoop, bindings));
    const segmentStart = this.dataSegments.length;
    for (const { record } of compiled) this.dataSegments.push(record);
    return { segmentStart, maxLength: compiled.reduce((total, { maxLength }) => total + maxLength, 0) };
  }
```

In `compileInvoke`, replace:

```ts
    const segmentStart = this.dataSegments.length;
    let maxDataLength = 0;
    for (const part of current.data) {
      const result = this.compileDataPart(part, inLoop, bindings);
      this.dataSegments.push(result.record);
      maxDataLength += result.maxLength;
    }
```

with:

```ts
    const { segmentStart, maxLength: maxDataLength } = this.compileDataParts(current.data, inLoop, bindings);
```

- [ ] **Step 4: Run the tests. Then confirm that no fixture changed.**

Run: `pnpm --dir clients/js exec vitest run src/compiler.test.ts -t "data segments"`

Expected: PASS.

Run: `pnpm fixtures && git diff --exit-code fixtures && pnpm --dir clients/js check`

Expected: exit 0.
- No shared fixture and no protocol example puts a `pda` inside invocation data, so every payload
  stays byte-identical.
- The instruction order and the blob order do not change either.

- [ ] **Step 5: Commit.**

```bash
git add clients/js/src/compiler.ts clients/js/src/compiler.test.ts
git commit -F- <<'EOF'
Keep each invocation's data segments contiguous when a part derives a PDA

A pda expression pushes its seed segments while it compiles. Pushed in
the middle of an invocation's parts, they became part of the
invocation's range. The verifier accepted the result whenever the widths
matched, and the CPI then sent the seed in place of the derived address.
Parts now compile first and append their segments together.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 4: `step.emit` and `step.setReturnData`

**Files:**
- Modify: `clients/js/src/schema.ts`:
  - the `Step` type, after the `invoke` variant (line 289);
  - `StepSchema`, after the `invoke` object;
  - the `step` constructors, after `invoke` (line 532).
- Modify: `clients/js/src/compiler.ts`:
  - a class field after `maxCpiDataLength = 0;` (line 321);
  - `compileSteps` (line 531);
  - `compileInvoke`;
  - a new method, `compileOutput`, before `compileReturnData`.
- Test: `clients/js/src/compiler.test.ts`, directly after the `describe('data segments', …)` block.

- [ ] **Step 1: Write the failing tests.** In `compiler.test.ts`, directly after the
  `describe('data segments', …)` block:

```ts
describe('output steps', () => {
  const compileSteps = (inputs: TemplateInput['inputs'], steps: Step[], batch?: TemplateInput['batch']) =>
    compileTemplate(
      defineTemplate({
        inputs,
        accounts: {
          systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
          payer: { signer: true, writable: true },
        },
        ...(batch ? { batch } : {}),
        steps,
      }),
    );
  const pay = (to: ReturnType<typeof account.fixed>) =>
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('payer'),
      to,
      lamports: expression.u64(1),
    });
  const rows = { maxIterations: 2, row: { recipient: { writable: true } } };

  test('emit and setReturnData lower to one record naming a contiguous run of their parts', () => {
    const compiled = compileSteps({ amount: { type: 'u64' } }, [
      step.emit([data.literal(Uint8Array.of(1, 2)), data.encode('u64', expression.input('amount'))], 'log'),
      step.setReturnData([data.encode('pubkey', expression.accountField(account.fixed('payer'), 'key'))], 'result'),
    ]);
    const outputs = records(compiled).filter(
      (record) => record[0] === opcode.emit || record[0] === opcode.setReturnData,
    );
    // Opcode, then dst, a, b, c and flags: an output names no register and takes no flag.
    expect(outputs.map((record) => [...record.slice(0, 6)])).toEqual([
      [opcode.emit, 0xff, 0xff, 0xff, 0xff, 0],
      [opcode.setReturnData, 0xff, 0xff, 0xff, 0xff, 0],
    ]);
    // The immediate is (first segment, segment count), two little-endian u32s.
    expect([...outputs[0]!.slice(6, 14)]).toEqual([0, 0, 0, 0, 2, 0, 0, 0]);
    expect([...outputs[1]!.slice(6, 14)]).toEqual([2, 0, 0, 0, 1, 0, 0, 0]);
    expect(segmentTables(compiled).segments).toEqual([
      { kind: 0, register: 0xff },
      { kind: 4, register: 0 },
      { kind: 7, register: 1 },
    ]);
    expect(compiled.sourceMap.filter((entry) => entry.label !== undefined)).toEqual([
      { pc: 1, path: 'steps[0]', label: 'log' },
      { pc: 2, path: 'steps[1]', label: 'result' },
      { pc: 3, path: 'steps[1]', label: 'result' },
    ]);
  });

  test('emit may appear anywhere, loops and invokes included', () => {
    expect(() =>
      compileSteps(
        {},
        [
          step.emit([data.literal(Uint8Array.of(1))]),
          step.forEach([
            step.emit([data.encode('u64', expression.loopIndex())]),
            pay(account.iteration('recipient')),
            step.emit([data.encode('u64', expression.loopIndex())]),
          ]),
          step.emit([data.literal(Uint8Array.of(2))]),
        ],
        rows,
      ),
    ).not.toThrow();
  });

  test('setReturnData comes once, outside every loop, after every invoke', () => {
    const result = step.setReturnData([data.literal(Uint8Array.of(1))]);
    expect(() => compileSteps({}, [pay(account.fixed('payer')), result])).not.toThrow();
    expect(() => compileSteps({}, [step.forEach([pay(account.iteration('recipient'))]), result], rows)).not.toThrow();
    expect(() => compileSteps({}, [step.forEach([result])], rows)).toThrow(
      'setReturnData is not allowed inside a loop',
    );
    expect(() => compileSteps({}, [result, result])).toThrow('setReturnData may appear only once');
    expect(() => compileSteps({}, [result, pay(account.fixed('payer'))])).toThrow(
      'invoke cannot follow setReturnData',
    );
    expect(() => compileSteps({}, [result, step.forEach([pay(account.iteration('recipient'))])], rows)).toThrow(
      'invoke cannot follow setReturnData',
    );
  });

  test('an output encodes at most 1,024 bytes, counting a bytes value at its maximum length', () => {
    const memo = { memo: { type: 'bytes', maxLength: 1024 } } as const;
    expect(() => compileSteps(memo, [step.setReturnData([data.encode('bytes', expression.input('memo'))])])).not.toThrow();
    expect(() =>
      compileSteps(memo, [
        step.emit([data.encode('bytes', expression.input('memo')), data.literal(Uint8Array.of(0))]),
      ]),
    ).toThrow('emit can encode 1025 bytes; maximum is 1024');
    expect(() => compileSteps({}, [step.setReturnData([data.literal(new Uint8Array(1025))])])).toThrow(
      'setReturnData can encode 1025 bytes; maximum is 1024',
    );
    expect(() => compileSteps({}, [step.emit([])])).toThrow();
  });
});
```

`records`, `segmentTables`, `opcode`, `systemTransfer`, `data`, `SYSTEM_PROGRAM_ADDRESS_BYTES` and the
`Step` and `TemplateInput` types are already in scope in this file.

- [ ] **Step 2: Run them and confirm that they fail.**

Run: `pnpm --dir clients/js exec vitest run src/compiler.test.ts -t "output steps"`

Expected: FAIL, all 4 tests, with `TypeError: step.emit is not a function` and
`TypeError: step.setReturnData is not a function`.

- [ ] **Step 3: Extend the schema** in `schema.ts`.

  - In the `Step` type, directly after the `invoke` variant (whose last field is `label?: string;`
    after `programAddress?: Uint8Array;`), add:

    ```ts
      | {
          /** Logs the encoded parts as one `Program data:` field. */
          kind: 'emit';
          parts: DataPart[];
          label?: string;
        }
      | {
          /**
           * Sets the encoded parts as the run's return data. Once per template, outside every loop, and
           * after the last invoke, because invoking a program clears return data.
           */
          kind: 'setReturnData';
          parts: DataPart[];
          label?: string;
        }
    ```

  - In `StepSchema`, directly after the `invoke` object's `.strict(),`:

    ```ts
        z.object({ kind: z.literal('emit'), parts: z.array(DataPartSchema).min(1).max(64), label }).strict(),
        z.object({ kind: z.literal('setReturnData'), parts: z.array(DataPartSchema).min(1).max(64), label }).strict(),
    ```

  - In `step`, directly after `invoke: …,`:

    ```ts
      /** Logs the parts, encoded as invocation data is, as one `Program data:` field. */
      emit: (parts: DataPart[], label?: string): Step => ({ kind: 'emit', parts, ...(label ? { label } : {}) }),
      /** Sets the parts, encoded as invocation data is, as the run's return data. */
      setReturnData: (parts: DataPart[], label?: string): Step => ({
        kind: 'setReturnData',
        parts,
        ...(label ? { label } : {}),
      }),
    ```

- [ ] **Step 4: Extend the compiler** in `compiler.ts`.

  - After `maxCpiDataLength = 0;`:

    ```ts
      /** Whether a `setReturnData` step has compiled; no invoke may follow it. */
      returnDataSet = false;
    ```

  - In `compileSteps`, replace the final branch:

    ```ts
          } else {
            this.compileInvoke(current, inLoop, bindings);
          }
    ```

    with:

    ```ts
          } else if (current.kind === 'emit' || current.kind === 'setReturnData') {
            this.compileOutput(current, inLoop, bindings);
          } else {
            this.compileInvoke(current, inLoop, bindings);
          }
    ```

  - At the start of `compileInvoke`'s body:

    ```ts
        if (this.returnDataSet) {
          throw new TypeError('invoke cannot follow setReturnData: invoking a program clears the return data');
        }
    ```

  - Directly before `compileReturnData`:

    ```ts
      /** EMIT and SET_RETURN_DATA: the parts are encoded as invocation data is, to at most 1,024 bytes. */
      compileOutput(current: Extract<Step, { kind: 'emit' | 'setReturnData' }>, inLoop: boolean, bindings: Bindings): void {
        if (current.kind === 'setReturnData') {
          // Solana clears return data whenever a program is invoked, so what a run returns is set
          // once, outside every loop, after its last invoke.
          if (inLoop) throw new TypeError('setReturnData is not allowed inside a loop');
          if (this.returnDataSet) throw new TypeError('setReturnData may appear only once');
          this.returnDataSet = true;
        }
        const { segmentStart, maxLength } = this.compileDataParts(current.parts, inLoop, bindings);
        if (maxLength > MAX_RETURN_DATA_LENGTH) {
          throw new RangeError(`${current.kind} can encode ${maxLength} bytes; maximum is ${MAX_RETURN_DATA_LENGTH}`);
        }
        const operation = current.kind === 'emit' ? opcode.emit : opcode.setReturnData;
        this.pushInstruction(
          instructionRecord(operation, NO_INDEX, NO_INDEX, NO_INDEX, NO_INDEX, rangeImmediate(segmentStart, current.parts.length)),
        );
      }
    ```

  Steps compile in order, and loop bodies compile in place, so "an invoke compiled after
  `setReturnData`" is exactly "an invoke at a later program counter". The verifier enforces the same
  rule in Task 7.

- [ ] **Step 5: Run the tests.**

Run: `pnpm --dir clients/js exec vitest run src/compiler.test.ts -t "output steps"`

Expected: PASS, 4 tests.

Run: `pnpm --dir clients/js check && pnpm fixtures && git diff --exit-code fixtures`

Expected: exit 0.

- [ ] **Step 6: Commit.**

```bash
git add clients/js/src/schema.ts clients/js/src/compiler.ts clients/js/src/compiler.test.ts
git commit -F- <<'EOF'
Author emit and setReturnData steps from TypeScript

Both take parts built with data.literal and data.encode, as invoke data
is, up to 1,024 bytes. setReturnData compiles once, outside every loop,
and no invoke may compile after it: invoking a program clears return
data.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 5: Rebase onto the loops phase

Tasks 1–4 touched these files:
- `wire.rs`, `builder.rs`;
- `compiler.ts`, `opcodes.test.ts`, `schema.ts`, `compiler.test.ts`;
- and the plan itself.

Phase 2 edits most of the same places. The conflicts that follow are mechanical.

- [ ] **Step 1: Find the loops branch and confirm that it is finished.**

```bash
git branch --list 'claude/runtime-*'
git show claude/runtime-loops:common/src/template/wire.rs | grep -n -E "OP_REPEAT: u8 = 61|InvalidLoop"
git show claude/runtime-loops:fixtures/verifier-error-names.txt | tail -2
git log --oneline -3 claude/runtime-loops
```

Expected:
- The branch exists. If phase 2 used another name, use that name everywhere below.
- `wire.rs` defines `OP_REPEAT: u8 = 61` and an `InvalidLoop` variant.
- The names file ends with `TooManyAccountGroups` and then `InvalidLoop`.
- The log shows the final commit of phase 2's plan.

If phase 2 is not finished, stop here and report. Tasks 6–12 cannot start without code 6129.

- [ ] **Step 2: Rebase.**

```bash
git rebase claude/runtime-loops
```

- [ ] **Step 3: Resolve each conflict by keeping both sides.** Then run
  `git add <file> && git rebase --continue`.

| File | Resolution |
| --- | --- |
| `common/src/template/wire.rs` | The opcode constants go in numeric order: `OP_READ_I32` (60), phase 2's `OP_REPEAT` (61), `OP_EMIT` (62), `OP_SET_RETURN_DATA` (63) |
| `clients/js/src/compiler.ts`, `opcode` table | `readI32: 60,`, phase 2's entry for 61, then `emit: 62,` and `setReturnData: 63,` |
| `clients/js/src/opcodes.test.ts` | Phase 2's `rustName` entry for `OP_REPEAT`, then `emit` and `setReturnData` |
| `common/src/template/builder.rs` | Keep phase 2's helpers and tests, and `emit_data`, `set_return_data`, `output` and `output_helpers_push_their_parts_and_write_no_register` |
| `clients/js/src/schema.ts` | Keep phase 2's `repeat` variant, schema object and constructor, and the `emit` and `setReturnData` ones |
| `clients/js/src/compiler.ts`, class fields and `compileSteps` | Keep phase 2's loop branches. The `emit`/`setReturnData` branch stays directly before the final `else` that calls `compileInvoke` |
| `clients/js/src/compiler.test.ts` | Keep every `describe` block. If both sides added a helper with the same name, rename this branch's helper and its uses |

- [ ] **Step 4: Fit the loop test in `compileOutput` to phase 2's compiler.**
  - The rule is that `setReturnData` is rejected inside *any* loop, count loops included.
  - If phase 2 kept a single `inLoop: boolean` that is true inside both kinds of loop, nothing
    changes.
  - If phase 2 split it (for example into "inside a loop" and "inside a row loop"), do two things:
    - pass `compileOutput` the same arguments that `compileInvoke` now receives;
    - make its first test the condition that `loopIndex` uses to accept itself, because `loopIndex`
      is valid in every loop.

- [ ] **Step 5: Add a count-loop test.** Add it to the `describe('output steps', …)` block in
  `compiler.test.ts`. It uses phase 2's constructor, `step.repeat(count, steps, { max, carry?, label? })`
  per the spec. If phase 2's signature differs, adapt the call, not the assertions.

```ts
  test('setReturnData stays out of count loops, and emit may run in them', () => {
    const count = { n: { type: 'u64' } } as const;
    expect(() =>
      compileSteps(count, [step.repeat(expression.input('n'), [step.setReturnData([data.literal(Uint8Array.of(1))])], { max: 2 })]),
    ).toThrow('setReturnData is not allowed inside a loop');
    expect(() =>
      compileSteps(count, [step.repeat(expression.input('n'), [step.emit([data.encode('u64', expression.loopIndex())])], { max: 2 })]),
    ).not.toThrow();
  });
```

- [ ] **Step 6: Run the checks.**

```bash
pnpm install --frozen-lockfile
pnpm --dir clients/js check
pnpm fixtures && git diff --exit-code fixtures
cargo test -p ballista-common -p ballista --lib
```

Expected: every command exits 0, including the new count-loop test. The parity test now covers 61,
62 and 63.

- [ ] **Step 7: Commit.**

```bash
git add clients/js/src/compiler.ts clients/js/src/compiler.test.ts
git commit -F- <<'EOF'
Keep setReturnData out of count loops

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 6: The `InvalidOutput` error kind

**Files:**
- `common/src/template/wire.rs`:
  - `TemplateError`, `code()` and `VERIFIER_ERROR_NAMES`;
  - the test `error_codes_are_unique_and_round_trip_context`.
- `clients/js/src/errors.ts` and `clients/js/src/errors.test.ts`.
- `clients/rust/src/lib.rs`, the test `error_codes_decode_with_context`.
- `certora/ballista-specs/src/rules/errors.rs`.
- `fixtures/verifier-error-names.txt`, regenerated.

- [ ] **Step 1: Write the failing tests.**

  (a) In `wire.rs`, `error_codes_are_unique_and_round_trip_context`:
  - Add `TemplateError::InvalidOutput(15),` as the last entry of `variants`.
  - Make the last-code assertion read
    `assert_eq!(*codes.last().unwrap(), VERIFIER_ERROR_BASE + 30);`. It reads `+ 27`, or `+ 29` if
    phase 2 changed it.
  - After `assert_eq!(TemplateError::InvalidCpi(6).code(), (VERIFIER_ERROR_BASE + 15, 6));`, add:

```rust
        assert_eq!(TemplateError::InvalidOutput(9).code(), (VERIFIER_ERROR_BASE + 30, 9));
        let decoded = decode_ballista_error(encode_error(VERIFIER_ERROR_BASE + 30, 9));
        assert_eq!(decoded.map(|error| error.name), Some("InvalidOutput"));
        assert_eq!(decode_ballista_error(VERIFIER_ERROR_BASE + 31), None);
```

  (b) In `errors.test.ts`, replace phase 2's first-unused assertion,
  `expect(decodeBallistaError(6130)).toBeUndefined();`, with:

```ts
    expect(decodeBallistaError((5 << 16) | 6130)).toMatchObject({ name: 'InvalidOutput', context: 5, source: 'verifier' });
    expect(decodeBallistaError(6131)).toBeUndefined();
```

  (c) In `clients/rust/src/lib.rs`, replace `assert!(decode_ballista_error(6130).is_none());` with:

```rust
        assert_eq!(decode_ballista_error(6130).unwrap().name, "InvalidOutput");
        assert!(decode_ballista_error(6131).is_none());
```

- [ ] **Step 2: Run them and confirm that they fail.**

| Run | Expected |
| --- | --- |
| `cargo test -p ballista-common --lib error_codes` | ``error[E0599]: no variant, associated function, or constant named `InvalidOutput` found for enum `wire::TemplateError` `` |
| `pnpm --dir clients/js exec vitest run src/errors.test.ts` | FAIL: `expected undefined to match object { name: 'InvalidOutput', …(2) }` |
| `cargo test -p ballista-sdk error_codes` | FAIL: a panic on `unwrap()` of `None` |

- [ ] **Step 3: Add the kind.**

In `wire.rs`, add the variant as the last one of `TemplateError`, after `InvalidLoop`:

```rust
    /// An `EMIT` or `SET_RETURN_DATA` can encode more than `MAX_RETURN_DATA_LEN` bytes, or a
    /// `SET_RETURN_DATA` repeats, sits in a loop, or precedes an invoke.
    InvalidOutput(usize),
```

In `code()`, after the `InvalidLoop` arm:

```rust
            TemplateError::InvalidOutput(index) => (30, clamp(index)),
```

In `VERIFIER_ERROR_NAMES`, change the length from `30` to `31`, and add `"InvalidOutput",` after
`"InvalidLoop",`.

In `errors.ts`, add `'InvalidOutput',` after `'InvalidLoop',` in `VERIFIER_ERROR_NAMES`.

In `certora/ballista-specs/src/rules/errors.rs`, `rule_verifier_error_codes_are_distinct_and_in_range`:
- the catch-all arm becomes `InvalidOutput`;
- `InvalidLoop` gets an arm of its own, keeping phase 2's payload expression;
- if `TooManyAccountGroups` is still the catch-all there, give it `28 =>` as well.

```rust
        29 => TemplateError::InvalidLoop(nondet()),
        _ => TemplateError::InvalidOutput(nondet()),
```

- [ ] **Step 4: Regenerate the shared names.**

Run: `pnpm fixtures && git diff --stat fixtures`

Expected: only `fixtures/verifier-error-names.txt` changes, gaining one line, `InvalidOutput`.

- [ ] **Step 5: Run the tests.**

```bash
cargo test -p ballista-common -p ballista -p ballista-sdk --features ballista-common/proptest
pnpm --dir clients/js exec vitest run src/errors.test.ts src/fixtures.test.ts
cargo check -p ballista-specs --features rt --manifest-path certora/Cargo.toml
```

Expected: PASS.
- `runtime_error_names_match_the_shared_fixture_in_code_order`, in `programs/ballista/src/error.rs`,
  compares the Rust table with the regenerated file.
- The verifier still returns `InvalidInstruction` for opcodes 62 and 63 until Task 7.

- [ ] **Step 6: Commit.**

```bash
git add common/src/template/wire.rs clients/js/src/errors.ts clients/js/src/errors.test.ts clients/rust/src/lib.rs certora/ballista-specs/src/rules/errors.rs fixtures/verifier-error-names.txt
git commit -F- <<'EOF'
Add the InvalidOutput verifier error, code 6130

It follows the loops phase's InvalidLoop, 6129. The first unused
verifier code is now 6131, which the introspection phase takes.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 7: Verifier rules

**Files:** modify `common/src/template/verify.rs`:
- the arms in `verify_instruction`, before `OP_REQUIRE => self.require_type(…)` (line 502);
- `verify_pda_seed_segment`, renamed `verify_segment` (lines 544 and 552), with `verify_output`
  before it;
- tests.

- [ ] **Step 1: Write the failing tests.**

  (a) In `every_opcode_rejects_uninitialized_or_mistyped_operands`, add these entries directly before
  `(39, Some(VALUE_U64), None, Err(TemplateError::InvalidInstruction(1))),`:

```rust
            // An output names its parts in the immediate and no register at all.
            (OP_EMIT, Some(VALUE_U64), None, Err(TemplateError::InvalidInstruction(1))),
            (
                OP_SET_RETURN_DATA,
                Some(VALUE_U64),
                Some(VALUE_U64),
                Err(TemplateError::InvalidInstruction(2)),
            ),
```

  and this one directly after the `39` line, unless phase 1 or 2 already added it:

```rust
            // No phase of the runtime extensions assigns 75 or anything above it.
            (75, Some(VALUE_U64), None, Err(TemplateError::InvalidInstruction(1))),
```

  (b) Add these tests directly before `fn instruction_reserved_bytes_must_be_zero`:

```rust
    #[test]
    fn outputs_are_encoded_like_invocation_data_up_to_the_return_data_limit() {
        // Literal bytes, a u64 narrowed to two bytes, a pubkey, and a bytes input counted at its
        // maximum length. Neither output is an invocation.
        let mut builder = ProgramBuilder::new();
        let owner = builder.account(0, None, None, 0);
        let memo_input = builder.input(VALUE_BYTES, 16);
        let memo = builder.load_input(memo_input);
        let amount = builder.const_u64(7);
        let key = builder.account_key(owner);
        let tag = builder.blob(b"TAG");
        builder.emit_data(&[
            Segment::Literal(tag),
            Segment::Register(DATA_REG_U16, amount),
            Segment::Register(DATA_REG_PUBKEY, key),
            Segment::Register(DATA_REG_BYTES, memo),
        ]);
        builder.set_return_data(&[Segment::Register(DATA_REG_U64, amount)]);
        let stats = verify_builder(&builder).unwrap();
        assert_eq!((stats.cpis, stats.max_expanded_cpis, stats.max_cpi_data_len), (0, 0, 0));

        // A 1,024-byte bytes input fills the limit; one more literal byte passes it.
        let at_limit = |extra: usize| {
            let mut builder = ProgramBuilder::new();
            let input = builder.input(VALUE_BYTES, MAX_INPUT_BYTES as u16);
            let value = builder.load_input(input);
            let literal = builder.blob(&vec![0; extra]);
            let at = builder.set_return_data(&[
                Segment::Register(DATA_REG_BYTES, value),
                Segment::Literal(literal),
            ]);
            (verify_builder(&builder).map(|_| ()), at)
        };
        assert_eq!(at_limit(0).0, Ok(()));
        let (outcome, at) = at_limit(1);
        assert_eq!(outcome, Err(TemplateError::InvalidOutput(at)));

        // EMIT has the same bound, and a select counts its longer branch: 1,000 bytes.
        let emit_with_padding = |padding: usize| {
            let mut builder = ProgramBuilder::new();
            let condition = builder.const_bool(true);
            let short = builder.const_bytes(&[1; 4]);
            let long = builder.const_bytes(&[2; 1_000]);
            let selected = builder.select(condition, short, long);
            let literal = builder.blob(&vec![0; padding]);
            let at = builder.emit_data(&[
                Segment::Register(DATA_REG_BYTES, selected),
                Segment::Literal(literal),
            ]);
            (verify_builder(&builder).map(|_| ()), at)
        };
        assert_eq!(emit_with_padding(24).0, Ok(()));
        let (outcome, at) = emit_with_padding(25);
        assert_eq!(outcome, Err(TemplateError::InvalidOutput(at)));
    }

    #[test]
    fn output_segments_follow_the_segment_rules() {
        // Each case builds a CPI with two segments first, so the output's own segment is at
        // index 2: errors name the segment's place in the whole table.
        let check = |mutate: &dyn Fn(&mut ProgramBuilder, u8), expected: TemplateError| {
            let mut builder = ProgramBuilder::new();
            let program = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
            let amount = builder.const_u64(1);
            let literal = builder.blob(&[2, 0, 0, 0]);
            let cpi = builder.cpi(
                program,
                &[],
                &[Segment::Literal(literal), Segment::Register(DATA_REG_U64, amount)],
            );
            builder.invoke(cpi, None);
            mutate(&mut builder, amount);
            assert_eq!(verify_builder(&builder), Err(expected), "{expected:?}");
        };
        check(
            &|builder, amount| {
                builder.emit_data(&[Segment::Register(DATA_REG_I64, amount)]);
            },
            TemplateError::TypeMismatch,
        );
        check(
            &|builder, _| {
                let unset = builder.register();
                builder.emit_data(&[Segment::Register(DATA_REG_U64, unset)]);
            },
            TemplateError::RegisterNotInitialized(1),
        );
        check(
            &|builder, amount| {
                builder.emit_data(&[Segment::Register(0xfe, amount)]);
            },
            TemplateError::InvalidDataSegment(2),
        );
        check(
            &|builder, _| {
                builder.emit_data(&[Segment::Literal((2, 3))]);
            },
            TemplateError::InvalidDataSegment(2),
        );
        check(
            &|builder, _| {
                builder.emit_data(&[Segment::Literal((0, 1))]);
                builder.segments_mut()[2].register = 0;
            },
            TemplateError::InvalidDataSegment(2),
        );
        check(
            &|builder, amount| {
                builder.emit_data(&[Segment::Register(DATA_REG_U64, amount)]);
                builder.segments_mut()[2].len_le = [1, 0];
            },
            TemplateError::InvalidDataSegment(2),
        );
        check(
            &|builder, amount| {
                builder.emit_data(&[Segment::Register(DATA_REG_U64, amount)]);
                builder.segments_mut()[2].reserved = [0, 1];
            },
            TemplateError::InvalidDataSegment(2),
        );
    }

    #[test]
    fn output_records_name_a_non_empty_range_and_no_register() {
        let with_output = |mutate: &dyn Fn(&mut InstructionRecord)| {
            let mut builder = ProgramBuilder::new();
            let amount = builder.const_u64(1);
            let at = builder.emit_data(&[Segment::Register(DATA_REG_U64, amount)]);
            mutate(&mut builder.instructions_mut()[at]);
            (verify_builder(&builder).map(|_| ()), at)
        };
        assert_eq!(with_output(&|_| {}).0, Ok(()));
        for mutate in [
            (|record: &mut InstructionRecord| record.dst = 0) as fn(&mut InstructionRecord),
            |record| record.a = 0,
            |record| record.b = 0,
            |record| record.c = 0,
            |record| record.immediate_le = range_immediate(0, 0).to_le_bytes(),
            |record| record.immediate_le = range_immediate(1, 1).to_le_bytes(),
            |record| record.immediate_le = range_immediate(0, 2).to_le_bytes(),
        ] {
            let (outcome, at) = with_output(&mutate);
            assert_eq!(outcome, Err(TemplateError::InvalidInstruction(at)));
        }
        let (outcome, at) = with_output(&|record| record.flags = INSTRUCTION_FLAG_DYNAMIC_OFFSET);
        assert_eq!(outcome, Err(TemplateError::InvalidFlags(at)));
    }

    #[test]
    fn return_data_is_set_once_outside_every_loop_after_the_last_invoke() {
        let transfer = |builder: &mut ProgramBuilder| {
            let system = builder.account(ACCOUNT_EXECUTABLE, Some([1; 32]), None, 0);
            builder.cpi(system, &[], &[])
        };
        let result = |builder: &mut ProgramBuilder| {
            let value = builder.const_u64(7);
            builder.set_return_data(&[Segment::Register(DATA_REG_U64, value)])
        };

        // After the last invoke, with outputs logged anywhere else, it verifies.
        let mut builder = ProgramBuilder::new();
        let cpi = transfer(&mut builder);
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let count = builder.const_u64(3);
        builder.emit_data(&[Segment::Register(DATA_REG_U64, count)]);
        builder.for_each(0, |body| {
            let row = body.loop_index();
            body.emit_data(&[Segment::Register(DATA_REG_U8, row)]);
            body.invoke(cpi, None);
        });
        builder.invoke(cpi, None);
        result(&mut builder);
        assert!(verify_builder(&builder).is_ok());

        // Before an invoke.
        let mut builder = ProgramBuilder::new();
        let cpi = transfer(&mut builder);
        let at = result(&mut builder);
        builder.invoke(cpi, None);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidOutput(at)));

        // Before a loop whose body invokes.
        let mut builder = ProgramBuilder::new();
        let cpi = transfer(&mut builder);
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let at = result(&mut builder);
        builder.for_each(0, |body| body.invoke(cpi, None));
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidOutput(at)));

        // Inside a loop.
        let mut builder = ProgramBuilder::new();
        builder.row_account(0, None, None, 0);
        builder.batch(2, 0);
        let value = builder.const_u64(7);
        let mut at = 0;
        builder.for_each(0, |body| {
            at = body.set_return_data(&[Segment::Register(DATA_REG_U64, value)]);
        });
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidOutput(at)));

        // Twice: the first names the error.
        let mut builder = ProgramBuilder::new();
        let at = result(&mut builder);
        result(&mut builder);
        assert_eq!(verify_builder(&builder), Err(TemplateError::InvalidOutput(at)));

        // The single-instruction entry point the specifications use never indexes out of range.
        let mut builder = ProgramBuilder::new();
        result(&mut builder);
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        let mut registers = [None; MAX_REGISTERS];
        registers[0] = Some(RegisterInfo::scalar(VALUE_U64));
        let record = program.instructions[1];
        assert_eq!(
            program.verify_single_instruction(&record, usize::MAX, false, None, &mut registers),
            Ok((0, 0))
        );
    }

    /// A count loop, the loops phase's `REPEAT`, is a loop like any other.
    #[test]
    fn return_data_is_not_set_inside_a_count_loop() {
        // REPEAT: `a` is the body length, `b` the u64 count register, `c` the static maximum.
        let with_body = |body: &dyn Fn(&mut ProgramBuilder, u8) -> usize| {
            let mut builder = ProgramBuilder::new();
            let count = builder.const_u64(2);
            let value = builder.const_u64(7);
            builder.emit(record(OP_REPEAT, NO_INDEX, 1, count, 2, 0, 0));
            let at = body(&mut builder, value);
            (verify_builder(&builder).map(|_| ()), at)
        };
        let (outcome, at) = with_body(&|builder, value| {
            builder.set_return_data(&[Segment::Register(DATA_REG_U64, value)])
        });
        assert_eq!(outcome, Err(TemplateError::InvalidOutput(at)));
        let (outcome, _) = with_body(&|builder, value| {
            builder.emit_data(&[Segment::Register(DATA_REG_U64, value)])
        });
        assert_eq!(outcome, Ok(()));
    }
```

The `REPEAT` record follows the spec's layout. If phase 2's verifier needs anything else in that
record (a `dst`, a flag), build the loop the way phase 2's own verifier tests do. Keep both
assertions.

- [ ] **Step 2: Run them and confirm that they fail.**

Run: `cargo test -p ballista-common --lib verify`

Expected: FAIL, 5 tests:
- `outputs_are_encoded_like_invocation_data_up_to_the_return_data_limit`;
- `output_segments_follow_the_segment_rules`;
- `output_records_name_a_non_empty_range_and_no_register`;
- `return_data_is_set_once_outside_every_loop_after_the_last_invoke`;
- `return_data_is_not_set_inside_a_count_loop`.

Every output record is still an unknown opcode, `InvalidInstruction`. The new sweep entries already
pass, because an unknown opcode fails at the same index.

- [ ] **Step 3: Implement.**

  (a) Rename `verify_pda_seed_segment` to `verify_segment`, both the definition and its one call in
  `verify_pda_seeds`, and give it this doc comment:

```rust
    /// Checks one data segment of a PDA seed or an output and returns the most bytes it can
    /// encode: a literal's length, a register kind's width, or a `bytes` register's maximum
    /// length. Invocation data applies the same widths in `verify_cpi`.
```

  (b) Directly before it, add:

```rust
    /// Shared by `EMIT` and `SET_RETURN_DATA`. The record names no register, and its immediate
    /// names a non-empty, in-bounds run of data segments, encoded as invocation data is. Their
    /// widths, a `bytes` register counted at its maximum length, must sum to at most
    /// `MAX_RETURN_DATA_LEN`, the return-data limit, which also bounds a log line.
    fn verify_output(
        &self,
        instruction: &InstructionRecord,
        instruction_index: usize,
        registers: &[Option<RegisterInfo>; MAX_REGISTERS],
    ) -> Result<(), TemplateError> {
        if [instruction.dst, instruction.a, instruction.b, instruction.c] != [NO_INDEX; 4] {
            return Err(TemplateError::InvalidInstruction(instruction_index));
        }
        let (segment_start, segment_len) = instruction.blob_range();
        let segment_end = segment_start
            .checked_add(segment_len)
            .ok_or(TemplateError::CountOverflow)?;
        if segment_len == 0 || segment_end > self.data_segments.len() {
            return Err(TemplateError::InvalidInstruction(instruction_index));
        }
        let mut max_len = 0usize;
        for (offset, segment) in self.data_segments[segment_start..segment_end]
            .iter()
            .enumerate()
        {
            let len = self.verify_segment(segment_start + offset, segment, registers)?;
            max_len = max_len
                .checked_add(len)
                .ok_or(TemplateError::CountOverflow)?;
        }
        if max_len > MAX_RETURN_DATA_LEN {
            return Err(TemplateError::InvalidOutput(instruction_index));
        }
        Ok(())
    }
```

  (c) In `verify_instruction`, directly before
  `OP_REQUIRE => self.require_type(registers, instruction.a, VALUE_BOOL)?,`:

```rust
            OP_EMIT => {
                self.verify_output(instruction, instruction_index, registers)?;
            }
            OP_SET_RETURN_DATA => {
                // The runtime clears return data whenever a program is invoked, CPIs included, so
                // what a run returns is set once, outside every loop, after its last invoke. Loops
                // run forward, so every instruction that can run later sits at a later index.
                let later = self
                    .instructions
                    .get(instruction_index.saturating_add(1)..)
                    .unwrap_or(&[]);
                if in_loop
                    || later
                        .iter()
                        .any(|record| matches!(record.opcode, OP_INVOKE | OP_SET_RETURN_DATA))
                {
                    return Err(TemplateError::InvalidOutput(instruction_index));
                }
                self.verify_output(instruction, instruction_index, registers)?;
            }
```

  Phase 2 may have renamed or split `in_loop` in `verify_instruction`. If so, test the condition
  that `OP_LOOP_INDEX` uses to accept itself, which is true inside every loop, not the one that
  guards row accounts. `return_data_is_not_set_inside_a_count_loop` checks this.

  `get(..).unwrap_or(&[])` keeps `verify_single_instruction`, which the Certora rules call with an
  arbitrary index, from panicking.

- [ ] **Step 4: Run the tests.**

Run: `cargo test -p ballista-common --lib verify`

Expected: PASS, including the five tests above and the extended sweep.

Run: `cargo test -p ballista-common --features proptest`

Expected: PASS. That covers `generated` and `no_panic`, and `every_shared_fixture_parses_and_verifies`
still verifies every fixture and all 12 protocol examples.

- [ ] **Step 5: Commit.**

```bash
git add common/src/template/verify.rs
git commit -F- <<'EOF'
Verify the output opcodes

EMIT and SET_RETURN_DATA name a non-empty run of data segments and no
register. The segments follow the PDA-seed rule, renamed verify_segment,
and their worst-case widths sum to at most 1,024 bytes.
SET_RETURN_DATA appears once, outside every loop, with no INVOKE at a
later index: the runtime clears return data whenever a program is
invoked. Violations are InvalidOutput.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 8: Executor, and what it costs

**Files:**
- Modify: `programs/ballista/src/processor/execute.rs`:
  - `Scratch` and `Scratch::new`;
  - `emit_event`;
  - the fallback arm of `extended_instruction`;
  - a new `write_output`;
  - the `ByteSink` doc and a new `OutputSink`;
  - tests.
- Modify: `programs/ballista/src/profile.rs`, and `programs/ballista/src/lib.rs` (`run_template`).
- Modify: `tests/ballista/src/lib.rs` (the math fixture prints its units) and
  `tests/ballista/src/cases.rs` (a new case).
- Modify: `fixtures/cu-ceilings.json`, `fixtures/example-ceilings.json`, `benches/CHANGELOG.md`.

**Rules for this task:**
- Leave the dispatch loop's outer match exactly as it is: no arm, no guard, no `if`. It is in
  `execute_instruction`, or in the function phase 1's refactor moved it to. LLVM folds any opcode
  test into its comparison tree, and SBF has no indirect jump.
- Leave the arms of `extended_instruction` alone as well. Only its fallback arm changes.

- [ ] **Step 1: Check the helper's shape, then measure the base.**

Run: `grep -n -A4 "^fn extended_instruction" programs/ballista/src/processor/execute.rs`

Expected: `machine: &mut Machine<'_, 'data>,` as the first parameter.
- If the helper still takes separate arguments, stop and ask.
- Step 6 records what proceeding without the refactor would cost.

In `tests/ballista/src/lib.rs`, `typescript_math_fixture_computes_exact_results`, add this line
directly after `assert!(exact.program_result.is_ok(), "{exact:#?}");`:

```rust
        eprintln!("math fixture compute units: {}", exact.compute_units_consumed);
```

Then run the following. `target/` is ignored by git, so the measurement files kept there are never
committed.

```bash
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
cargo test --manifest-path tests/ballista/Cargo.toml -- compute_units_stay_under_their_ceiling measure_every_example
cargo test --manifest-path tests/ballista/Cargo.toml typescript_math_fixture_computes_exact_results -- --nocapture 2>&1 | grep "math fixture compute units" | tee target/math-before.txt
```

Expected: both ceiling tests pass, and `target/math-before.txt` holds one line, such as
`math fixture compute units: 4859`. It was 4,859 at `7a5e15e`; phase 1's refactor and phase 2 may
have moved it.

- [ ] **Step 2: Write the failing tests.** In the `tests` module of `execute.rs`, directly after
  `opcodes_the_executor_does_not_run_fail_before_reading_operands`, add the two tests below.
  - They call `execute_instruction` with its separate arguments, as the module's other tests do.
  - If phase 1's refactor changed how tests run one instruction, call it the way
    `unset_operands_report_invalid_register_before_type_mismatch` now does.

```rust
    /// `EMIT` and `SET_RETURN_DATA` encode into their own buffer. `data` may hold an invocation
    /// payload that a batch sends again without encoding it, so an output must never write there.
    #[test]
    fn outputs_encode_into_their_own_buffer() {
        let mut builder = ProgramBuilder::new();
        let amount = builder.const_u64(0x0102);
        let key = builder.const_pubkey([7; 32]);
        let tag = builder.blob(b"OUT");
        let logged = builder.emit_data(&[
            Segment::Literal(tag),
            Segment::Register(DATA_REG_U16, amount),
            Segment::Register(DATA_REG_PUBKEY, key),
        ]);
        let returned = builder.set_return_data(&[Segment::Register(DATA_REG_U64, amount)]);
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        let mut scratch = Scratch::new(&program);
        scratch.data.extend_from_slice(b"cached payload");
        let mut registers = vec![U64(0x0102), Pubkey([7; 32])];

        execute_instruction(
            &program,
            &[],
            &[],
            &mut registers,
            &mut scratch,
            &program.instructions[logged],
            None,
        )
        .unwrap();
        let mut expected = b"OUT".to_vec();
        expected.extend_from_slice(&0x0102u16.to_le_bytes());
        expected.extend_from_slice(&[7; 32]);
        assert_eq!(scratch.output.as_deref(), Some(&expected));
        assert_eq!(scratch.data, b"cached payload", "the invocation buffer is untouched");
        // The first output allocates the most any output encodes; later ones reuse it.
        let buffer = scratch.output.as_ref().unwrap();
        assert_eq!(buffer.capacity(), MAX_RETURN_DATA_LEN);
        let allocated = buffer.as_ptr();

        execute_instruction(
            &program,
            &[],
            &[],
            &mut registers,
            &mut scratch,
            &program.instructions[returned],
            None,
        )
        .unwrap();
        assert_eq!(scratch.output.as_deref(), Some(&0x0102u64.to_le_bytes().to_vec()));
        assert_eq!(scratch.output.as_ref().unwrap().as_ptr(), allocated, "no second allocation");
        assert_eq!(scratch.data, b"cached payload");
        assert_eq!(registers, vec![U64(0x0102), Pubkey([7; 32])], "outputs write no register");
    }

    /// Outputs fail like invocation data does on a bad register, and as an invalid program on a
    /// shape the verifier rules out.
    #[test]
    fn outputs_reject_bad_operands_and_unverified_shapes() {
        let mut builder = ProgramBuilder::new();
        for _ in 0..3 {
            builder.register();
        }
        // r0 is unset, r1 holds a u64 too wide for a u8, r2 holds an i64.
        let unset = builder.emit_data(&[Segment::Register(DATA_REG_U64, 0)]);
        let narrow = builder.emit_data(&[Segment::Register(DATA_REG_U8, 1)]);
        let mistyped = builder.set_return_data(&[Segment::Register(DATA_REG_U64, 2)]);
        let long = builder.blob(&[0; MAX_RETURN_DATA_LEN + 1]);
        let too_long = builder.emit_data(&[Segment::Literal(long)]);
        let past_the_table = builder.emit(record(
            OP_EMIT,
            NO_INDEX,
            NO_INDEX,
            NO_INDEX,
            NO_INDEX,
            0,
            range_immediate(4, 1),
        ));
        let bytes = builder.build().unwrap();
        let program = ProgramView::parse(&bytes).unwrap();
        let mut scratch = Scratch::new(&program);
        let mut registers = vec![Unset, U64(256), I64(-1)];
        let mut run = |index: usize| {
            execute_instruction(
                &program,
                &[],
                &[],
                &mut registers,
                &mut scratch,
                &program.instructions[index],
                None,
            )
        };
        assert_eq!(run(unset), Err(err(BallistaError::InvalidRegister)));
        assert_eq!(run(narrow), Err(err(BallistaError::ArithmeticOverflow)));
        assert_eq!(run(mistyped), Err(err(BallistaError::TypeMismatch)));
        assert_eq!(run(too_long), Err(err(BallistaError::InvalidTemplateProgram)));
        assert_eq!(run(past_the_table), Err(err(BallistaError::InvalidTemplateProgram)));
    }
```

In `opcodes_the_executor_does_not_run_fail_before_reading_operands`:
- Replace the opcode list with `[0, 39, OP_FOREACH, 75, 0xfe, u8::MAX]`.
- Keep any entry that phase 2 added for its own loop opcode, but drop entries of the form
  `OP_… + 1`: those name a number that a later phase assigns.
- Update the comment above the loop:

```rust
        // 39 is unassigned, and no phase of the runtime extensions assigns 75 or anything above
        // it. FOREACH reaches the executor only from inside a loop body.
```

- [ ] **Step 3: Run them and confirm that they fail.**

Run: `cargo test -p ballista --lib outputs`

Expected: a compile error, ``error[E0609]: no field `output` on type `execute::Scratch<'_>` ``.

- [ ] **Step 4: Implement.**

  (a) In `Scratch`, directly after the field `data: Vec<u8>,`:

```rust
    /// The bytes an `EMIT` or `SET_RETURN_DATA` encodes. Never `data`: inside a batch, `data` can
    /// hold a loop-invariant invocation payload that the next row sends again without encoding
    /// it, and an output written there would replace it.
    ///
    /// `None` until the run's first output, which allocates the buffer at `MAX_RETURN_DATA_LEN`
    /// bytes, the most any verified output encodes, for every later output to reuse. It is leaked
    /// rather than owned: the SBF heap never frees, so that costs nothing, and a null reference is
    /// the cheapest field to initialize. Every run pays for that store, one compute unit here
    /// against two for an `Option<Vec<u8>>` and three or four for an empty `Vec<u8>`.
    output: Option<&'data mut Vec<u8>>,
```

  In `Scratch::new`, after `data: Vec::with_capacity(max_data),`, add `output: None,`.

  (b) Give `emit_event` a doc comment, directly above its `#[inline(always)]`:

```rust
/// Logs `event` as one `Program data:` field: the run event, and every `EMIT`.
```

  (c) In `extended_instruction`, change only the fallback arm. It reads either
  `_ => Err(BallistaError::InvalidTemplateProgram.into()),` or, inside a `let value = match`,
  `_ => return Err(BallistaError::InvalidTemplateProgram.into()),`. It becomes the matching one of:

```rust
        _ => write_output(machine, instruction),
```

```rust
        _ => return write_output(machine, instruction),
```

  Add `write_output` directly after `extended_instruction`:

```rust
/// `EMIT` and `SET_RETURN_DATA`, reached through `extended_instruction`'s fallback arm so that
/// its match keeps its shape. Every opcode the executor does not run arrives here too, and fails
/// before anything is read.
///
/// The output's parts are encoded into the run's output buffer, exactly as invocation data is,
/// then logged as one `Program data:` field or set as the run's return data. The verifier bounds
/// every output at `MAX_RETURN_DATA_LEN` bytes, and admits one `SET_RETURN_DATA`, outside every
/// loop and after the last invoke, because the runtime clears return data whenever a program is
/// invoked. A `cu-profile` build replaces it with the profile record once the run ends; see
/// `profile::report`.
#[inline(never)]
fn write_output(machine: &mut Machine<'_, '_>, instruction: &InstructionRecord) -> RunResult<()> {
    let emit = match instruction.opcode {
        OP_EMIT => true,
        OP_SET_RETURN_DATA => false,
        _ => return Err(BallistaError::InvalidTemplateProgram.into()),
    };
    let (start, count) = instruction.blob_range();
    let segments = start
        .checked_add(count)
        .and_then(|end| machine.program.data_segments.get(start..end))
        .ok_or(BallistaError::InvalidTemplateProgram)?;
    let buffer = machine
        .scratch
        .output
        .get_or_insert_with(|| Box::leak(Box::new(Vec::with_capacity(MAX_RETURN_DATA_LEN))));
    buffer.clear();
    let mut sink = OutputSink(buffer);
    for segment in segments {
        encode_segment(machine.program, machine.registers, segment, &mut sink)?;
    }
    let bytes = sink.0.as_slice();
    if bytes.len() > MAX_RETURN_DATA_LEN {
        return Err(BallistaError::InvalidTemplateProgram.into());
    }
    if emit {
        emit_event(bytes);
    } else {
        pinocchio::cpi::set_return_data(bytes);
    }
    Ok(())
}
```

  (d) Replace the doc comment on `pub trait ByteSink` with:

```rust
/// Destination for encoded segment bytes: a `Vec` for CPI data, the output buffer for `EMIT` and
/// `SET_RETURN_DATA`, or a fixed stack buffer for PDA seeds. Spec builds keep `push_bytes` out of
/// line so the prover can summarize the copy inside.
```

  and add, directly before `pub struct FixedSink<'buffer> {`:

```rust
/// The output buffer as a byte sink. A type of its own rather than the `Vec<u8>` sink, so the
/// output opcodes get their own copy of the segment encoder: sharing the invocation's copy made
/// the compiler stop inlining it into `invoke_cpi`, which cost every invocation about 50 compute
/// units.
struct OutputSink<'buffer>(&'buffer mut Vec<u8>);

impl ByteSink for OutputSink<'_> {
    #[cfg_attr(feature = "spec-api", inline(never))]
    #[cfg_attr(not(feature = "spec-api"), inline(always))]
    fn push_bytes(&mut self, bytes: &[u8]) -> RunResult<()> {
        self.0.extend_from_slice(bytes);
        Ok(())
    }
}
```

  (e) Document the profiling overwrite, without changing it.
  - In `profile.rs`, replace the module doc's sentence
    "the run ends by emitting one `sol_log_data` record that" with
    "the run ends by setting one record that". The record is return data, as the doc's next sentence
    already says.
  - After the paragraph that ends "without parsing logs.", add:

```rust
//!
//! Setting it replaces whatever return data the run set with `SET_RETURN_DATA`, so a template run
//! on a profiling build never returns its own result. That is acceptable for a build that exists
//! only to measure; the deployed binary never enables the feature.
```

  In `profile.rs`, make the doc on `report` in the `enabled` module read:

```rust
    /// Emits the record: magic, mark count, `(tag, remaining)` pairs, then the CPI totals. It
    /// becomes the instruction's return data, replacing any the template set.
```

  In `lib.rs`, `run_template`, directly above `profile::report();`:

```rust
    // A `cu-profile` build sets its record as return data here, over any the run set.
```

- [ ] **Step 5: Run the tests, build for SBF, and check the frames.**

Run: `cargo test -p ballista --lib`

Expected: PASS, including `outputs_encode_into_their_own_buffer`,
`outputs_reject_bad_operands_and_unverified_shapes` and
`opcodes_the_executor_does_not_run_fail_before_reading_operands`.

Run:

```bash
cargo build-sbf --manifest-path programs/ballista/Cargo.toml 2>&1 | grep -i -E "stack|error" || true
cargo build-sbf --manifest-path programs/ballista/Cargo.toml --features cu-profile 2>&1 | grep -i -E "stack|error" || true
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
```

Expected: no output from either grep. The last command leaves the release binary in
`target/deploy`, where the Mollusk tests read it.

- [ ] **Step 6: If `extended_instruction` still took separate arguments.** Skip this step when Step 1
  confirmed the `Machine` shape. If you were told to proceed without it:
  - Add `scratch: &mut Scratch<'data>` to `extended_instruction` after `registers`.
  - Pass it from the `_` arm of `execute_instruction`.
  - Give `write_output` the parameters `(program, registers, scratch, instruction)`.
  - The prototype measured the math fixture at +33 CU that way (4,859 → 4,892), against +1 with the
    `Machine` shape. Record that in the commit and the changelog entry.

- [ ] **Step 7: Add a ceiling case for the output path.** In `tests/ballista/src/cases.rs`:
  - In `cases()`, add the line
    `("run, log and return 16 bytes", output_case(creator, 10)),` directly before
    `("create template, payroll 30 rows", upload(9)),`.
  - Add this function directly before `fn sum_rows`:

```rust
/// A read and an input, logged with a tag and then returned: the two output opcodes and their
/// syscalls, `sol_log_data` and `sol_set_return_data`.
fn output_case(creator: Pubkey, template_id: u16) -> Case {
    let mut builder = ProgramBuilder::new();
    let oracle = builder.account(0, None, None, 128);
    let price = builder.read(OP_READ_U64, oracle, 64);
    let amount_input = builder.input(VALUE_U64, 0);
    let amount = builder.load_input(amount_input);
    let tag = builder.blob(b"BOUT");
    builder.emit_data(&[
        Segment::Literal(tag),
        Segment::Register(DATA_REG_U64, price),
        Segment::Register(DATA_REG_U64, amount),
    ]);
    builder.set_return_data(&[
        Segment::Register(DATA_REG_U64, price),
        Segment::Register(DATA_REG_U64, amount),
    ]);
    let (account_key, account) = with_data(template_id * 100 + 1);
    run_case(
        creator,
        template_id,
        builder.build().expect("builds"),
        vec![(AccountMeta::new_readonly(account_key, false), account)],
        1_000u64.to_le_bytes().to_vec(),
    )
}
```

  Template ID 10 is unused by the other cases, whose IDs run from 1 to 9.

- [ ] **Step 8: Measure.**

```bash
cargo test --manifest-path tests/ballista/Cargo.toml -- compute_units_stay_under_their_ceiling measure_every_example 2>&1 | grep -E " CU, (ceiling|no ceiling)" | tee target/output-ceilings.txt
cargo test --manifest-path tests/ballista/Cargo.toml typescript_math_fixture_computes_exact_results -- --nocapture 2>&1 | grep "math fixture compute units" | tee target/math-after.txt
```

Expected:
- Both ceiling tests fail. `target/output-ceilings.txt` holds lines such as
  `  run, empty template: 596 CU, ceiling 595, over by 1` and
  `  run, log and return 16 bytes: 1505 CU, no ceiling recorded`.
- The math fixture prints one more unit than `target/math-before.txt`.

On phase 1's tip, the prototype measured:
- every run's cost, and every cookbook example's, up by exactly 1;
- over the ceiling, and so listed: the eight run cases, and 18 of the 32 examples;
  - the other 14 examples' ceilings sat a unit or more above their cost, so they need no edit;
- `create template, payroll 30 rows` 1 under, which needs no edit either;
- the new case at 1,505.

If a case is over by more than 2, or the math fixture rises by more than 2, look before raising
anything:
- Is `write_output` still `#[inline(never)]`?
- Does `extended_instruction`'s match have no arms of its own for 62 and 63?
- Is `OutputSink` still the only sink `write_output` passes to `encode_segment`?

- [ ] **Step 9: Raise the ceilings by hand to the printed values.** Work from
  `target/output-ceilings.txt`.
  - In `fixtures/cu-ceilings.json`, set each case listed "over by" to its printed CU. Add
    `"run, log and return 16 bytes"` with its printed value, keeping the keys in alphabetical order.
  - In `fixtures/example-ceilings.json`, set each example listed "over by" to its printed value.
  - Do not lower anything in this commit.
  - Then run: `cargo test --manifest-path tests/ballista/Cargo.toml`

Expected: PASS, the whole suite.

- [ ] **Step 10: Record the cost in `benches/CHANGELOG.md`.**
  - Add a section `## Runtime extensions, not merged` directly above
    `## Pending: integrated, not merged`. If phase 1 or 2 already added a section for the
    runtime-extension branches, put the entry at its top instead.
  - Where Step 8 measured differently from the prototype, use your figures in the first three
    **Measured** bullets.
  - The two per-opcode bullets come from a probe that the prototype ran on phase 1's tip. Keep them
    as they are.

```md
## Runtime extensions, not merged

### 2026-09-27 · An output buffer for `EMIT` and `SET_RETURN_DATA` · `claude/runtime-output`
- **Change:** `Scratch` holds the output opcodes' buffer. Every run sets it to `None`, the run's
  first output allocates it at 1,024 bytes, and later outputs reuse it. Both opcodes reach one
  out-of-line helper through `extended_instruction`'s fallback arm and encode through a sink type of
  their own.
- **Measured:**
  - Every run: +1 CU, the store that sets the buffer to `None`. Each of the eight run cases and each
    of the 32 cookbook examples rose by exactly 1.
  - The math fixture: +1, the same store. Its opcodes pay nothing for the new path.
  - `run, log and return 16 bytes`, a new case: 1,505.
  - A 20-byte `EMIT` in three parts, measured on phase 1's tip: 429 CU, of which `sol_log_data`
    charges 220. A run's first output also allocates the buffer, about 38 CU more.
  - A 16-byte `SET_RETURN_DATA` as a run's first output, measured on phase 1's tip: 296 CU, of which
    `sol_set_return_data` charges 100.
- **Checked:** the executor's unit tests, and the Mollusk suite with its ceilings. Later commits on
  the branch run the path on chain: a fixture that logs between two sends of a cached transfer
  payload, and generated programs that log and return data.
- **Watch:** `sol_log_data` charges 100 CU per call, 100 per field and 1 per byte, so a 1,024-byte
  `EMIT` costs 1,224 CU in the syscall alone.
```

Under `## Tried and rejected`, add this bullet after the existing ones:

```md
- **Output opcodes:**
  - Encoding outputs through the invocation data's `Vec<u8>` sink gave the encoder a second caller.
    The compiler stopped inlining it into `invoke_cpi`, which cost about +50 CU per invocation and +1,600
    on `index-weighted-rewards`. Marking both encoders `#[inline(always)]` still cost about +13 per
    invocation.
  - An empty `Vec<u8>` for the buffer cost 3–4 CU per run, and an `Option<Vec<u8>>` cost 2.
  - In a prototype of the `Machine`-shaped router, arms of their own in `extended_instruction` cost
    every math opcode about 7 CU. One helper per opcode gave the encoder two callers again, and
    each output cost about 65 CU more.
```

- [ ] **Step 11: Commit.** The message's table comes from Step 8's files:
  - *before* is the ceiling the file held;
  - *after* is the measured value;
  - a new case shows `-` as its *before*.

```bash
{
  cat <<'EOF'
Log and return data from a buffer of the executor's own

EMIT and SET_RETURN_DATA encode their parts into Scratch::output, never
into Scratch::data: inside a batch, data can hold a loop-invariant CPI
payload that the next row sends without encoding it again. The buffer is
allocated at 1,024 bytes by a run's first output and reused after that.
Both opcodes reach write_output through extended_instruction's fallback
arm, so neither match changes shape, and they encode through a sink
type of their own, which keeps invoke_cpi's encoder inlined.

A cu-profile build still replaces the run's return data with its
record; the profile code now says so.

Ceilings raised by hand to the measured values. The new case locks in
the output path.

EOF
  printf '%-40s %8s %8s\n' '' before after
  sed -nE 's/^ +(.+): ([0-9]+) CU, ceiling ([0-9]+), over by [0-9]+$/\1|\3|\2/p; s/^ +(.+): ([0-9]+) CU, no ceiling recorded$/\1|-|\2/p' target/output-ceilings.txt \
    | awk -F'|' '{ printf "%-40s %8s %8s\n", $1, $2, $3 }'
  printf '\nThe math fixture: %s -> %s CU.\n\n' "$(awk '{ print $NF }' target/math-before.txt)" "$(awk '{ print $NF }' target/math-after.txt)"
  echo 'Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>'
} > target/output-commit.txt
cat target/output-commit.txt
git add programs/ballista/src/processor/execute.rs programs/ballista/src/profile.rs programs/ballista/src/lib.rs tests/ballista/src/lib.rs tests/ballista/src/cases.rs fixtures/cu-ceilings.json fixtures/example-ceilings.json benches/CHANGELOG.md
git commit -F target/output-commit.txt
```

Expected: `cat` shows one table row per line of `target/output-ceilings.txt` that was over its
ceiling or had none. On phase 1's tip that was 27 rows, from `run, log and return 16 bytes  -  1505`
and `run, empty template  595  596` to `waterfall-until-the-money-runs-out  17660  17661`, followed by
`The math fixture: 4859 -> 4860 CU.`

---

### Task 9: The program generator

**Files:** modify `common/src/template/generate.rs`:
- `from_choices`, after the `if batched { … }` block (line 217);
- `emit_operation` (line 258);
- a new function, `output_part`, before `any_program`.

- [ ] **Step 1: Extend the generator.**
  - In `emit_operation`, change `match choices.below(16)` to `match choices.below(17)`. If phase 2
    changed the count, add one to it.
  - Insert this arm directly before the final `_ =>` arm:

```rust
        15 => {
            // A log line of one to three registers, sometimes after a literal byte. Outputs may
            // appear anywhere, loop bodies included, and write no register.
            let mut parts = Vec::new();
            if choices.below(4) == 0 {
                parts.push(Segment::Literal(builder.blob(&[choices.next() as u8])));
            }
            for _ in 0..1 + choices.below(3) {
                parts.push(output_part(choices, registers));
            }
            builder.emit_data(&parts);
        }
```

  - If phase 2 already used 15, number this arm one past phase 2's last numbered arm.
  - In `from_choices`, directly before
    `let bytes = builder.build().expect("generated programs stay within the payload limit");`, add:

```rust
        // A run may set its return data once, after its last invoke. Generated programs invoke
        // nothing, so the end of the root is always a legal place.
        if choices.below(2) == 1 {
            let part = output_part(&mut choices, &registers);
            builder.set_return_data(&[part]);
        }
```

  The root's `registers` is the one that `emit_operation` used after the loops. Loop bodies worked
  on copies, so every register it lists holds a value here.

  - Directly before `/// A strategy producing programs that verify by construction.`, add:

```rust
/// One output part: a register that holds a value, with an encoding its type accepts. Unsigned
/// values may be narrowed, and a value too wide for its encoding fails the run with
/// `ArithmeticOverflow`, an allowed value-dependent failure.
fn output_part(choices: &mut Choices<'_>, registers: &Registers) -> Segment {
    let (register, kind) = registers.entries[choices.below(registers.len())];
    let encoding = match kind {
        VALUE_BOOL => DATA_REG_BOOL,
        VALUE_I64 => DATA_REG_I64,
        VALUE_PUBKEY => DATA_REG_PUBKEY,
        VALUE_U128 if choices.below(2) == 0 => DATA_REG_U128,
        _ => [DATA_REG_U8, DATA_REG_U16, DATA_REG_U32, DATA_REG_U64][choices.below(4)],
    };
    Segment::Register(encoding, register)
}
```

  Generated programs only hold the five scalar types, so there is no `bytes` case.

- [ ] **Step 2: Run the generator properties, on the host and on chain.**

Run: `cargo test -p ballista-common --features proptest --test generated`

Expected: PASS.

Run:

```bash
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
PROPTEST_CASES=256 cargo test --manifest-path tests/ballista/Cargo.toml generated_programs_never_hit_structural_errors
```

Expected: PASS. Over 2,000 programs generated from random choices, the prototype found:
- `EMIT` in about a third of them, about one in four of those inside a loop body;
- `SET_RETURN_DATA` in about 45%.

Output failures are narrowing overflows, 6013, which `ALLOWED_RUNTIME_ERRORS` already permits.

- [ ] **Step 3: Commit.**

```bash
git add common/src/template/generate.rs
git commit -F- <<'EOF'
Generate programs that log and return data

Operations may now be an EMIT of one to three registers, loop bodies
included, and a generated program may end by setting its return data.
The on-chain property then covers both opcodes: whatever the verifier
accepts, the executor runs without a structural error.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 10: The `output` fixture, end to end

**Files:**
- `clients/js/src/fixtures.test.ts`: the `fixtures` map, directly before `'event-flag': () =>`.
- `common/src/template/verify.rs`: `every_shared_fixture_parses_and_verifies`.
- `tests/ballista/Cargo.toml`, `tests/ballista/Cargo.lock`.
- `tests/ballista/src/lib.rs`:
  - the imports;
  - `fixture()`;
  - `event_flag_does_not_change_run_semantics`, whose comment at line 739 wrongly says that Mollusk
    cannot expose logs;
  - three new tests and two helpers.

- [ ] **Step 1: Add the fixture.** In `fixtures.test.ts`, directly before `'event-flag': () =>`:

```ts
  output: () =>
    defineTemplate({
      inputs: { amount: { type: 'u64' }, memo: { type: 'bytes', maxLength: 16 } },
      accounts: { ...systemPrograms, payer: { signer: true, writable: true } },
      batch: { maxIterations: 2, minIterations: 1, row: { recipient: { writable: true } } },
      steps: [
        step.let('paid', expression.u64(0)),
        step.forEach(
          [
            systemTransfer({
              systemProgram: account.fixed('systemProgram'),
              from: account.fixed('payer'),
              to: account.iteration('recipient'),
              lamports: expression.input('amount'),
            }),
            // The next row sends the transfer's data again without encoding it, so this log
            // must leave the invocation's buffer alone.
            step.emit(
              [
                data.literal(Uint8Array.of(0x50, 0x41, 0x49, 0x44)),
                data.encode('u8', expression.loopIndex()),
                data.encode('pubkey', expression.accountField(account.iteration('recipient'), 'key')),
              ],
              'logRow',
            ),
            step.assign('paid', expression.add(expression.variable('paid'), expression.input('amount'))),
          ],
          { carry: ['paid'] },
        ),
        step.emit([data.encode('bytes', expression.input('memo'))], 'logMemo'),
        step.setReturnData(
          [
            data.encode('u64', expression.variable('paid')),
            data.encode('pubkey', expression.accountField(account.fixed('payer'), 'key')),
          ],
          'returnTotal',
        ),
      ],
    }),

```

Why this fixture catches a regression:
- The transfer's data is a literal and a hoisted input, so it is loop-invariant, and the second row
  reuses it without encoding it.
- If `EMIT` ever wrote the invocation's buffer, the second transfer would send `PAID…` as System
  Program data and fail.
- `0x50 0x41 0x49 0x44` is ASCII `PAID`.

- [ ] **Step 2: Regenerate the fixtures, and list the payload for the Rust verifier.**

Run: `pnpm fixtures && git status --short fixtures`

Expected: `?? fixtures/output.hex` and ` M fixtures/manifest.json`, and nothing else. On phase 1's
compiler the prototype's entry recorded:
- 384 payload bytes and 13 instructions;
- 7 registers and 1 CPI;
- `maxExpandedCpis` 2 and `maxCpiDataLength` 12.

In `verify.rs`, `every_shared_fixture_parses_and_verifies`:
- add one to the array length in `let fixtures: [(&str, &str); …]`;
- add this line after the `math-ops` entry:

```rust
            ("output", include_str!("../../../fixtures/output.hex")),
```

Run: `cargo test -p ballista-common every_shared_fixture_parses_and_verifies`

Expected: PASS.

- [ ] **Step 3: Add the log dependencies.** In `tests/ballista/Cargo.toml`:
  - after `ballista-common = …`, add `base64 = "=0.22.1"`;
  - after `solana-signature = …`, add
    `solana-svm-log-collector = { version = "=4.1.1", features = ["agave-unstable-api"] }`.

The lock already holds both crates, the first as a transitive dependency and the second through
Mollusk. The crate's library is empty without the `agave-unstable-api` feature.

Run: `cargo check --manifest-path tests/ballista/Cargo.toml --tests && git diff --stat tests/ballista/Cargo.lock`

Expected: `Finished`, and `tests/ballista/Cargo.lock | 2 ++`. Those two lines add `base64` and
`solana-svm-log-collector` to `ballista-integration-tests`' dependency list. No version changes.

- [ ] **Step 4: Write the end-to-end tests.** In `tests/ballista/src/lib.rs`, `mod tests`:

  (a) Imports:
  - Replace `use std::collections::HashMap;` with the first block below.
  - Add the second line after `use solana_sdk_ids::system_program;`.

```rust
    use std::{cell::RefCell, collections::HashMap, rc::Rc};

    use base64::{engine::general_purpose::STANDARD, Engine as _};
```

```rust
    use solana_svm_log_collector::LogCollector;
```

  (b) In `fixture()`, after the `"math-ops"` arm:

```rust
            "output" => include_str!("../../../fixtures/output.hex"),
```

  (c) In `event_flag_does_not_change_run_semantics`:
  - Replace the doc comment with:

```rust
    /// The event flag adds one data log after a successful run and changes nothing else. Mollusk
    /// records program logs once it is given a log collector, so the event is read back from its
    /// `Program data:` line.
```

  - Replace `let context = context(funded_accounts([creator, payer, recipient], 10_000_000_000));`
    with:

```rust
        let mut context = context(funded_accounts([creator, payer, recipient], 10_000_000_000));
        let logger = LogCollector::new_ref();
        context.mollusk.logger = Some(logger.clone());
```

  - Directly after `assert_eq!(lamports(&context, recipient), before + 1_000);`, add:

```rust
        // Magic, bytecode version, iterations, invokes reached, the mask of those that ran, and
        // the template address.
        let mut event = b"BEV1".to_vec();
        event.extend_from_slice(&[1, 0, 1]);
        event.extend_from_slice(&1u64.to_le_bytes());
        event.extend_from_slice(template.as_ref());
        assert_eq!(program_data(&logger), vec![event]);
```

  (d) Directly before `fn generated_programs_never_hit_structural_errors` and its doc comment, add:

```rust
    /// The output opcodes as the TypeScript SDK compiles them, run on chain. Each row logs its
    /// index and recipient right after a transfer whose data the next row sends again without
    /// encoding it, so a log that wrote the invocation's buffer would break the second transfer.
    /// The run then logs the memo and returns the total paid and the payer.
    #[test]
    fn typescript_output_fixture_logs_every_row_and_returns_the_total() {
        let creator = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let first = Pubkey::new_unique();
        let second = Pubkey::new_unique();
        let mut context = context(funded_accounts([creator, payer, first, second], 10_000_000_000));
        let payload = fixture("output");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 91, &payload))
            .program_result
            .is_ok());
        let (template, _) = find_template_pda(&creator, 91);
        let logger = LogCollector::new_ref();
        context.mollusk.logger = Some(logger.clone());
        let before = (lamports(&context, first), lamports(&context, second));

        let result = context.process_instruction(&run_instruction(
            template,
            vec![
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(payer, true),
                AccountMeta::new(first, false),
                AccountMeta::new(second, false),
            ],
            &output_fixture_inputs(),
        ));
        assert!(result.program_result.is_ok(), "{result:#?}");
        assert_eq!(lamports(&context, first), before.0 + 1_000);
        assert_eq!(lamports(&context, second), before.1 + 1_000);

        let row = |index: u8, recipient: Pubkey| {
            let mut line = b"PAID".to_vec();
            line.push(index);
            line.extend_from_slice(recipient.as_ref());
            line
        };
        assert_eq!(
            program_data(&logger),
            vec![row(0, first), row(1, second), b"hello".to_vec()]
        );
        let mut returned = 2_000u64.to_le_bytes().to_vec();
        returned.extend_from_slice(payer.as_ref());
        assert_eq!(result.return_data, returned);
        eprintln!("output fixture compute units: {}", result.compute_units_consumed);
    }

    /// Return data survives the callee's return: a template that runs the output fixture through
    /// Ballista reads the total it set, directly after the invoke, then returns its own value.
    #[test]
    fn a_nested_template_reads_the_return_data_its_callee_set() {
        let creator = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let first = Pubkey::new_unique();
        let second = Pubkey::new_unique();
        let context = context(funded_accounts([creator, payer, first, second], 10_000_000_000));
        let inner_payload = fixture("output");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 93, &inner_payload))
            .program_result
            .is_ok());
        let (inner, _) = find_template_pda(&creator, 93);

        let mut builder = ProgramBuilder::new();
        let ballista = builder.account(ACCOUNT_EXECUTABLE, Some(ID.to_bytes()), None, 0);
        let template = builder.account(0, None, Some(ID.to_bytes()), 80);
        let system =
            builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
        let source = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
        let to_first = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let to_second = builder.account(ACCOUNT_WRITABLE, None, None, 0);
        let mut data = vec![IX_RUN];
        data.extend_from_slice(&output_fixture_inputs());
        let literal = builder.blob(&data);
        let cpi = builder.cpi(
            ballista,
            &[
                (template, 0),
                (system, 0),
                (source, ACCOUNT_SIGNER | ACCOUNT_WRITABLE),
                (to_first, ACCOUNT_WRITABLE),
                (to_second, ACCOUNT_WRITABLE),
            ],
            &[Segment::Literal(literal)],
        );
        builder.invoke(cpi, None);
        let paid = builder.return_data(OP_READ_U64, 0);
        let expected = builder.const_u64(2_000);
        let same = builder.binary(OP_EQ, paid, expected);
        builder.require(same);
        let doubled = builder.binary(OP_ADD, paid, paid);
        builder.set_return_data(&[Segment::Register(DATA_REG_U64, doubled)]);
        let outer_payload = builder.build().expect("builds");
        assert!(context
            .process_instruction(&create_template_instruction(creator, 94, &outer_payload))
            .program_result
            .is_ok());
        let (outer, _) = find_template_pda(&creator, 94);

        let result = context.process_instruction(&run_instruction(
            outer,
            vec![
                AccountMeta::new_readonly(ID, false),
                AccountMeta::new_readonly(inner, false),
                AccountMeta::new_readonly(system_program::id(), false),
                AccountMeta::new(payer, true),
                AccountMeta::new(first, false),
                AccountMeta::new(second, false),
            ],
            &[],
        ));
        assert!(result.program_result.is_ok(), "{result:#?}");
        assert_eq!(result.return_data, 4_000u64.to_le_bytes());
    }

    /// Return data that a later invoke would erase is refused when the template is created, with
    /// the output error and the index of the instruction that sets it.
    #[test]
    fn return_data_set_before_an_invoke_is_rejected_at_create() {
        let creator = Pubkey::new_unique();
        let context = context(funded_accounts([creator], 10_000_000_000));
        let mut builder = ProgramBuilder::new();
        let system =
            builder.account(ACCOUNT_EXECUTABLE, Some(system_program::id().to_bytes()), None, 0);
        let value = builder.const_u64(1);
        let at = builder.set_return_data(&[Segment::Register(DATA_REG_U64, value)]);
        let cpi = builder.cpi(system, &[], &[]);
        builder.invoke(cpi, None);
        let payload = builder.build().expect("builds");
        let result =
            context.process_instruction(&create_template_instruction(creator, 95, &payload));
        assert_eq!(custom_code(&result), Some(((at as u32) << 16) | 6130), "{result:#?}");
    }

    /// Run data for the output fixture: 1,000 lamports per row and the memo `hello`.
    fn output_fixture_inputs() -> Vec<u8> {
        let mut inputs = 1_000u64.to_le_bytes().to_vec();
        inputs.extend_from_slice(&5u16.to_le_bytes());
        inputs.extend_from_slice(b"hello");
        inputs
    }

    /// The bytes of every `Program data:` line the collector recorded, in order. Ballista logs one
    /// field per line, which the runtime writes as base64.
    fn program_data(logger: &Rc<RefCell<LogCollector>>) -> Vec<Vec<u8>> {
        logger
            .borrow()
            .get_recorded_content()
            .iter()
            .filter_map(|line| line.strip_prefix("Program data: "))
            .map(|field| STANDARD.decode(field).expect("base64 field"))
            .collect()
    }
```

How these tests work:
- **The log collector.** Mollusk's `process_instruction` hands `mollusk.logger` to the invoke
  context. The runtime writes each `sol_log_data` call as `Program data: <base64 per field>`, and
  `InstructionResult::return_data` is the transaction's return data after the instruction.
- **The nested test.** The outer template invokes Ballista itself, which the runtime allows as direct
  self-recursion. Its `RETURN_DATA` accepts the inner run's data because the setter's program ID,
  Ballista, is the program the outer template just invoked.

- [ ] **Step 5: Build and run.**

Run:

```bash
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
cargo test --manifest-path tests/ballista/Cargo.toml -- output event_flag nested_template_reads rejected_at_create
```

Expected: PASS, 4 tests:
- `typescript_output_fixture_logs_every_row_and_returns_the_total`;
- `a_nested_template_reads_the_return_data_its_callee_set`;
- `return_data_set_before_an_invoke_is_rejected_at_create`;
- `event_flag_does_not_change_run_semantics`.

Run: `cargo test --manifest-path tests/ballista/Cargo.toml`

Expected: PASS, the whole suite, ceilings included.

- [ ] **Step 6: Commit.**

```bash
git add clients/js/src/fixtures.test.ts fixtures/output.hex fixtures/manifest.json common/src/template/verify.rs tests/ballista/Cargo.toml tests/ballista/Cargo.lock tests/ballista/src/lib.rs
git commit -F- <<'EOF'
Run a TypeScript-compiled output fixture end to end and read its log

The fixture logs each batch row between two sends of a cached transfer
payload, logs a memo, and returns the total and the payer. Mollusk
records program logs once it has a log collector, so the test reads
every Program data line back, and the event-flag test now reads the run
event the same way instead of claiming logs are out of reach. A nested
template reads the fixture's return data through RETURN_DATA, and a
template that sets return data before an invoke is refused at create
with 6130.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 11: Formal specifications

**Files:**
- `certora/ballista-specs/src/rules/util.rs`: `writes_destination`, a test, and the ABI comment.
- `certora/ballista-specs/envs/cvlr_inlining.txt` and `certora/ballista-specs/envs/cvlr_summaries.txt`.

`errors.rs` already gained `InvalidOutput` in Task 6. `typing.rs`'s `value_dependent` needs nothing
new: an accepted output fails only on a narrowing overflow, `ArithmeticOverflow`, which is already
listed.

- [ ] **Step 1: Write the failing test.** In `util.rs`'s `tests` module, directly before
  `fn pick_yields_the_first_option_under_the_host_runtime`:

```rust
    #[test]
    fn outputs_do_not_write_a_destination() {
        assert!(!writes_destination(OP_EMIT));
        assert!(!writes_destination(OP_SET_RETURN_DATA));
        assert!(writes_destination(OP_MOVE));
    }
```

- [ ] **Step 2: Run it and confirm that it fails.**

Run: `cargo test -p ballista-specs --features rt --manifest-path certora/Cargo.toml --lib util`

Expected: FAIL in `outputs_do_not_write_a_destination`, an assertion failure on
`!writes_destination(OP_EMIT)`.

- [ ] **Step 3: Implement.**

In `util.rs`, replace `writes_destination` with:

```rust
/// Whether an accepted instruction writes its destination register. The output opcodes name no
/// register at all: the verifier requires `dst` to be `NO_INDEX`.
pub fn writes_destination(opcode: u8) -> bool {
    !matches!(
        opcode,
        OP_REQUIRE | OP_INVOKE | OP_FOREACH | OP_EMIT | OP_SET_RETURN_DATA
    )
}
```

If phase 2 added `OP_REPEAT` to that list, keep it.

In `abi_sizes`, change the comment `// FixedSink::push_bytes: three 32-bit words through r1.` to:

```rust
        // The three ByteSink::push_bytes implementations: three 32-bit words through r1.
```

In `cvlr_inlining.txt`, replace the byte-sink block (the comment that begins
`;; Both byte sinks copy a register's bytes` and its two `#[inline(never)]` lines) with:

```text
;; The three byte sinks copy a register's bytes with a length the prover cannot fix statically:
;; PDA seeds into a stack buffer, CPI data into a vector, and EMIT and SET_RETURN_DATA output into
;; a vector of its own. The program keeps every `push_bytes` out of line in spec builds (cfg_attr on
;; the spec-api feature) so they can stay external here. What they would have written only feeds
;; the PDA search, the CPI and the output syscalls, none of which any rule inspects.
#[inline(never)] ^<ballista::processor::execute::FixedSink as ballista::processor::execute::ByteSink>::push_bytes$
#[inline(never)] ^<alloc::vec::Vec<u8> as ballista::processor::execute::ByteSink>::push_bytes$
#[inline(never)] ^<ballista::processor::execute::OutputSink as ballista::processor::execute::ByteSink>::push_bytes$
```

In `cvlr_summaries.txt`, after the `Vec<u8>` sink's summary (its last line is
`^<alloc::vec::Vec<u8> as ballista::processor::execute::ByteSink>::push_bytes$`), append:

```text
#[type((*i32)(r1+0):num)]
#[type((*i32)(r1+4):num)]
#[type((*i32)(r1+8):num)]
^<ballista::processor::execute::OutputSink as ballista::processor::execute::ByteSink>::push_bytes$
```

No other entry is needed:
- `write_output` is ordinary code for the prover.
- Its syscalls, `sol_log_data` and `sol_set_return_data`, are modelled the way `emit_event`'s already
  are; `emit_event` has no entry either.
- Its allocation goes through `__rust_alloc`, which the inlining file already keeps external.

- [ ] **Step 4: Typecheck, test, and check the frames.**

```bash
cargo check -p ballista-specs --features rt --manifest-path certora/Cargo.toml
cargo test -p ballista-specs --features rt --manifest-path certora/Cargo.toml
(cd certora/ballista-specs && cargo certora-sbf --tools-version v1.53 2>&1 | grep -i -E "stack|frame|error" || true)
```

Expected:
- `Finished`.
- The util tests pass, including `outputs_do_not_write_a_destination`.
- No output from the grep.

`cargo certora-sbf` compiles for SBPF v0 and reports any frame over 4 KiB; `cargo build-sbf` does not.
`write_output` keeps no array on the stack. Do not pass `--no-build`: that flag skips the build this
check needs.

- [ ] **Step 5: Commit.**

```bash
git add certora/ballista-specs
git commit -F- <<'EOF'
Specify that the output opcodes write no register

writes_destination excludes EMIT and SET_RETURN_DATA, and the output
sink stays external to the prover like the other two byte sinks.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

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
cargo check -p ballista-specs --features rt --manifest-path certora/Cargo.toml
cargo test -p cvlr-pinocchio --features rt --manifest-path certora/Cargo.toml
cargo clippy -p ballista -p ballista-common --all-targets 2>&1 | grep -E "^(warning|error)" | sort | uniq -c
git diff --check
```

Expected: every command exits 0, and clippy shows no warning beyond Task 0's summary. The base
already carries these warnings, none in code this plan wrote:
- `len_without_is_empty` on `DataSegment`;
- `explicit_auto_deref` in `wire.rs`;
- `cloned_ref_to_slice_refs` in `lib.rs`;
- `type_complexity` and `chunks_exact_to_as_chunks` in `verify.rs`'s tests.

- [ ] **Step 2: Check the numbers once more.**
  - `grep -n "OP_EMIT: u8 = 62\|OP_SET_RETURN_DATA: u8 = 63" common/src/template/wire.rs` shows both
    constants.
  - `tail -3 fixtures/verifier-error-names.txt` shows `TooManyAccountGroups`, `InvalidLoop` and then
    `InvalidOutput`.
  - `grep -rn "6131" clients/js/src/errors.test.ts clients/rust/src/lib.rs` shows the two
    first-unused assertions.

- [ ] **Step 3: Report.** Give:
  - the commits;
  - every ceiling that moved, and why, with Task 8's table;
  - the math fixture's cost before and after;
  - that the branch must merge after phase 2, and before phase 4. Phase 4 moves the first unused
    verifier code to 6132, and its unknown-opcode sweeps keep 75 and `0xfe`.
