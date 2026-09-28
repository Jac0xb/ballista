# Compute-unit changelog

Every change made to cut the compute units (CU) a Ballista run costs, newest first, with the
measurements that justify it. Ideas that did not pay off are recorded too, so nobody measures them
twice.

## How to measure

Every number here comes from the Mollusk harness in `tests/ballista`, so it reproduces on any
machine. Build the program first with `pnpm build:program`.

| What | Command | Output |
| --- | --- | --- |
| Cost of each feature | `cargo test --manifest-path tests/ballista/Cargo.toml profile_compute_units -- --nocapture` | printed table |
| Thirteen fixed cases | `pnpm cu:bench` | `benches/compute_units.md` |
| Every cookbook example | `cargo test --manifest-path tests/ballista/Cargo.toml measure_every_example -- --nocapture` | one line per example |
| Where one run's compute goes | `pnpm cu:phases` | `fixtures/cu-phases.json` |
| Lock in a win | `pnpm cu:ceilings`, `pnpm benchmarks` | lowers `fixtures/cu-ceilings.json`, `fixtures/example-ceilings.json` |

The ceiling tests fail on any increase. A change that makes something slower raises its ceiling in
the same commit and says why here.

## Entry format

```md
### YYYY-MM-DD · What changed · status (commit or branch)
- **Change:** what the code now does differently.
- **Measured:** before → after for the rows it moves, and the cookbook total.
- **Checked:** the tests showing behavior did not change.
- **Watch:** what could undo it.
```

---

## Pending: runtime extensions, not merged

### 2026-09-28 · Registry entries · `claude/runtime-registry`
- **Change:** opcodes 75 to 77 reach one cold helper through `extended_instruction`'s fallback
  router; the outer dispatch is untouched. `run` takes the instruction's whole account list, so
  the template's address needs no sixth argument on the stack. An open entry is marked in its own
  borrow flag, so the reentry check is the borrow check every CPI already makes, mapped to
  `RegistryReentry` only when an invocation fails.
- **Measured** against the phase-4 tip (`65c9b45`):
  - The nine fixed cases without a CPI: 1 CU less each (`run, empty template` 582 → 581). Every
    case and example with a CPI: unchanged. Cookbook total: 552,304, unchanged.
  - `create template, payroll 30 rows`: 4,483 → 4,486, the verifier's three new arms and the
    review's two rules (no data read of an entry, no byte read of a fixed writable account).
  - `run, registry open and update`, a new case: 1,632, now ratcheted.
  - Uploading the 32 cookbook examples, not ratcheted: 144,999 → 145,299 (+300), from the two
    rules. The entry check scans the program only for a read of a writable account; scanning for
    every read cost +555.
  - Review fixes (an open refuses an entry declared as anything but writable, and any CPI that
    lists its entry writable) add 6 to 22 CU to creating a template that opens an entry:
    3,845 → 3,851 for the Mollusk counter, 3,064 → 3,081 nesting a run, 6,481 → 6,503 for the
    rate-limited transfer fixture; the open scans the CPI account records instead of finding the
    invokes by a pass over every instruction.
- **Measured on the prototype, and not built:** a slot table in `Scratch` cost every run 2 CU,
  in `Machine` 426 on a 30-pass count loop; the literal reentry check (the invoked program is
  Ballista and an entry is open) cost 3 CU a CPI, 89 on `run, payroll 30 rows`.
- **Checked:** the verifier's, executor's and registry module's unit tests; the Mollusk registry
  tests, the rate-limited transfer end to end, the generated-program property tests; both ceiling
  tests.
- **Watch:** a new field in `Machine` can cost the dispatch loop hundreds of units; one in
  `Scratch` costs every run about 2. Keep per-run registry state in the entry account itself.

### 2026-09-27 · Introspection on top of loops and output · `claude/runtime-introspection`
- **Change:** the branch is rebased onto the output tip. `FOREACH`, `REPEAT`, `EMIT` and
  `SET_RETURN_DATA` reach `write_output` through `extended_instruction`'s fallback arm, and rustc
  tests a range pattern apart from the match's switch, on that path. The arm for the seven
  instruction reads now lists its opcodes instead of a range.
- **Measured** against the output tip (`8c93291`):
  - A loop entry: 1 CU less for `FOREACH` and 2 for `REPEAT`; an output: 2 less. That moves the
    four fixed cases with a loop, the output case (1,481 → 1,477) and the 11 examples that loop.
  - The math opcodes case: 2,534 → 2,501. The TypeScript math fixture: 4,309 → 4,276.
  - `create template, payroll 30 rows`: 4,481 → 4,476, and creating a cookbook example moves
    between −15 and +16, from the verifier's new arms.
  - `run, introspection, no cpi`: 3,105, now ratcheted.
  - Cookbook total: 552,315 → 552,304 (−11). Every other case and example: unchanged.
- **Review polish:** `create template, payroll 30 rows` 4,476 → 4,483; every run unchanged.
- **Checked:** every host, SDK and Mollusk test, both ceiling tests, the proptest suites and the
  Certora specs' host tests.
- **Bisecting:** every commit from `a31b686` through `e179e58` fails both ceiling tests, by 5 CU
  per loop entry or output. `9b57112` fixes it.
- **Watch:** a range pattern in `extended_instruction`'s match puts its test on the fallback path,
  which every loop entry and every output takes.

### 2026-09-27 · Introspection and byte opcodes · `claude/runtime-introspection`
- **Change:** eleven opcodes, 64 to 74, reach the executor through `extended_instruction`'s inner
  match, whose outer dispatch is untouched. The count, the index and `BYTES_LEN` run there; the
  sysvar parsing and both byte reads run in two `#[inline(never)]` helpers that take four words
  each, all in registers.
- **Measured** against the phase-1 tip (`1efbd24`):
  - Uploading the 30-row payroll: 4,446 → 4,448, the verifier's new arms.
  - A run using all eleven (`run, introspection, no cpi`): 3,552, now ratcheted.
  - One check per data read, no dead parser arms: 3,552 → 3,488; the settlement, 9,895 → 9,568.
  - The math opcodes case: 2,797 → 2,761. The TypeScript math fixture: 4,712 → 4,676.
  - Every other case and every cookbook example: unchanged.
- **Checked:** the introspection fixture and a signed-quote settlement under Mollusk; every host
  and Mollusk test.
- **Watch:** a helper that takes the loop context as an argument takes words from the stack, and
  their loads move to `extended_instruction`'s entry, where every math opcode pays for them. The
  larger router also stopped LLVM inlining `math::remainder`, which `#[inline(always)]` now pins.

### 2026-09-27 · An output buffer for `EMIT` and `SET_RETURN_DATA` · `claude/runtime-output`
- **Change:** `Scratch` holds the output opcodes' buffer. Every run sets it to `None`, the run's
  first output allocates it at 1,024 bytes, and later outputs reuse it. Both opcodes reach one
  out-of-line, `#[cold]` helper through `extended_instruction`'s fallback arm and encode through a
  sink type of their own.
- **Measured** against the loops tip (`2e300e4`):
  - Every run: +1 CU, the store that sets the buffer to `None`. Five of the ten fixed run cases and
    21 of the 32 cookbook examples rose by exactly 1.
  - Every loop: +8 more. `FOREACH` and `REPEAT` start from the executor's fallback arm, which now
    calls the helper before failing. The four fixed cases with a loop, and 11 examples, rose by 9.
  - The math fixture: 4,321 → 4,309, and the math case: 2,546 → 2,534. Without `#[cold]` on the
    helper, the fixture cost 4,358.
  - Cookbook total: 552,195 → 552,315 (+120).
  - `run, log and return 16 bytes`, a new case: 1,481.
  - `create template, payroll 30 rows`: 4,470 → 4,481, from the verifier's two new arms. Creating a
    cookbook example costs 11 to 48 more.
  - The verifier's check that every `EMIT` starts with a tag outside the run event's family,
    against `02c3017`: no fixed case moved, and no cookbook example's run or create.
- **Measured** on the phase-1 tip (`1efbd24`), what one output costs:
  - A 20-byte `EMIT` in three parts: 429 CU, of which `sol_log_data` charges 220. A run's first
    output also allocates the buffer, about 38 CU more.
  - A 16-byte `SET_RETURN_DATA` as a run's first output: 296 CU, of which `sol_set_return_data`
    charges 100.
- **Checked:** the executor's unit tests, and the Mollusk suite with its ceilings. Later commits on
  the branch run the path on chain: a fixture that logs between two sends of a cached transfer
  payload, and generated programs that log and return data.
- **Bisecting:** `9ec10fc` and `683689f` fail the ceiling test: `create template, payroll 30 rows`
  measures 4,481 against its 4,470 ceiling, which `d6faac0` raises.
- **Watch:** `sol_log_data` charges 100 CU per call, 100 per field and 1 per byte, so a 1,024-byte
  `EMIT` costs 1,224 CU in the syscall alone.

### 2026-09-27 · Enter loops from the failure branch · `claude/runtime-loops`
- **Change:**
  - `dispatch` no longer compares every instruction's opcode with `FOREACH`. The handler already
    fails an opcode it does not run, before reading an operand; at the root, that failure is
    where a `FOREACH` or a `REPEAT` starts its loop.
  - Both loop kinds share one `Loop` state, and every loop of a run snapshots its registers into
    one buffer in `Scratch`.
- **Measured** against the phase-1 tip (`1efbd24`):
  - Ten fixed cases: −1,358 in total. Sum 30 rows without CPIs: 11,047 → 10,350. Oracle band:
    1,083 → 1,037. Math opcodes: 2,797 → 2,546.
  - Payroll 8 rows: 13,371 → 13,349.
  - Create template: 4,446 → 4,470, the verifier's loop rules.
  - Cookbook total: 557,356 → 552,195 (−5,161).
  - A 30-pass count loop costs 8,201.
- **Checked:** every host, SDK and Mollusk test. Eight loops over 64 registers run in the default
  heap only because the snapshot is shared.
- **Watch:** in a prototype on `dedc168`, a field added to `Machine` moved the dispatch loop's
  register allocation and cost +1,686 over the nine fixed cases of the time, even on templates
  without loops. Keep per-run state in `Scratch`.

### 2026-09-27 · Run the new opcodes behind the dispatch loop's fallback arm · `claude/runtime-extensions`
- **Change:** The ten runtime-math opcodes, `MUL_DIV` (51) to `READ_I32` (60), run in one
  out-of-line helper, `extended_instruction`, which the dispatch match reaches through its fallback
  arm. The comparison tree the match compiles to, and the loop's entry, are the same as before the
  opcodes existed. The helper takes the interpreter's state as one pointer, each of its arms stores
  its own result, and multiply-divide has an out-of-line helper of its own. Two commits: "Keep the
  math opcodes out of the dispatch loop's way" (`7a5e15e`) and "Route new opcodes through a thin
  out-of-line helper".
- **Measured** against `e213241`, before the opcodes existed:
  - As arms of the loop (`0fc777c`), every run paid for them: fixed cost 595 → 602, summing 30
    rows 11,017 → 11,085, cookbook total 557,309 → 557,767 (+458).
  - Now: fixed cost 595, summing 30 rows 11,047, cookbook total 557,356 (+47).
  - Each math opcode costs more than as an arm, except multiply-divide: bitwise AND 79 → 104 CU,
    power of ten 58 → 76, multiply-divide 176 → 168. The TypeScript fixture that runs all ten:
    4,589 → 4,712.
  - A new fixed case, "run, math opcodes, no cpi", holds the helper to 2,797 (2,668 as arms).
  - `7a5e15e` raised these ceilings to the measured values: create template 4,443 → 4,446 (the
    verifier's new arms), oracle band 1,082 → 1,083, bump search 2,012 → 2,014, bump supplied
    1,514 → 1,515, summing 30 rows 11,017 → 11,047, and eleven cookbook examples by 1 to 16 CU.
- **Checked:**
  - The Mollusk and host suites pass, with new tests for `READ_I32`'s offsets, row accounts and
    return data, and for opcodes the executor does not run.
  - A differential harness compared 9,744 runs against `0fc777c`: `READ_I32` at every kind of
    offset, every operand shape of the math and unknown opcodes, return data, loop rows, and 400
    generated programs. Results, error codes and accounts were identical.
- **Watch:**
  - The fallback arm must stay one unconditional call, and none of these opcodes may get an arm of
    its own. See Tried and rejected.
  - The comparison arm still costs 2 CU more than at `e213241`, and reading an account's lamports
    1 more, from register allocation.

## Pending: integrated, not merged

The three branches below were measured on their own against `85067e3`, where the 32 cookbook
examples total **654,847 CU**, then integrated on `cu/integrated`. The first entry has the
combined numbers; the entries after it keep what each branch measured alone.

### 2026-09-26 · All three branches together · `cu/integrated` (`e213241`)
- **Change:** `cu/interpreter`, `cu/pda-syscalls` and `cu/run-setup` on one branch, plus one
  commit that locks in the new ceilings.
  - The PDA commit was reworked so both derivations go through one `derive_pda()` helper.
  - The register-file commit was reworked: its slice now bounds the batch snapshot, which the
    interpreter work had moved out of `run`.
  - "Find each batch row's first account without a checked multiply" was dropped. The
    interpreter work had already removed that multiply.
- **Measured** against `85067e3`:
  - Cookbook total: 654,847 → 557,309 (−97,538, −14.9%). Alone, the branches reached 610,934,
    633,873 and 607,671.
  - Fixed cost of a run: 1,014 → 595.
  - A CPI: 1,730 → 1,579. Each account passed to one: 136 → 110.
  - PDA derivation with a bump search: 4,852 → 1,300. With a supplied bump: 1,898 → 769.
  - 30-row payroll: 51,741 → 46,713. Summing 30 rows without CPIs: 18,208 → 11,017.
  - Uploading all 32 templates: 239,217 → 141,775.
  - No ceiling rose.
- **Checked:**
  - 36 Mollusk tests and 61 host tests pass. A second, independent run on `e213241` gave the same
    totals.
  - Differential fuzzing against a fresh `85067e3` build: 560,000 runs of the interpreter harness
    with no difference. An extended harness made about 568,000 comparisons per seed, covering
    upload, headers, inputs, CPIs, PDAs and all 32 examples. It differs only as described under
    Watch.
  - 2,311 on-chain PDA derivations match the host's.
  - No stack frame exceeds 4,096 bytes. The deepest, `bounded_invoke`, uses 3,624.
- **Watch:**
  - The run path now trusts a finalized template's header and input records. Six kinds of
    header corruption now run instead of failing:
    - non-zero reserved bytes in the account header or the program header;
    - a written length that is too short or too long;
    - a wrong payload-length field;
    - an unknown flag bit.

    Templates declaring more than 256 run inputs also decode differently. Only the program can
    write a finalized template, and it verifies every template before finalizing it, so none of
    these can arise through its instructions.
  - Checking an account with no constraint went from 46 to 50 CU in the interpreter work, and
    nothing merged since won it back.
  - The program binary grew from 104,904 to 111,424 bytes.
  - Hand-written figures in `docs/` (1,014, 1,730, 4,852, 1,898 and others) and the generated cost
    tables must be updated when this merges.

### 2026-09-26 · Derive PDAs with `sol_sha256` and the curve syscall · `cu/pda-syscalls`
- **Change:** PDA derivation hashes the seeds with `sol_sha256` and checks the result with
  `sol_curve_validate_point`, instead of calling the runtime's PDA syscalls, which charge 1,500 CU
  for every bump tried. The results are identical to `find_program_address` and
  `create_program_address`.
- **Measured:**
  - One bump attempt: 1,500 → 302 CU with one 32-byte seed.
  - PDA derivation with a bump search: 4,852 → 1,302.
  - PDA derivation with a supplied bump: 1,898 → 772.
  - Template upload (30-row payroll): 6,817 → 4,443.
  - `assert-create-then-transfer`: 191,260 → 171,328.
  - Cookbook total: 654,847 → 633,873 (−20,974).
- **Checked:**
  - 2,311 on-chain derivations are compared against the host's `find_program_address` and
    `create_program_address`. They also pass against the old binary, so the comparison is real.
  - 36 Mollusk tests and 60 host tests pass.
- **Watch:**
  - This depends on `sol_curve_validate_point`, whose feature has been active on mainnet since
    slot 275,184,000.
  - Supplying a bump now saves about 300 CU per skipped attempt instead of 1,500.

### 2026-09-26 · A faster interpreter · `cu/interpreter`
- **Change:**
  - Failure handling is kept out of the hot path.
  - One pointer replaces seven arguments per instruction.
  - Registers are read in place, and every value's payload sits at one offset.
  - The batch body runs inside the same inlined dispatch loop.
  - Checked multiplications stay out of the loop.
- **Measured:**
  - Each instruction costs 40–62% less: constant 86 → 33, checked add 164 → 83, comparison
    158 → 66, assertion 80 → 30.
  - Fixed cost of a run: 1,014 → 809.
  - Summing 30 rows without CPIs: 18,208 → 11,285.
  - Cookbook total: 654,847 → 610,934 (−43,913).
- **Checked:**
  - 30 Mollusk tests and 57 host tests pass at each of the branch's eight commits.
  - 28,000 differentially fuzzed runs against the old binary show no difference in result, error
    code, return data, accounts, or logs.
- **Watch:**
  - The dispatch is one large inlined match, so unrelated edits move instruction costs by a few CU.
  - Validating an account went from 46 → 50 CU, and passing an account to a CPI from 136 → 142.

### 2026-09-26 · Cheaper run setup and CPI assembly · `cu/run-setup`
- **Change:**
  - A finalized template is read without repeating the upload-time checks.
  - Inputs decode straight into their slots. This removes two 128-bit multiply calls at 43 CU
    each, which explains why a run with no inputs used to spend 221 CU parsing them.
  - A CPI's account list is built in one pass, and repeat rows skip the descriptor lookups.
  - A redundant per-account address check in the invoke helper is gone.
  - The last invoked program is kept by reference rather than copied.
- **Measured:**
  - Fixed cost of a run: 1,014 → 632.
  - A CPI: 1,730 → 1,611. Each account passed to one: 136 → 106.
  - 30-row payroll: 51,741 → 47,616.
  - Cookbook total: 654,847 → 607,671 (−47,176).
- **Checked:**
  - 30 Mollusk tests and 56 host tests pass.
  - 31 failure scenarios were compared against the old binary. 28 are identical; the other 3 are
    the intended change described under Watch.
- **Watch:** The fast path trusts a finalized template's header. Only the program itself can write
  that header, so a malformed one cannot be produced through its instructions; if it could, it
  would now run instead of failing.

## Measured, not built

### 2026-09-26 · One batched CPI for many token transfers
- **Idea:** Every CPI costs a flat 946 CU (Agave 4.1). The mainnet token program accepts a `Batch`
  instruction (discriminator 255) that carries many transfers in one call.
- **Measured** (20 transfers from one source):
  - One CPI per row: 35,135 CU.
  - One batched CPI: 15,211 CU, which is 57% less. Each extra transfer costs 631 CU instead of
    1,671.
- **Needs:**
  - A way for a batch to append each row's instruction to a single CPI. The template language
    cannot express this today.
  - A callee that accepts batches: the token program does; the System and ATA programs do not.
  - A way around the limit of 64 accounts per CPI, which caps a batch at about 21 transfers unless
    repeated accounts are passed once.

### 2026-09-26 · Fuse a comparison into the `require` that reads it
- **Estimate:** 42 such pairs appear across all 58 compiled templates, and 49 execute across the
  cookbook. That is under 1% of the cookbook at the old instruction cost, and less after the
  interpreter work. Not worth a new opcode.

## Tried and rejected

- **Interpreter:**
  - One match arm per opcode made the dispatch tree deeper.
  - Keeping the loop state in memory added loads.
  - Inlining the next-row step made loops cheaper but straight-line templates more expensive.
  - A compact register file was not worth breaking the register API the formal specifications use.
- **Run setup:**
  - A register file on the stack: −39 CU overall, with regressions elsewhere.
  - CPI account lists kept inline: +1,585.
  - An allocator without overflow checks: −299 on some examples, +2,060 on others.
  - Keeping account infos between rows: +2,537.
  - A fast path for unconstrained accounts made batch templates slower.
- **PDA derivation:** Writing seeds straight into the hash input saved 120–160 CU per derivation.
  It also moved register allocation in the interpreter: a checked add rose 6 CU, and some examples
  rose by up to 298.
- **New opcodes:**
  - As arms of the dispatch loop: 7 CU more on every run, and up to 68 more on a loop.
  - `#[inline(never)]` on `math::integer` and `math::pow10`: 12 CU more again on every run. The
    loop hoisted the addresses of their return slots to its entry.
  - A case for `READ_I32` in the read arm: 60 CU more on summing 30 rows, a template that never
    reads an i32. One helper behind an arm of its own for the math opcodes measured the same.
  - A guard or `if` on the opcode inside the fallback arm: LLVM folds it into the match as a case
    of its own, and it measured the same as an arm.
  - `#[cold]` on the helper: no better.
- **Loop entry:**
  - Testing `REPEAT` beside `FOREACH` ahead of every instruction: +23 over the ten fixed cases,
    +160 over the cookbook.
  - Keeping `FOREACH`'s test there and recognising only `REPEAT` on the failure branch: −272 and
    −1,085, against −1,358 and −5,161 for both on the failure branch.
  - A scope parameter on every account lookup in the verifier: 56 units on `create template` in
    the prototype, against 24 for one check on each instruction in a count-loop body.
- **Output opcodes:**
  - Encoding outputs through the invocation data's `Vec<u8>` sink gave the encoder a second caller.
    The compiler stopped inlining it into `invoke_cpi`, which cost about +50 CU per invocation and
    +1,600 on `index-weighted-rewards`. Marking both encoders `#[inline(always)]` still cost about
    +13 per invocation.
  - An empty `Vec<u8>` for the buffer cost 3–4 CU per run, and an `Option<Vec<u8>>` cost 2.
  - In a prototype of the `Machine`-shaped router, arms of their own in `extended_instruction` cost
    every math opcode about 7 CU. One helper per opcode gave the encoder two callers again, and
    each output cost about 65 CU more.
  - The helper without `#[cold]`, on the loops tip: the math fixture +37. Keeping `instruction`
    alive for the call cost the math arms spills.
  - In the verifier, both opcodes in one out-of-line helper. Reached from a shared arm, `create
    template` rose 7 rather than 11, but each cookbook example's create moved between −14 and +13
    against the two arms. Reached from the fallback arm, it rose 13.
  - Routing the outputs from `dispatch`'s failure branch, where loops are entered, so that a loop
    entry no longer passes through `write_output`:
    - Calling `write_output` there and then setting `rest = tail` cost +4,462 CU over the 44
      ceiling cases, from register pressure in `dispatch`.
    - Rebuilding `rest` and `loop_context` from `current` after the call was −142 net, but single
      cases moved between −99 and +110, and the output case cost 96 more.
    - The outputs stay behind `extended_instruction`'s fallback arm, and each loop entry keeps its
      +8.
- **Introspection dispatch** (math fixture 4,859 at base, on `7a5e15e`'s layout):
  - Three arms whose helpers took the loop context: +50, from the stack-passed words' loads on
    the router's entry.
  - One `64..=74` arm into one helper: +47, and +42 on an introspecting run.
  - The router's fallback arm into the helper: +47, and +53 on a settlement.
  - Sharing the count-and-index code between the router and the parser: +10 on the ratchet case.
  - On top of loops and output (`8c93291`), the seven instruction reads as one range arm: +5 on
    every loop entry and every output, from the range's own test on the way to the fallback arm.

## Landed

### 2026-09-26 · Reuse a batch's CPI between rows; load inputs and constants once (`e8d6a2f`, `5fe9ca2`)
- **Change:**
  - When every CPI in a batch's loop body is the same call, the account list and the unchanged
    data are built once rather than on every row.
  - The compiler loads inputs and constants once, before the first step.
- **Measured:** Both changes landed together, so they were measured together. The comparison uses
  `e87ff98` against `dc618e2` and excludes the three examples whose cost depends on PDA bump depth.
  The benchmark addresses changed between those commits, which moved those three examples for
  unrelated reasons.
  - The other 17 examples: 365,424 → 330,109 CU (−9.7%).
  - `existing-account-token-payroll`: 64,465 → 55,183.
  - `bounded-sol-payroll`: 59,338 → 51,741.
- **Checked:** The Mollusk suite, including a test that data derived from the row index is rebuilt
  on every row.

### 2026-09-26 · Let PDA and ATA assertions take a known bump (`232d47d`)
- **Change:** `CREATE_PDA` derives the address once from a bump the caller supplies, instead of
  searching for it.
- **Measured:**
  - 1,898 CU against 4,852 for a search three bumps deep, with one literal seed.
  - An ATA assertion now costs the same whatever the bump.
- **Watch:** A supplied bump proves the address comes from those seeds and that bump, not that the
  bump is the canonical one.
