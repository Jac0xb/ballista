# TODO

## After publishing the packages

Once the program is deployed and `@jac0xb/ballista` (npm) and `ballista-sdk` (crates.io) are
published, replace every build-from-the-repository instruction with the published packages.

- [ ] `docs/guide/getting-started.md`, Install:
  - Drop "Neither the program nor the SDKs are published yet, so you build them from the
    repository" and the clone / `cargo build-sbf` steps.
  - Start the local validator with the deployed program (`--clone-upgradeable-program` or
    `--bpf-program` from a released `.so`) instead of a local build.
  - TypeScript: `npm install @jac0xb/ballista @solana/kit@8` instead of `pnpm build:sdk`,
    `npm pack` and the `.tgz` install.
  - Rust: `cargo add ballista-sdk` instead of `--git https://github.com/Jac0xb/ballista`.
  - "Run the shipped examples" still needs the clone; say so there.
- [ ] `clients/rust/examples/getting-started/Cargo.toml`: switch `ballista-sdk` from
  `path = "../.."` to the published version, so the crate's dependencies match the install line
  exactly.
- [ ] `docs/reference/rust.md`: drop "The crate is not on crates.io yet", use a versioned
  `ballista-sdk = "x.y"` dependency and `cargo add ballista-sdk`.
- [ ] `clients/rust/README.md`: same change as `rust.md`.
- [ ] `clients/js/README.md` and the root `README.md`: add the `npm install` line.
- [ ] Search again before closing this out:
  `grep -rniE 'not published|not on crates|npm pack|\.tgz|--git https://github.com/Jac0xb/ballista' docs clients README.md`

## SDK

- [ ] Ship the SPL Token account layout in both SDKs instead of redefining it per example.
  `TOKEN_ACCOUNT_AMOUNT_OFFSET = 64` and `TOKEN_ACCOUNT_LENGTH = 165` (plus the mint at 0 and the
  owner at 32) are copied into about 25 examples under `clients/js/examples/` and live in
  `clients/js/examples/protocols/shared.ts`.
  - Solana's packages only cover part of it: `getTokenSize()` in `@solana-program/token` and
    `Account::LEN` in Rust's SPL Token crates give the 165, but neither exports the field offsets.
  - So export the constants from `@jac0xb/ballista` and `ballista-sdk`. Better still, add an
    expression helper such as `token.amount(account)` that reads the balance and implies the
    owner pin and minimum length, so templates never spell out offsets.
  - Then update the examples and the docs pages that include them.

## From the docs site

- [ ] Everything in inline code styling should be orange (docs/guide/expressions.md § Prices and decimals): "expression.multiplyDivide(a, b, divisor, rounding) computes a × b ÷ divisor. The product a × b is an intermediate value, used only on the way to the result, and it is held at twice the inputs' width …" <!-- review:65774845-fdd1-4dd8-ab76-4b2aba33b3d7 -->
- [ ] Probably should condense this section (docs/guide/expressions.md § Inputs and expressions): "Inputs and expressions" <!-- review:bc76e593-094b-4267-b5ca-d45bfdc99947 -->
- [ ] Lets reconvene if this is a bad design (docs/guide/registries.md § Remember state between runs): "The first run to open an entry creates it, and the payer the template names pays its rent, never returned: 1,097,280 lamports for 16 bytes of fields. Only runs of its template can change an entry, an…" <!-- review:ab0d0ae8-c8e9-40d6-b6ac-23d9bb07fa5d -->
