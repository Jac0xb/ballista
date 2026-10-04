//! Rust snippets from the guide pages on inputs, loops and limits that are not whole templates.
//! Each sits in a function, so `cargo test -p ballista-sdk`, which builds every example, compiles
//! it. The whole templates are in `docs_templates.rs` and `docs_runs.rs`.
//!
//! `encode_run_inputs` is a twin of the TypeScript encoding beside it on the expressions page,
//! `clients/js/examples/docs/named-inputs.ts`: `tests/docs_examples.rs` holds the two to the same
//! bytes.
//!
//! The pages include each function by its `#region` name.

#![allow(dead_code)]

#[path = "docs_templates.rs"]
mod templates;

fn main() {}

// #region encode-run-inputs
/// The run data for inputs `amount: u64`, `deadline: i64`, `enabled: bool` and `routeData: bytes`.
pub fn encode_run_inputs(route_data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let bytes = templates::named_inputs()
        .compile()?
        .run_inputs()
        .input("amount", 25_000u64)
        .input("deadline", 1_800_000_000i64)
        .input("enabled", true)
        .input("routeData", route_data) // a u16 length, then the bytes
        .encode_inputs()?;
    Ok(bytes)
}
// #endregion encode-run-inputs
