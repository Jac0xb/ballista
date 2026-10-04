//! Generators and a reference model for fuzzing the Ballista executor and template lifecycle.
//!
//! The crate depends only on `ballista-common`. Callers inject PDA derivation and the rent
//! schedule, so it pins no Solana runtime crate and compiles both for the Mollusk suite (where it
//! is a dev-dependency of `tests/ballista`) and for the native libFuzzer target.
//!
//! - [`source`] turns a seed or libFuzzer's bytes into a stream of choices.
//! - [`template`] builds templates that verify by construction, exercising every opcode family,
//!   both loop kinds, registries (opened early or late), and CPIs to the System program, a probe
//!   program, the Token program and Ballista itself.
//! - [`scenario`] builds the accounts, run data and surrounding instructions for one run, with
//!   duplicates, aliasing and deliberate mutations.
//! - [`model`] is an independent interpreter of the wire format that predicts a run's CPIs and
//!   outputs, so a successful run can be checked for doing the right thing, not merely for not
//!   aborting.

pub mod model;
pub mod scenario;
pub mod source;
pub mod template;

pub use source::{ByteSource, Gen, SplitMix64};
