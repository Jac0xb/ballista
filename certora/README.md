# Formal verification with the Certora Solana Prover

**Nothing here has been proved against the current code.** CI runs the prover only when the
repository has a `CERTORAKEY` secret, and no prover job has run on this code: the last one ran on
2026-09-20, on a branch rewritten since. Every status below is an expectation until a prover job at
a named commit confirms it.

This directory is its own Cargo workspace, so nothing here touches the release build, its lock
file, or its dependency policy.

| Crate | Purpose |
| --- | --- |
| `cvlr-pinocchio` | Nondeterministic pinocchio accounts, laid out as the runtime lays them out. Program-agnostic. |
| `ballista-specs` | The rules. Builds to an SBF binary holding the program (`no-entrypoint`, `spec-api`) and the rules. |

## Run

```bash
cd certora
cargo check --features rt                         # typecheck the rules on the host
cargo test --features rt                          # host tests: layouts, the error split, conf coverage
cargo test --release -p ballista-specs --features rt --test typing_enumeration -- --ignored
                                                  # the typing property, enumerated on the host
./build-sbf.sh                                    # SBF build; fails on a frame over 4 KiB

cd ballista-specs                                 # needs CERTORAKEY
certoraSolanaProver run.conf                      # expected to prove
certoraSolanaProver run-candidates.conf           # expected to prove, never run
certoraSolanaProver run-candidates-memcmp.conf
certoraSolanaProver run-twins.conf                # every rule must FAIL
certoraSolanaProver run-twins-memcmp.conf
certoraSolanaProver run-blocked.conf              # known or suspected blockers
```

Install with `pipx install certora-cli` and `cargo install cargo-certora-sbf`. Certora publishes
platform tools up to v1.53; the program builds with v1.54. Both ship rustc 1.89.

## How the rules are kept honest

- **Twins.** Each rule edited since the last prover run has a twin that states the property with
  one deliberate error and must fail. A twin that passes means its rule passes for some reason
  other than the program.
- **Reachability.** Those rules also have a `cvlr_satisfy!` rule for each branch that asserts.
  Satisfy rules stay separate: in a rule that contains one, Certora treats every assert as an
  assumption.
- **Vacuity.** Every conf sets `rule_sanity: basic`.
- **Coverage.** A host test checks that each of the 112 rules sits in exactly one conf.

## Status

### Expected to prove, unconfirmed (`run.conf`, 15 rules)

- **Arithmetic, comparisons and casts:** `u64` and `i64` checked arithmetic, signed division's
  errors, mixed operands, comparisons, ordering, five of the six casts, and `u64` remainder, shifts
  and bitwise operations.
- **Error codes:** they round-trip, and runtime and verifier codes stay in their ranges.
- **Parser:** magic and version come first; short payloads are truncated; sections tile the
  payload. This covers payloads up to 96 bytes; Kani (track FV5) checked the full 10,240 in 159 s.

At risk: six of these (`u64` and `i64` arithmetic, signed division, mixed operands, casts, `u64`
integer operations) read an error's kind back from a stack copy the prover may not follow ("Error
kinds in stack copies" below). The 2026-09-20 binaries had no such copy in the arithmetic rules;
today's do.

The section rule was wrong and vacuous until this branch: it counted only the fixed inputs, and a
`cvlr_satisfy!(true)` turned its asserts into assumptions. Its twin is the old statement.

### Expected to prove, never run (`run-candidates*.conf`, 15 rules)

- **`u128` arithmetic:** one rule per opcode, where one rule over all six blocked. Add, sub, mul
  and div are at risk like the `u64` rule.
- **Casts:** `u128` to `i64`, the sixth pair. At risk the same way.
- **Registry:**
  - an entry opens only if it is writable and of the declared size;
  - field reads and writes need an open entry;
  - an entry opens only if its header names the template and the key. This one needs
    `-solanaOptimisticMemcmp`, and says nothing about the header's first word.
- **Return data:** a read returns the invoked program's data or nothing, and nothing without an
  invoke.
- **Memory controls:** two diagnostics expected to prove. The `run-candidates.conf` job at cb2fb2d
  found one, `rule_stack_word_copy_keeps_both_halves`, violated (see "Error kinds in stack copies").

### Blocked (`run-blocked.conf`, 20 rules)

| Rules | Reason |
| --- | --- |
| Account constraints (5) | Suspected: `validate_account`'s error kinds travel in stack copies |
| Account header reads | Suspected: the executor's register write copies one-byte-tagged values by words |
| Registry: an open of an open entry fails | Suspected: the outcomes differ only in the result, copied the same way |
| `mul_div` | Suspected: its result is copied the same way |
| Privilege ceiling, finalization's half | Suspected: `verify_cpi`'s refusals are copied the same way, so one can read as acceptance |
| Typing preservation (2) | Suspected: the same, for registers and verifier results; the host enumeration passes |
| Lifecycle (4) | Byte-stored payload and instruction data; the run rule also hits the model limits below |
| Diagnostics (5) | Expected to fail, or probes: they isolate the limits |

"Suspected" means the rule reads a value through a copy the scanner flags, and a prover job has
to settle it. A rule in this conf that passes counts only if its twin fails and its reachability
rules pass.

## Why the blocked rules are blocked

Everything below was checked against the compiled code (`llvm-objdump`) and the prover's source
(Certora/CertoraProver, release of 2026-09-07). Only a prover job confirms it.

- **Heap cells have a width.** The prover keys each heap cell by its address and the width it was
  written at. It rebuilds nothing when a load reads at another width. LLVM merges adjacent byte
  reads into wide loads on SBF, so constants written a byte at a time and then parsed reach the
  parser as unrelated values. The rules now build inputs over havoced memory instead
  (`rules::symbolic`), constrained through the program's own accessors.
- **Error kinds in stack copies.** The stack keeps values by offset. Executor errors are built as
  a two-byte tag, two bytes never written, and a four-byte kind, then copied as one word. The copy
  carries at most the tag, so a rule that reads the kind afterwards sees an unknown value. The
  prover was thought to rebuild a word from two four-byte halves, but the control for that,
  `rule_stack_word_copy_keeps_both_halves`, came back violated at cb2fb2d
  ([job](https://prover.certora.com/output/5644982/88f0a104b1624caf821dc961242d6bec)): no shape of
  narrower stores is known to survive the copy.
  - `scan-stack-copies.py` lists these copies in every rule.
  - The `rule_stack_word_copy_*` diagnostics test the mechanism directly.
  - One-byte `RuntimeValue` tags copied the same way may survive: the ordering rule had them when
    it proved on 2026-09-20.
  - The rules' own types are copied the same way. Keep them in whole words: `u64` tags and fields,
    as `rules::accounts::Validation` has.
- **Some calls have no model.**
  - `sol_get_return_data` writes nothing, not even `r0`. A link-time `--wrap` sends the program's
    calls to a stand-in in `src/mocks.rs`.
  - The bump search loops over `sol_sha256` and `sol_curve_validate_point`, which have no model;
    `derive_pda` and `get_template_address` are external.
  - `__umodti3` has no summary.
  - A global copied by `memcpy` is never initialized.

## Model limits

- **External calls.** CPIs, entry creation, PDA derivation, `invoke_cpi` and whole-program `verify`
  are external calls with nondeterministic results that touch no memory.
- **No aliasing.** Each account slot is its own allocation, so the lifecycle run rule cannot see
  `bounded_invoke`'s borrow check, and would pass a mutant without it.
- **Test-only hook.** `Scratch::set_last_invoked_for_spec`, compiled only with `spec-api`, stands in
  for a successful invoke in the return-data rules.

## The binary the prover sees

The prover analyzes the spec build, not the deployed one. The spec build differs in:

- platform tools: v1.53 against v1.54;
- crate type: an `rlib` against the program's `cdylib`;
- features: `spec-api` and `no-entrypoint`.

Compiled sizes differ too: `ProgramView::parse` is 290 SBF instructions against 118 deployed,
`read_return_data` 257 against 222, and `registry::open` 84 against 71. The dependencies match;
`solana-account-view` is pinned to the program's 2.0.0.

`build-sbf.sh` fails on any frame over SBPF v0's 4 KiB, which `cargo certora-sbf` reports but exits
0 on. The confs set `-solanaStackSize 4096` to match.

## What would unblock the rest

- **Program: `#[repr(C, u32)]` on `RunError`, dropped.** It would turn the tag and the kind into
  two four-byte halves, for about +0.74% compute. The prover does not rebuild a value from two
  halves in that shape either (the violated control above), so it would not unblock the rules.
- **Prover:** let a stack word rebuild from narrower stores, halves or not, keeping the written
  bytes exact.
