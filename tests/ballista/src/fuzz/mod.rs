//! Fuzzing and stateful testing of the on-chain program in Mollusk.
//!
//! - [`harness`] builds a Mollusk with Ballista, the Token and Memo programs and a small probe
//!   program (loaded at two addresses), uploads generated templates, runs generated scenarios, and
//!   captures each run's result, inner instructions and logs, including the signer and writable
//!   flags each probe call received.
//! - [`invariants`] checks every outcome, and every finding is hard: no abort; no account-rule
//!   error (`PrivilegeEscalation`, `ReadonlyDataModified`, ...) in Ballista's own frame; no
//!   structural error from a verified template (P40); the finalized template and registry headers
//!   intact; Ballista's CPIs touching only declared accounts and programs; and, when the
//!   independent reference model ([`ballista_fuzz_gen::model`]) predicts the run concretely, every
//!   CPI (program, accounts, data, flags), EMIT line and return data exactly as predicted, or the
//!   failure it predicted.
//! - [`executor`] is the main seeded fuzz loop, with floors on how much the model compared and a
//!   limits mode (`FV_LIMITS=1`); [`lifecycle`] runs random create/begin/write/finalize/cancel/run
//!   sequences; [`mutation_guards`] are deterministic tests that assert the exact error of each
//!   account check, so deleting the check fails them; [`registry_ordering`] probes the late-open
//!   design question the second critic raised.

mod critic_lost_write;
mod executor;
mod harness;
mod invariants;
mod lifecycle;
mod mutation_guards;
mod registry_ordering;

