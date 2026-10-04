# Formal verification

What checks Ballista's program beyond its unit tests, what they have shown, and what none of them
covers. None of this replaces an audit, and Ballista hasn't had one: see
[Audit status](/guide/security#audit-status).

## The checks

- **Certora Solana Prover.** Rules about the compiled program, proved for every input the
  prover's model allows. [`certora/README.md`](https://github.com/Jac0xb/ballista/blob/main/certora/README.md)
- **Kani.** Bounded proofs on the program's Rust source: each harness covers every input up to a
  stated size. [`kani/README.md`](https://github.com/Jac0xb/ballista/blob/main/kani/README.md)
- **Four fuzzers:**
  - the **verifier** fuzzer runs random and mutated templates through the verifier and an
    independent reference checker, and compares the two
    ([`fuzz/README.md`](https://github.com/Jac0xb/ballista/blob/main/fuzz/README.md));
  - the **executor** fuzzer runs generated templates on Mollusk and compares their CPIs and return
    data with an independent model of the interpreter;
  - the **lifecycle** fuzzer drives random uploads, writes, cancels, finalizes and runs against a
    state model;
  - the **compiler** fuzzer compiles generated TypeScript templates, checks register reuse against
    its own operand table, and runs the variants on Mollusk.
- **Mutation testing.** Deletes one check at a time and confirms that some test fails. A check
  whose deletion every test survives is a check nothing tests.
- **Critics.** Four separate reviews attacked the threat model, the stated invariants, the test
  method and the proofs themselves. Most of what this page admits came from them.

## What's proved

At commit `cb2fb2d`, Certora proved 19 rules. Proved means the rule verified, its sanity check
passed, every reachability rule passed, and a twin with a deliberate error failed. They cover:

- **Parsing:** a template's sections tile its payload (payloads of up to 96 bytes).
- **Arithmetic:** `u128` subtract, multiply, divide, min and max, and the `u128` to `i64` cast.
- **Registries:** an entry opens only if it is writable, of its declared size, and bound to its
  template and key; fields need an open entry; a second open of an entry fails.
- **Return data:** it comes from the program just invoked, and is empty without an invoke.
- **Run-start account checks:** signers, pinned addresses and owners, minimum data length, the
  account count, and account header reads.

**These proofs predate the current verifier**, which now refuses a registry open after an invoke
and non-canonical encodings, and adds the account-group opcodes. Each needs a re-run before it
counts for today's code. Another 15 rules verified, but have no twin or reachability rule, so they
don't count as proved.

Kani's harnesses state bounded proofs of arithmetic, comparisons, casts, shifts and bitwise
operations, of section parsing up to the 10,240-byte limit, and of per-opcode agreement between
the verifier and the executor for opcodes that touch only registers. Its README gives each
harness's bound and its result at a named commit; one that timed out proves nothing.

## What's sampled

- **Verifier:** about 650 million fuzz executions against the reference checker, including the CPI
  privilege ceiling. Each of 21 verifier rules, deleted alone, makes a test fail.
- **Executor:** 200,000 generated templates on Mollusk, about 14,000 of them matched against the
  model byte for byte, and 57 million native runs without a crash.
- **Typing:** an enumeration of single instructions over 55 opcodes, 564,756,480 verifier calls,
  in which no verified instruction failed for a structural reason.
- **Compiler:** 20,000 seeds and 40,000 mutants in TypeScript, and about 16,000 Mollusk runs
  comparing compiles.

The account-group opcodes came after most of these runs, and are covered only in part.

## Known blind spots

- **Aliasing.** In both Certora's and Kani's models, every account is separate memory, so no rule
  sees one account passed in two slots. The program's borrow check handles that case, and Mollusk
  tests exercise it for registry entries; no proof does.
- **Opaque calls.** What a called program does, SHA-256, the curve check and CPI execution are
  stubbed or left out.
- **The binary gap.** The prover checks a build with a different toolchain version and crate type,
  and with test hooks compiled in. Kani checks the Rust source. Neither checks the deployed
  binary.
- **Prover imprecision.** Bitwise operations on two unknown values can give spurious results, and
  an overflow branch counts only if a reachability rule reaches it.
- **Bounds.** Kani's multiply and divide proofs use narrowed operands, and its loops run at most
  two passes.
- **What templates do.** Nothing proves a whole run end to end, or that a template does what its
  signers expect. See [Trust model](/guide/trust-model).

## Run them

```bash
# Certora, with a key from Certora
cd certora/ballista-specs
cargo certora-sbf --tools-version v1.53
certoraSolanaProver run.conf

# Kani 0.67
cd kani && cargo kani -p ballista-kani -Z stubbing -j

# Verifier fuzzer: a target and seconds, then the stable checks
fuzz/scripts/run.sh differential 900
cargo test --manifest-path fuzz/Cargo.toml -p ballista-fuzz-support --release

# Executor and lifecycle fuzzers (build the probe first)
cargo build-sbf --manifest-path fuzz-executor/probe/Cargo.toml
cargo test --manifest-path tests/ballista/Cargo.toml fuzz_executor_differential
cargo test --manifest-path tests/ballista/Cargo.toml fuzz_lifecycle_sequences

# Compiler fuzzer
pnpm --dir clients/js exec vitest run src/compiler-fuzz.test.ts

# Verifier mutants
python3 fuzz/scripts/mutants.py 120
```

CI runs the Certora spec tests, the SBF build check and the typing enumeration on pull requests,
and the fuzzers and mutants nightly. It runs the prover only when the repository has a
`CERTORAKEY` secret, which it doesn't yet.
