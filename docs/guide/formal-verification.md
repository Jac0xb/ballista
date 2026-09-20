# Formal verification

Ballista ships specifications for the Certora Solana Prover. The prover analyzes the compiled SBF
program symbolically and proves each rule for every input, rather than for the inputs a test
happens to try. Everything lives under `certora/` in its own Cargo workspace; see its README for
setup and commands.

## Why this program suits it

The runtime is a small typed bytecode VM with a static verifier, one bounded loop, no recursion,
and no unsafe code outside two syscall shims. The most important property is also the one tests
cannot fully establish: that whatever the verifier accepts at finalize, the executor runs without a
structural error. That is a per-instruction statement, and the prover checks it for every
instruction shape at once.

## What is proved

| Area | Rules |
| --- | --- |
| Typing preservation | For any register typing, consistent register values, and any instruction the verifier accepts, execution returns success or a value-dependent error, and the destination holds the recorded type |
| Arithmetic | Checked add, subtract, multiply, divide, min, and max match Rust's checked semantics for `u64`, `i64`, and `u128`; mixed and non-numeric operands are rejected |
| Comparisons and casts | Ordered comparisons match Rust and are numeric-only; casts succeed exactly when the value fits |
| Parser | Magic and version are checked first, short payloads report truncation, parsed sections exactly consume the payload, and verified programs respect every static limit |
| Account constraints | Signer, writable, executable, address, owner, minimum length, and account count are enforced exactly; header reads return the account's real fields |
| Lifecycle | Finalized templates reject chunk writes and cancellation, uploading templates never run, and a run never writes the template account |
| Error codes | Encoding round-trips; every code is runtime, verifier, or foreign, never two of them |

## How it fits the code

The program crate exposes its executor module behind a `spec-api` feature so rules can call the
pure functions directly. The feature changes visibility only, the module stays private in every
other build, and the release binary is byte-identical with or without it.

`cvlr-pinocchio` builds nondeterministic pinocchio account views by laying out the runtime's input
format with symbolic contents. It is the pinocchio counterpart of Certora's `cvlr-solana` and has
no Ballista-specific code.

## What is not proved

- Anything that depends on an invoked program. CPIs are modelled as opaque calls; the properties
  stop at the boundary where the callee's semantics begin.
- Syscalls such as hashing, PDA search, and sysvar reads are modelled by the prover, not verified.
- Whole-program execution across many instructions. The typing rule is the induction step; the
  induction itself is an argument, not a machine-checked proof.
- The pinocchio and Agave runtime code underneath, which remain the trusted base.

## Running a job

```bash
cd certora/ballista-specs
cargo certora-sbf --tools-version v1.53
certoraSolanaProver run.conf
```

A `CERTORAKEY` is required; the CI workflow runs the prover only when the secret is present and
otherwise just typechecks the specifications.
