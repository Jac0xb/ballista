# Verification report

2026-10-03 · PR #8 (`claude/verification`, head `f1acfa1`) · internal record for the owner.
Statuses per property are in [the safety-property list](../specs/2026-10-03-safety-properties.md);
the public summary is `docs/guide/formal-verification.md`.

## In short

- **Three production fixes shipped:** a verifier ordering rule for registry opens, refusal of
  non-canonical encodings, and the carried-alias fix in both template compilers. The fourth
  approved change, `#[repr(C, u32)]` on `RunError`, was dropped.
- **Certora proved 19 rules at `cb2fb2d`**, under the bar below. None has been re-run against this
  PR's verifier, and CI has no `CERTORAKEY` secret, so CI has never run the prover.
- **The prover found no program bug.** Its four violations were prover or model artifacts.
- **Mutation evidence backs 25 of 98 properties.** 41 are asserted by a test that no mutant has
  confirmed; the first draft called 65 "tested" and 7 "proved".
- **Uploads cost 7.9% more compute** (the canonical-encoding checks); runs are unchanged.

## What was built

- **Safety-property list** (FV1): 98 properties plus G1–G8 from Solana, findings F1–F11, mutation
  evidence, next checks per tool.
- **Verifier fuzzing** (FV2, `fuzz/`, own workspace): targets `parse`, `verify`, `structured`,
  `differential`; a stable support crate with a wire model, a reference checker and CPI-ceiling
  pass that share no code with `verify.rs`, single-rule breaks, a generator and a mutator; 87
  seeds; `scripts/mutants.py` (21 verifier mutants); coverage and regression tooling. Group rules
  added in `1c442ec`.
- **Executor and lifecycle fuzzing** (FV3, `fuzz-executor/`, `tests/ballista/src/fuzz/`):
  generators for aliasing, mutations, nested runs and opens around CPIs; `model.rs`, an independent
  interpreter predicting CPIs and return data; an SBF probe program that logs CPI flags and really
  resizes; Mollusk executor differential, lifecycle state machine, mutation guards, registry-ordering
  repros; native targets `run_executor` and `run_template_bytes`. Every finding is hard, structural
  codes fail the run, and per-count floors stop a generator from quietly comparing too little.
- **Compiler fuzzing** (FV4): `compiler-fuzz.test.ts` (seeded generator, mutator, own decoder, and
  a register-reuse replay over its own operand table, which throws on an unknown opcode);
  `common/tests/compiler_corpus`; `tests/ballista` `compiler_fuzz` (natural vs forced reuse vs
  alias-materialized compiles on Mollusk); fixtures `compiler-fuzz-{corpus,findings}.json`. Group
  opcodes added in `5219ff0`.
- **Kani** (FV5, `kani/`): 97 harnesses over the program source as an `rlib` with `spec-api`,
  heavy ones split and windowed (`bcb85f8`). The README's result table is still being filled.
- **Certora** (FV6, FV6b, FV8, FV8b, `certora/`): 112 rules, each in exactly one conf (host test);
  twins, per-branch reachability rules and `rule_sanity: basic`; `scan-stack-copies.py`;
  `check-results.py`; `build-sbf.sh`; the host typing enumeration; the spec-only hook
  `Scratch::set_last_invoked_for_spec`.
- **CI**: the probe is built before FV3 runs; nightly proptests get `--features proptest` (F5);
  nightly jobs for the fuzz targets, FV3 (normal and limits mode), the compiler fuzzer and both
  mutant sets; `certora.yml` always runs the spec tests, `build-sbf.sh` and the enumeration, with
  pinned tools and an inverted gate for twins; a non-gating release-hash check.
- **Critic commits taken:** `1323e3e` (lost-write demo), `124f414` (FV3 fails on structural errors;
  section-order test), `a278637` (generator reach tool), `123f9c3` (mainnet feature switch),
  `f6fdf4c` (loop-restore guards and mutants).

## What ran

| Check | Volume | Result |
| --- | --- | --- |
| Verifier fuzz | `parse` 167M/20 min, `verify` 211M/40 min, `structured` 70M/70 min, `differential` 205M/70 min | No crash; corpora cover every `verify.rs` line but `verify_single_instruction` |
| CPI ceiling pass | ≥ 6.3M accepted programs, 3.8M invoke sites, 37k at exactly 64 CPIs | Per-slot ceiling holds |
| Verifier mutants | 21, one rule each | All caught by the stable tests and by `structured` within 22 s |
| Executor, Mollusk | 200,000 cases; 24,043 succeeded; 13,827 matched the model, 9,917 CPIs byte for byte | 0 violations |
| Executor, native | `run_executor` 3.9M runs, `run_template_bytes` 53.2M runs, 11 min each | No crash |
| Lifecycle | 120,000 steps | Matched the state model |
| SBF coverage | FV3 68.5% at 1,500 cases; Mollusk suite 64.2%; combined 75.9% | — |
| Typing enumeration | 564,756,480 verifier calls, 100,728,150 executions, 55 of 76 opcodes | 0 violations; FV3's runs reach the other 21 with no structural failure |
| Compiler fuzz | 20k seeds + 40k mutants (JS, 91 s); 5k Rust seeds, 15,986 verified, 15,955 Mollusk runs | One bug (carried alias); 13/13 operand-table mutants caught |
| Certora, `cb2fb2d` | 6 jobs, certora-cli 8.19.2 | 19 proved; 4 violations, all artifacts |
| Kani | 97 harnesses at `bcb85f8` | Table pending. An earlier run at `af60075`: 47 passed, 3 unwinding failures, 20 timeouts, 9 never run |

Certora jobs: `run.conf` 48efedc3 (17/17 verified); `run-candidates.conf` 88f0a104 (27 of 29);
`run-candidates-memcmp.conf` 7225c27f (2/2); twins 705ecbe0 and d142c280 (14/14 violated, as
required); `run-blocked.conf` 8da461a1 (22 verified, 15 violated, 13 no result at 38 min). Triage:
aac2bacd, a83bc7fc.

**The bar for "proved":** the rule verified, its sanity check passed, every reachability rule it
has passed, and its twin failed. Proved at `cb2fb2d`: sections tile the payload; `u128` sub, mul,
div, min, max and the `u128` → `i64` cast; registry opens (writable, size, template and key under
`-solanaOptimisticMemcmp`), fields need an open entry, a second open fails; return-data provenance
and emptiness; signers, pinned address and owner, minimum length, account count, account header
reads. `u128` add verified, but its overflow branch is unreachable in the model. The other 14
`run.conf` rules verified with no twin or reachability rule.

## Findings and outcomes

### Production fixes

1. **Registry ordering, and the lost write** (FV3, invariants critic, threat critic). An entry's
   borrow mark exists only from its open, and `refuse_entry_data` checked only the entry's own
   slot. A template built without the compilers could read the entry's raw bytes through a second
   slot, run itself nested before its open, then open and write: the nested run's write was lost
   (demonstrated: field start+1 vs control start+2). **Fixed** in `728159f`: the verifier refuses
   an `OPEN_REGISTRY` with any `INVOKE` before it (6132 at upload). FV2's checker states the rule,
   and its mutant is caught. A `CREATE_PDA` may still come first; it calls only the System program.
2. **Carried alias, in both compilers** (FV4). In a loop body, `let` or `snapshot` of a carried
   variable bound its register, so a later `assign` changed the binding: `total - before <= cap`
   always passed (a cap bypass), `before + 1 == total` always failed. User-written templates only;
   no shipped example or helper used the pattern. **Fixed**: TypeScript `06f1ecf`, Rust `af0b10b`;
   20,000 seeds with 4,743 pattern cases show no difference, and both finding documents compile to
   the TypeScript bytes.
3. **Non-canonical encodings** (FV1 F4, FV2, invariants critic). A stored template is never
   re-verified, so an ignored field is accepted for good. **Fixed** in `4f8aebe`: one operand table
   requires `0xff` (zero for the immediate) in every unused field; `verify_cpi` checks invocation
   data with `verify_segment` (no register on a literal, no blob offset or length on a register
   segment); `InvalidDataSegment` names the table index, not one relative to the descriptor;
   unreferenced segments and uninvoked descriptors are refused; `verify_single_instruction` returns
   `TooManyRegisters` instead of panicking over 64 registers. The 87 real templates and 32,067
   corpus payloads verify as before.

**Compute.** Uploading the 32 cookbook examples went from 145,299 to 156,709 CU (+7.9%); the
payroll-30 create ceiling from 4,486 to 4,709. Runs are unchanged. Release `.so` 90e84b93… →
1428ffc4…; the group merge changes it again, so the hash fixture needs regenerating.

**Dropped: `#[repr(C, u32)]` on `RunError`.** It was meant to let the prover follow error kinds
through stack copies. Measured: 362 copies gone, +731 CU over the bench (+0.74%, worst +2.7%), both
ceiling tests would need raising, binary −200 bytes. The `cb2fb2d` run then showed the prover keeps
only a copied word's first store (`rule_stack_word_copy_keeps_both_halves` violated,
`..._keeps_a_short_tag` verified), so two four-byte halves would not survive either. No benefit,
so the owner dropped it (`59b5200`).

### Docs corrected

- The privilege ceiling is per slot, not per address (F6); group members take the writability of
  Ballista's own instruction, which a calling program chooses.
- Anyone can make an account Ballista-owned; it is inert (F7).
- Compute is not bounded: a 22-instruction template with no CPI exhausts 1.4M CU (F8).
- The trust model's registry claim held only for compiled templates until `728159f`.
- All in `322aa53` and `728159f`; the formal-verification page and the security page's
  verification lines now match the evidence (this branch).

### Low findings, open unless noted

- F2: `TypeMismatch` (6012) also fails a value (a `bool` byte above 1); oracles must not treat it as
  structural. The enumeration's oracle (`oracle.rs`) splits the two; the Certora typing rules'
  error set still needs widening.
- F3: reading an open entry through another slot fails with Solana's `AccountBorrowFailed`, no
  Ballista code.
- F9: `explainRunError` names an outer step for a failure inside a nested run (test skipped).
- Run data is capped at 1,024 bytes, but the verifier allows 256 input values, so a template can
  declare more rows than a run can carry.
- Compiler messages: more than 256 invokes reports "Expected u8"; a fixed read offset near
  `u32::MAX` reports "Expected u32"; about 1,500 levels of nesting overflow Zod's stack; a
  `__proto__` name is dropped silently.
- **New here, P98:** nothing deduplicates group members, so one account passed twice counts twice
  in `GROUP_LENGTH` and `GROUP_COUNT`. Not documented, not tested.

## The critics, and what came of it

| Critique | Outcome |
| --- | --- |
| Nothing was proved against current code; the prover never ran in CI | Six jobs at `cb2fb2d`, 19 proved under the bar. **Open:** re-run at this PR; add the CI secret |
| The section rule was vacuous (`cvlr_satisfy!(true)`) and wrong (F1) | Fixed; proved at `cb2fb2d` |
| "Proved" needs a job, sanity, reachability and a failing twin | Adopted: README bar, `check-results.py`, the property list |
| A prover panic silently cuts a path (`solanaAssertOnPanic` off); a `checked_add` → `+` mutant still "proves" P70 | **Open**: the flag is still unset; P70 is now "tested" by its mutant, not "proved" |
| Rules state less than the docs: tiling checks lengths not starts, the ceiling rule reuses the verifier's own constraint, the field rule tries offset 0, return data covers only the read arm | Ceiling rule rewritten (`b7eba8d`), section-order test added (`124f414`). **Open:** the rest, noted in the property rows |
| The Kani hook changed the release binary | Fixed (`83352a5`); non-gating release-hash check in CI |
| The proved binary is not the deployed one (tool version, crate type, features, stack size) | Documented in `certora/README.md`. **Open** |
| Accounts never alias in the models; the run-never-writes-template rule passes with the borrow check deleted | Disclosed. **Open:** P69 unchecked; no Mollusk test passes the template to a CPI |
| FV3 could not fail: soft divergences, any Ballista code accepted, probe not built in CI | Fixed: hard findings, floors, structural codes fail, probe built in CI, flags logged, real resize |
| Tests assert only `is_err()`; deleted checks survive | Guards kill all 7 Mollusk-reachable survivors; FV3 asserts exact kind and context. Statuses now require a mutant. **Open:** F11's `one_shot_open_run_guards_and_privileges` |
| Generators reached 27 of 76 opcodes, no CPIs | Fixed: all three reach all 76 (FV2's all but the clock), before groups; FV3 limits mode |
| The reuse replay shared the allocator's operand table | Own table; the `mulDiv` mutant fails `pnpm test`. **Open:** generate it from `verify.rs` |
| A loop-restore mutant survived every suite | `critic_loops.rs` kills it and three variants |
| Registry guards bypassable through an aliased slot; lost write shown | Fixed by the ordering rule |
| Harness gaps: no mainnet log limit (F10), rent 6,960 vs 5,080, per-instruction sysvar flags, Pyth only at Full | **Open** |
| Signers trust templates: no payload binding, no disassembler, SDKs default to an upgradeable devnet build | **Open**: the SDK items below |
| FV2's oracle was one-directional | Partly: `checker_audit` lists disagreements both ways |

## Open items

1. **Re-prove at this PR's commit.** Every gating conf, the two rewritten rules (ceiling and
   writable, with their reachability rules and twins), `u128` add with
   `-solanaTACSoundSignedMath true`, overflow reachability for `run.conf`'s `u64` and `i64`
   arithmetic, and the 13 blocked rules without a result (`mul_div`, typing, lifecycle). Save each
   job's URL; check passing satisfy traces for "Imprecision detected".
2. **Group expressions.** Work through
   [the test plan](../plans/2026-10-03-group-expressions-testing.md): mutants for the owner, floor,
   match and except tests (P96 is asserted only); Kani on `GroupFilter`; the TypeScript and Rust
   compilers compared on group documents (P97); groups in the typing enumeration and FV3's model
   (P40); a decision on duplicates (P98).
3. **The Certora key.** It sits in `/Users/jacob/Documents/ballista/.env`, a tracked file, as an
   uncommitted line; no commit has ever held it. Move it to an ignored file, and add `CERTORAKEY`
   as a repository secret so CI runs the prover.
4. **SDK changes, as the threat critic ranked them:**
   1. a disassembler with lints, listing each CPI's program, signers, writables and data source
      (medium);
   2. refuse to build a run unless the template is finalized and its hash matches (small);
   3. no default program address (small, breaking);
   4. an expected payload hash in `Run` (small, mostly redundant with 2);
   5. a lint for per-address privilege through a second slot or a group;
   6. nested-run error attribution (small, F9).
5. **Smaller:** set `solanaAssertOnPanic`; run the protocol scenarios at the mainnet log limit;
   generate the compiler's operand table from `verify.rs`; add `docs/guide/formal-verification.md`
   to the Security sidebar in `docs/.vitepress/config.mts`; finish the Kani README table.
