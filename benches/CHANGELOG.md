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
| Nine fixed cases | `pnpm cu:bench` | `benches/compute_units.md` |
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
