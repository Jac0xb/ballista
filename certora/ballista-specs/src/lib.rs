//! Certora Solana Prover specifications for Ballista.
//!
//! Each `#[rule]` function is compiled into this crate's SBF binary together with the program
//! itself (built with `no-entrypoint`). The prover analyzes the binary and explores every rule for
//! all nondeterministic values. Rules never run on-chain.
//!
//! Organisation:
//!
//! - `rules::arithmetic`: checked arithmetic, comparisons, casts and multiply-divide.
//! - `rules::errors`: error codes round-trip and partition into runtime, verifier, and foreign.
//! - `rules::parser`: the parser rejects foreign payloads and never returns inconsistent views.
//! - `rules::accounts`: account constraints are enforced exactly, and account reads are typed.
//! - `rules::registry`: registry entries open only as the run names them, and fields need an open.
//! - `rules::returndata`: return data comes from the invoked program.
//! - `rules::ceiling`: a CPI asks only for privileges its accounts' declarations require.
//! - `rules::typing`: whatever the verifier accepts for one instruction, the executor runs without
//!   a structural error (`rules::oracle` splits the errors) and leaves the recorded types.
//! - `rules::lifecycle`: finalized templates are immutable and unfinalized ones never run.
//! - `rules::diagnostics`: what the prover's memory model follows, with expected results.
//! - `rules::symbolic` and `rules::util`: inputs built at the widths the program reads them.
//! - `mocks`: a stand-in for `sol_get_return_data`, which the prover does not model.
//!
//! Each conf in this crate's directory states what its rules are expected to do; see
//! `certora/README.md`.

pub mod mocks;
pub mod rules;
