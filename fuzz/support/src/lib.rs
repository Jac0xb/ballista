//! Shared code for the Ballista verifier fuzz targets in `fuzz/fuzz_targets`.
//!
//! - [`model`]: an owned payload model, decoded by byte offset from the wire-format docs.
//! - [`ceiling`]: the CPI guarantees only finalization enforces (the privilege ceiling, declared
//!   accounts, the loop-expanded CPI count), checked first and on their own.
//! - [`checker`]: a reference checker for what finalization guarantees, independent of `verify`.
//! - [`harness`]: the properties each target asserts.
//! - [`gen`] and [`mutate`]: structure-aware program generation and mutation.
//! - [`negative`]: one rule broken in an accepted program, which `verify` must then reject.
//! - [`structured`]: the `structured` target's input format.
//!
//! It has no libFuzzer dependency, so `cargo test` runs its tests on stable.

pub mod ceiling;
pub mod checker;
pub mod gen;
pub mod harness;
pub mod model;
pub mod mutate;
pub mod negative;
pub mod structured;
