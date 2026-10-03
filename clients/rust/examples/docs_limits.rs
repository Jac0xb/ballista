//! Rust snippets from the guide pages on inputs, loops and limits that are not whole templates.
//! Each sits in a function, so `cargo test -p ballista-sdk`, which builds every example, compiles
//! it. The whole templates are in `docs_templates.rs` and `docs_runs.rs`.
//!
//! The pages include each function by its `#region` name.

#![allow(dead_code)]

fn main() {}

// #region encode-run-inputs
/// The run data for inputs `amount: u64`, `deadline: i64`, `enabled: bool` and `routeData: bytes`.
fn encode_run_inputs(route_data: &[u8]) -> Vec<u8> {
    use ballista_sdk::RunInputs;

    RunInputs::new()
        .u64(25_000)
        .i64(1_800_000_000)
        .bool(true)
        .bytes(route_data) // a u16 length, then the bytes
        .finish()
}
// #endregion encode-run-inputs
