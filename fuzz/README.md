# Verifier fuzzing

Coverage-guided fuzzing of `ProgramView::parse` and `verify` in `ballista-common`. Finalization
trusts these two completely: every template that verifies runs without further checks.

This is a cargo-fuzz crate in its own Cargo workspace, like `certora/`, so the release build and
its lock file are untouched. The shared code is in `support/`, which has no libFuzzer dependency
and tests on stable.

## Run

```sh
fuzz/scripts/run.sh differential 1800   # target, seconds (default 900)
```

The script wraps `cargo +nightly fuzz run -s none TARGET fuzz/corpus/TARGET fuzz/seeds/TARGET`.

- New inputs go to `fuzz/corpus/` and crashes to `fuzz/artifacts/`. Git ignores both.
- `fuzz/seeds/` is read and never written.
- Reproduce a crash with `cargo +nightly fuzz run -s none TARGET fuzz/artifacts/TARGET/crash-…`.
- **No sanitizer by default.** On macOS 26 the AddressSanitizer runtime of the installed nightlies
  deadlocks while the process starts. On Linux, set `FUZZ_SANITIZER=address`. The code under test
  is safe Rust, and debug assertions and overflow checks stay on either way.

Stable checks, no nightly needed:

```sh
cargo test --manifest-path fuzz/Cargo.toml -p ballista-fuzz-support --release
```

They replay every seed, check every real template, and check that `verify` refuses every
negative mutation of each.

Line coverage of the verifier's sources by a target's corpus, with the nightly's `llvm-tools`:

```sh
fuzz/scripts/coverage.sh differential
```

## Targets

| Target | Input | Asserts |
| --- | --- | --- |
| `parse` | Raw bytes, tried as a payload, a template account, and instruction data | No panic. `parse_finalized`, the run's fast path, splits a payload exactly as `parse` does, and refuses only what `parse` refuses for its structure. The independent model splits it the same way and re-encodes it byte for byte. |
| `verify` | A raw payload | No panic. `CreateTemplate` and `FinalizeTemplate` (the payload read back from a template account) reach the same verdict. A verified payload's finalized account is read by the run's fast path. The stats echo the header. Each error maps to a verifier code the SDKs name. |
| `structured` | A program from `generate.rs`, from `support/src/gen.rs`, or from a real template, then 0 to 4 structure-aware mutations | Everything `parse` and `verify` assert. If it verifies, the `differential` checks. Then one rule broken (see [Negative mutation](#negative-mutation)), which `verify` must refuse with that rule's error. |
| `differential` | A raw payload, seeded with real templates. A custom mutator edits it by structure two times in three. | If it verifies: the CPI ceiling pass, the reference checker, equal worst-case CPI counts and data lengths, and one negative mutation. |

## What the checks prove

- **CPI ceiling pass** (`support/src/ceiling.rs`) checks every invoke of an accepted program, and
  is always fatal:
  - each account record is signer or writable only within its slot's declaration, in the
    invoke's scope;
  - every account it names is declared;
  - the program is executable;
  - the loop-expanded CPI count is at most 64 and matches the verifier's.

  This is the one guarantee the run never checks again: `invoke_cpi` copies a record's flags
  as they are.
- **Reference checker** (`support/src/checker.rs`) re-derives each finalization guarantee. It
  follows the executor's operand types and the docs, never `verify.rs`. Register types are
  tracked over every path, and loops run to a fixpoint bounded by their maximum. Each rule has a
  dotted name.
- **Negative mutation** (`support/src/negative.rs`) breaks one rule of an accepted program, and
  `verify` must return that rule's exact error. The breaks:
  - CPIs: a record above its declaration, a record with the executable bit, an undeclared account,
    program or group, and a loop maximum past 64 CPIs;
  - registers: a read before a write, and a type mismatch;
  - outputs: an `EMIT` tag, and `SET_RETURN_DATA` before an invoke, in a loop, or before an open;
  - registries: a writable entry, a ninth open, and an open after an invoke;
  - other limits and pins: a ninth loop, a read past the minimum length, a guarded invoke before
    `RETURN_DATA`, and the sysvar pin;
  - encodings: an unused operand or immediate set, a CPI data segment's unused field set, and an
    unused data segment or descriptor added.

**Per address**, the ceiling is wider, and finalization cannot see it. The caller picks the
address in each slot and the members of each account group.

- An address in a slot declared read-only reaches a callee writable in two ways:
  - the same CPI also passes it in a slot declared writable;
  - the CPI forwards a group holding it, and the transaction marked it writable.

  The runtime merges the flags of an account listed twice in one instruction.
- It reaches a callee as a signer only through a slot declared signer: group members never are.
- A template can guard against the first with `notEqual` on the two keys. It cannot guard against
  the second: group members can't be read or checked.
- `ceiling::address_ceiling` states this bound, and a test pins it.

## Findings

The first long run found four encoding gaps, and a review of the entry points two more. The
verifier now refuses each, and `common/tests/fuzz_findings.rs` asserts the exact error:

- `verify_cpi` checked invocation data segments with its own loop, and skipped the two unused-field
  checks that `verify_segment` makes (rules `cpi.segment-literal-register` and
  `cpi.segment-register-fields`). It now calls `verify_segment`, which also names a bad segment by
  its index in the table, not from the descriptor's first.
- A data segment or CPI descriptor that nothing reached was never checked (`unreferenced.segment`
  and `unreferenced.cpi`). Every one must now be used.
- `verify_single_instruction` panicked on a view with more than 64 registers. It now returns
  `TooManyRegisters`.

The checker also holds every record to the opcode table's unused fields (`format.unused-field`):
`0xff` in each operand an opcode doesn't use, and zero in an unused immediate.

`fuzz/regressions/differential/` keeps one crash per old finding, minimized with
`cargo fuzz tmin`. `harness::KNOWN_FINDINGS` lists rules whose violations the targets print once
and keep fuzzing past, for a finding not fixed yet; it is empty. `BALLISTA_FUZZ_STRICT=1`, or a
comma-separated list of rule names, makes such rules fatal again.

## Seeds

```sh
python3 fuzz/scripts/seeds.py
```

The script rebuilds `fuzz/seeds/` from every template payload in:

- `fixtures/*.hex`;
- `fixtures/protocol-examples.json`, `protocol-scenarios.json` and `benchmarks.json`;
- `clients/rust/tests/fixtures/*.hex` and `docs-examples.json`.

That is 87 distinct templates, about 260 KiB in all.

## Triage

Three examples in `support/examples` help with triage:

- `explain FILE` prints a payload's sections and every verdict.
- `checker_audit TARGET DIR` lists where the checker and `verify` disagree, in both directions. A
  verifier error the checker lacks is a rule neither side can catch. Over the corpora of the first
  long run, about 14,700 inputs, the four encoding findings were the only disagreements.
- `corpus_stats TARGET DIR` counts invoke sites by scope, records checked, and worst-case CPI
  counts.

Run any of them with
`cargo run --release --manifest-path fuzz/Cargo.toml -p ballista-fuzz-support --example NAME -- ARGS`.

## Mutation score

```sh
python3 fuzz/scripts/mutants.py 120
```

The script deletes one verifier rule at a time, in a throwaway worktree. For each, it runs the
stable tests and `structured` from an empty corpus, and reports which catch the deletion and how
fast. A mutant that survives both is a rule nothing here tests.

The 21 mutants cover the CPI ceiling, declared accounts and groups, the CPI count,
read-before-write, read bounds, return data, `EMIT` tags, registry entries and their order, the
sysvar pin, unused fields, data segment fields and indexes, and unused segments and descriptors.
The stable tests catch every one, and so does `structured`, within 22 seconds of an empty corpus
(`python3 fuzz/scripts/mutants.py 90`, 2026-10-03, on a machine running other fuzzers). That
includes the critic's `verify-cpi-privilege`, which the proptests in `common/tests` miss.

## First local run

On 2026-10-03, on a loaded 16-core Mac, without a sanitizer. No target crashed.

| Target | Time | Executions | Edges at the end |
| --- | ---: | ---: | ---: |
| `parse` | 20 min | 167M | 385 |
| `verify` | 40 min | 211M | 910 |
| `structured` | 70 min | 70M | 4,653 |
| `differential` | 70 min | 205M | 2,116 |

- Edge coverage had stopped growing in each target before its run ended.
- Together, the corpora cover every line of `verify.rs` except `verify_single_instruction`, which
  no target calls.
- In the last two rounds, the CPI ceiling pass checked at least 6.3M accepted programs, 3.8M invoke
  sites and 8.4M account records. 37,000 of those programs sit at exactly 64 worst-case CPIs.
