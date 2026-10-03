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
- **Negative mutation** (`support/src/negative.rs`) breaks one rule of an accepted program: a
  record above its declaration, an undeclared account or program, a loop maximum past 64 CPIs, a
  read before a write, a type mismatch, an `EMIT` tag, a writable registry entry, a guarded
  invoke before `RETURN_DATA`, a read past the minimum length, or the sysvar pin. `verify` must
  return that rule's exact error.

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

## Known findings

Each finding has an ignored test in `common/tests/fuzz_findings.rs`.

| Rule | Finding |
| --- | --- |
| `cpi.segment-literal-register`, `cpi.segment-register-fields` | `verify_cpi` checks invocation data segments with its own loop and skips the two unused-field checks that `verify_segment` makes. |
| `unreferenced.segment`, `unreferenced.cpi` | A data segment or CPI descriptor that nothing reaches is never checked. |

Two more findings there have no checker rule:

- `verify_cpi` numbers a bad segment from the descriptor's first, not the table's.
- `verify_single_instruction` panics on a view with more than 64 registers.

The four rules in the table are listed in `harness::KNOWN_FINDINGS`, so the targets print them once
and keep fuzzing.

- `BALLISTA_FUZZ_STRICT=1` makes them all fatal again, for example to check a fix.
- A comma-separated list of rule names makes only those fatal. This is how
  `fuzz/regressions/differential/` was made: one crash per rule, minimized with `cargo fuzz tmin`.

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
  verifier error the checker lacks is a rule neither side can catch. On the corpora of the first
  long run, the four known findings were the only disagreements.
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

The 14 mutants cover the CPI ceiling, declared accounts and groups, the CPI count,
read-before-write, read bounds, return data, `EMIT` tags, registry entries and the sysvar pin.
The stable tests catch every one, and so does `structured`, within 5 seconds of an empty corpus.
That includes the critic's `verify-cpi-privilege`, which the proptests in `common/tests` miss.
