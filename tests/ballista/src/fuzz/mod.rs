//! Fuzzing and stateful testing of the on-chain program in Mollusk.
//!
//! - [`harness`] builds a Mollusk with Ballista, the Token and Memo programs and a small probe
//!   program, uploads generated templates, runs generated scenarios, and captures each run's
//!   result and inner instructions.
//! - [`invariants`] checks every outcome: the failure is a documented one (never an abort), the
//!   finalized template and registry headers are intact, Ballista's CPIs touch only declared
//!   accounts and declared programs, and — when the independent reference model
//!   ([`ballista_fuzz_gen::model`]) predicts the run concretely — the CPIs and return data match
//!   what the template should emit.
//! - [`executor`] is the main seeded fuzz loop; [`lifecycle`] runs random create/begin/write/
//!   finalize/cancel/run sequences; [`mutation_guards`] are deterministic tests that assert the
//!   exact error of each account check, so deleting the check fails them; [`registry_ordering`]
//!   probes the late-open design question the second critic raised.

mod critic_lost_write;
mod executor;
mod harness;
mod invariants;
mod lifecycle;
mod mutation_guards;
mod registry_ordering;

