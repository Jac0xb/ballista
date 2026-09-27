# Protocol tests

Ballista's live-protocol templates, run in [LiteSVM](https://github.com/LiteSVM/litesvm) as real
signed transactions against the real mainnet programs. Programs and accounts come from a committed
one-slot snapshot of mainnet, so the suite never touches the network. Tests may write only wallet
balances, oracle prices and the clock; every other change goes through the protocols' own
instructions. Design: [real-protocol tests](../../docs/superpowers/specs/2026-09-26-real-protocol-tests-design.md).

This directory is its own Cargo workspace, excluded from the root one.

## Run

From the repository root:

```bash
git lfs pull                                                 # if snapshot/programs/*.so are pointer files
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
cargo test --manifest-path tests/protocols/Cargo.toml
```

## The snapshot

| File | Holds |
| --- | --- |
| `snapshot/manifest.json` | Slot, clock and test wallet; each program's and account's owner, lamports, size and sha256 |
| `snapshot/accounts.json` | Every account's data |
| `snapshot/routes.json` | Each Jupiter route's quote and instructions, by name |
| `snapshot/programs/*.so` | Program binaries, in Git LFS |

`scripts/snapshot/manifests/` says what to take: programs, accounts and the swaps to quote. To
refresh (Node 22 or later), from the repository root:

```bash
node scripts/snapshot/snapshot.mjs scripts/snapshot/manifests/milestone-1.json tests/protocols/snapshot
```

The run replaces `snapshot/` whole and prints what changed.

- `SOLANA_RPC_URL`: a mainnet RPC. Defaults to the public `https://api.mainnet.solana.com`.
- `JUPITER_API_KEY`: query `api.jup.ag` with this key. Defaults to the keyless `lite-api.jup.ag`.

The tool's own tests: `node --test 'scripts/snapshot/*.test.mjs'`.
