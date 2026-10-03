# Formal verification with the Certora Solana Prover

**Prover jobs ran at cb2fb2d on 2026-10-03. "Prover results at cb2fb2d" lists the 19 rules they
proved, and supersedes the statuses below where they differ.** CI runs the prover only when the
repository has a `CERTORAKEY` secret, and then only `run.conf`. Every other status is an expectation
until a prover job at a named commit confirms it.

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

## Prover results at cb2fb2d

certora-cli 8.19.2, 2026-10-03. **Proved** means the rule verified, its sanity check passed, every
reachability rule it has passed, and its twin failed.

| Conf | Job | Rules | Result |
| --- | --- | --- | --- |
| `run.conf` | [48efedc3](https://prover.certora.com/output/5644982/48efedc37e174aef8dfa7a46c9e70db8) | 17 | All verified |
| `run-candidates.conf` | [88f0a104](https://prover.certora.com/output/5644982/88f0a104b1624caf821dc961242d6bec) | 29 | 27 verified, 2 violated |
| `run-candidates-memcmp.conf` | [7225c27f](https://prover.certora.com/output/5644982/7225c27f21dd44c48e719fe3e14ac65f) | 2 | All verified |
| `run-twins.conf` | [705ecbe0](https://prover.certora.com/output/5644982/705ecbe0edc7474db6773d7005171d35) | 13 | All violated, as required |
| `run-twins-memcmp.conf` | [d142c280](https://prover.certora.com/output/5644982/d142c280baa7435593750520e6b46d20) | 1 | Violated, as required |
| `run-blocked.conf` | [8da461a1](https://prover.certora.com/output/5644982/8da461a1c1194ba4bec337139d7c5cba) | 50 | 22 verified, 15 violated, 13 no result |

Every verified rule with asserts also passed its sanity check. The blocked conf's 15 violations
are 9 twins and 4 diagnostics, as expected, and the 2 triaged below. Its 13 rules without a result
(`mul_div`, typing and lifecycle) were still running when the saved log ended at 38 minutes.

**Proved (19):**

- `run.conf`: sections tile the payload. The other 14 verified but have no twin or reachability
  rules.
- Candidates: `u128` sub, mul, div, min and max; `u128` to `i64`; a registry entry opens only if
  writable and of the declared size; fields need an open entry; return data comes from the
  invoked program, and is empty without an invoke; an open binds the template and key (under
  `-solanaOptimisticMemcmp`).
- Blocked conf: signers, pinned address and owner, minimum data length, account count, account
  header reads, and an open of an open entry fails. The suspected stack-copy blocker did not bite
  them.

`u128` add verified but is not proved: its overflow branch is unreachable in the prover's model.

### The four violations

Traces from triage jobs
[aac2bacd](https://prover.certora.com/output/5644982/aac2bacd98254d8bae289bcc83067eef) (ceiling,
writable) and [a83bc7fc](https://prover.certora.com/output/5644982/a83bc7fc09084b30ac2d1ab974763c62)
(the other two). None is a program bug.

| Rule | Cause | Evidence |
| --- | --- | --- |
| Writable and executable constraints | Prover: an AND of two unknowns | The program refused correctly, at index 0. LLVM compiled `!satisfied && index == 0` to `~satisfied & (index == 0)`, and the trace says `Imprecision detected: BWAnd(18446744073709551615, 1) = 1, but is 0`. |
| CPI privilege ceiling | Prover: an OR in the rule's own return value | The verifier refused (tag 0xf, `InvalidCpi`: descriptor `reserved1[0]` was 1), and the rule logged `accepted: 0`. `Invoke` came back packed in one register, built with ORs, and the solver set its `accepted` byte (`Imprecision detected: BWOr(...)`). No error copy was lost, and the verifier accepted nothing it should refuse. |
| `u128` add reaches overflow | Model: no 64-bit wraparound | Each limb add is a 256-bit `Add` with no mod 2^64, so LLVM's carry tests (`sum >= operand`) always hold. The prover wraps only the adds it matches as `checked_add` patterns within one block. No rule input reaches the branch. |
| Stack word copy keeps both halves | Model: a copied word is never rebuilt | The front end turned the copy into a `memcpy`, which moved two cells (7 and 6012). The eight-byte load read 7, and the presolver folded the assert to `false`. |

Fixes in this commit:

- The writable rule asserts its two facts separately.
- `Invoke` is whole words returned through memory.
- The ceiling rule checks each privilege against a constant bit.
- `the_counterexample_descriptor_is_refused` rebuilds the ceiling trace's input on the host.
- `scan-stack-copies.py` no longer exempts two four-byte halves.

### What the results change

- A copied word keeps only its first store. The one-byte tag at offset 0 survives
  (`rule_stack_word_copy_keeps_a_short_tag` verified); everything at other offsets is lost, two
  four-byte halves included. So `#[repr(C, u32)]` on `RunError` would not have helped.
- Outside matched `checked_add` patterns, an overflow branch is vacuous unless a reachability rule
  for it passes. `run.conf`'s `u64` and `i64` arithmetic rules have none.
- Bitwise operations on two unknowns are imprecise. They produce spurious counterexamples, and
  could produce spurious witnesses: check a passing satisfy rule's trace for "Imprecision detected".

### Run next

- The two fixed rules, with their reachability rules and twins: the eight `rule_cpi_requests_*`
  and `rule_writable_and_executable_*` rules in `run-blocked.conf`.
- `rule_u128_add_reaches_overflow` with `-solanaTACSoundSignedMath true` added to the conf's
  `prover_args`. The flag masks every 64-bit operation, and is experimental.
- Overflow reachability rules for `run.conf`'s `u64` and `i64` arithmetic.
- The blocked conf's 13 rules without a result, saving the job's report URL and key.

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
