//! Certora Solana Prover specifications for Ballista.
//!
//! Each `#[rule]` function is compiled into this crate's SBF binary together with the program
//! itself (built with `no-entrypoint`). The prover analyzes the binary and explores every rule for
//! all nondeterministic values. Rules never run on-chain.
//!
//! Organisation:
//!
//! - `rules::arithmetic`: checked arithmetic, comparisons, and casts match their specification.
//! - `rules::errors`: error codes round-trip and partition into runtime, verifier, and foreign.
//! - `rules::parser`: the parser rejects foreign payloads and never returns inconsistent views.
//! - `rules::typing`: whatever the verifier accepts for one instruction, the executor runs without
//!   a structural error and leaves the destination register with the recorded type.
//! - `rules::accounts`: account constraints are enforced exactly, and account reads are typed.
//! - `rules::lifecycle`: finalized templates are immutable and unfinalized ones never run.

pub mod rules;
