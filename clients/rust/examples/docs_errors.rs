//! The Rust code on the Errors and events page, compiled so that it cannot go stale.
//!
//! ```bash
//! cargo run -p ballista-sdk --example docs_errors
//! ```

fn main() {
    decode();
    println!("decoded (7 << 16) | 6015 as RequirementFailed at program counter 7");
}

// #region decode
fn decode() {
    use ballista_sdk::decode_ballista_error;

    // A failed `require` at program counter 7: the kind in the low 16 bits, the context in the high 16.
    let decoded = decode_ballista_error((7 << 16) | 6015).unwrap();
    assert_eq!(decoded.name, "RequirementFailed");
    assert_eq!(decoded.context, 7);
}
// #endregion decode
