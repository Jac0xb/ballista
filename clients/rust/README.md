# Rust client

`ballista-sdk` derives Ballista's template and registry entry addresses, builds the instructions
that upload and run a template, encodes run inputs, and decodes error codes, with current Solana
Rust types. Its `ProgramBuilder` writes the same bytecode as the
TypeScript compiler, and it shares the template parser and verifier with the on-chain program
through `ballista-common`.

## Install

The crate is not on crates.io yet. Install it from the repository:

```toml
[dependencies]
ballista-sdk = { git = "https://github.com/Jac0xb/ballista" }
# The SDK's instructions and addresses are solana-program 4.1.0 types; use the same version.
solana-program = "=4.1.0"
```

`cargo add ballista-sdk --git https://github.com/Jac0xb/ballista` writes the same line.

The [Rust SDK reference](https://jac0xb.github.io/ballista/reference/rust) covers the API.
