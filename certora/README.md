# Formal verification with the Certora Solana Prover

This directory is its own Cargo workspace so nothing here touches the release build, its lock
file, or its dependency policy.

| Crate | Purpose |
| --- | --- |
| `cvlr-pinocchio` | Nondeterministic `AccountView` construction for pinocchio programs; the counterpart of `cvlr-solana` for `AccountInfo`. Program-agnostic. |
| `ballista-specs` | The `#[rule]` specifications. Builds to an SBF binary that contains the program (with `no-entrypoint`) plus the rules. |

The program crate itself changes in one way: the `spec-api` feature makes the executor module
public so rules can call its pure functions. The module stays private in every other build, and
`cargo build-sbf` never enables the feature.

## Install

```bash
pipx install certora-cli                 # certoraSolanaProver
cargo install cargo-certora-sbf          # builds SBF with Certora's platform tools
export CERTORAKEY=...                    # from https://www.certora.com
```

## Build and run

```bash
cd certora
cargo check --features rt                # host typecheck of the adapter and rules
cargo test -p cvlr-pinocchio --features rt

cd ballista-specs
cargo certora-sbf --tools-version v1.53  # SBF binary for the prover
certoraSolanaProver run.conf             # every rule
certoraSolanaProver run.conf --rule rule_verified_pure_instructions_preserve_register_typing
```

Results appear at `https://prover.certora.com/output/<job>/<key>`. Platform tools v1.53 ship the
Rust 1.89 compiler that pinocchio 0.11 requires; older versions will not build the program.

## A build-time check in its own right

`cargo certora-sbf` compiles for SBPF version 0 and reports any function whose stack frame exceeds
4 KiB, which the regular `cargo build-sbf` does not. That report found two real frame overflows in
the program on first use. Run it after changes to the executor even when no prover job follows.

## What the rules state

- **Typing preservation.** For any register typing, any register values consistent with it, and
  any instruction the verifier accepts against it, the executor returns success or a
  value-dependent error and leaves the destination holding the recorded type. This is the
  per-instruction step of the argument that finalize-time verification is sound.
- **Arithmetic, comparisons, casts.** Match Rust's checked semantics for every input and reject
  mixed or non-numeric operands.
- **Parser.** Checks magic and version first, reports truncation rather than misparsing, and
  returns sections that exactly consume the payload.
- **Account constraints.** Signer, writable, executable, address, owner, minimum length, and count
  are enforced exactly, and header reads return the account's real fields.
- **Lifecycle.** Finalized templates reject chunk writes and cancellation, uploading templates
  never run, and a run never writes the template account.
- **Error codes.** Encoding round-trips and every code decodes into exactly one of runtime,
  verifier, or foreign.

Syscalls (`sol_sha256`, PDA search, sysvars, CPI) are modelled by the prover; the summaries file
types the PDA search result. Rules that go through a CPI are deliberately absent: the value of a
CPI depends on the invoked program, which is outside the property.
