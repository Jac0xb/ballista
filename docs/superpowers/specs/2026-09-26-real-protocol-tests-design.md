# Real-protocol tests

Status: proposed · 2026-09-26 · branch `claude/protocol-tests`, from `1d74c68` (`cu/integrated`
plus the protocol-example fixes)

## Goal

Run every live-protocol template as a signed transaction against the real deployed programs and
real mainnet account state, with no stand-ins. Today a template only has to compile, pass the
verifier and satisfy tests that read its document. After this change, a template that would fail
against Jupiter, Kamino, Orca, Drift, marginfi, Pyth or Jito fails CI instead of failing someone's
upload.

## Decisions

| Question | Decision |
| --- | --- |
| Harness | LiteSVM, loading a pinned snapshot of mainnet. Real transactions: signatures, fees, compute budget, lookup tables |
| What is committed | Program binaries (Git LFS) and account state. The suite never touches the network |
| Jupiter routes | Requested from Jupiter's API at snapshot time with `useSharedAccounts: false`. The snapshot captures the returned instruction, its lookup tables, and every account the route touches |
| What tests may write directly | Only (1) test wallets' SOL and token balances, (2) oracle price accounts, in their real layout, and (3) the clock. Every other state change goes through the protocols' own instructions |
| Scope now | The 12 live-protocol templates. Each runtime phase later adds real tests for its own features |
| CI | Every PR. LFS objects are cached so runs don't spend the LFS bandwidth quota |

## Layout

```text
scripts/snapshot/
  snapshot.mjs            fetch programs, accounts and Jupiter routes at one slot; write the snapshot
  manifests/*.json        per protocol: program IDs, account addresses or derivations, swaps to quote
tests/protocols/          its own Cargo workspace, excluded from the root like tests/ballista
  snapshot/
    manifest.json         slot, block time, program checksums, per-account metadata and data hash
    accounts.json         every account: owner, lamports, executable, data (base64)
    routes.json           Jupiter instructions, lookup tables and quotes, by name
    programs/*.so         Git LFS
  src/
    snapshot.rs           load and verify the snapshot into LiteSVM; set the clock to the snapshot time
    wallet.rs             funded test wallets and token accounts (write rule 1)
    oracle.rs             move a Pyth price update's price (write rule 2)
    template.rs           upload a template with Ballista's real instructions; build runs by name
    kamino.rs, marginfi.rs, drift.rs, orca.rs, jupiter.rs
                          instruction builders from each protocol's IDL, for setup
  tests/<template>.rs     one file per template: a passing run and its failure runs
```

- **The program under test.** Ballista is built from source (`cargo build-sbf`) and loaded at
  its declared ID.
- **Binding by name.** Templates are uploaded from `fixtures/protocol-examples.json`. That file
  gains each template's account, input and group order, which the TypeScript compiler already
  computes. Runs are then built by name rather than by hand-copied positions; hand-copied
  positions are how the Rust runner came to pass an extra account.

## Scenarios

"Setup" happens through the protocols' own instructions unless a write rule is named. Each
failure path asserts the template's own requirement label, so a test cannot pass because
something else failed.

| Template | Setup | Must pass | Must fail |
| --- | --- | --- | --- |
| `jupiterDepositExactOutput` | Kamino user metadata and obligation; refresh reserve and obligation | Obligation collateral rises by exactly the swap's output | Floor above the fill (`swapMetItsFloor`) |
| `jupiterOracleCheckedSwap` | None | The fill clears the Pyth floor | Oracle moved above market, rule 2 (`fillBeatTheOracle`) |
| `tokenSweepIntoSwap` | A wallet balance different from the quoted amount, rule 1 | The whole balance is sold, and the quote is rescaled and met | Balance at the floor (`worthSelling`) |
| `jitoProfitGuardedTip` | A circular route | A tip at or under the realised profit is paid | A tip above profit (`profitCoversTheTip`); also records how a loss (negative profit) fails |
| `pythFreshPriceGate` | None | Fresh and in band, so the route runs | Clock past `maximumAge`, rule 3 (`priceIsFresh`); a band that excludes the price |
| `orcaCompoundFees` | Open a position, add liquidity, swap through the pool to accrue fees, `update_fees_and_rewards` | Fees collected and compounded; liquidity rises | No fees: both calls skipped, and the run lands |
| `orcaHarvestManyPositions` | Several positions, with fees accruing in only some ranges | Only the rows with fees collect | None |
| `kaminoRepaySwapOutput` | An obligation with collateral and a borrow | Debt falls by the swap's output | None |
| `kaminoLiquidateWithProof` | Borrow near the limit, then move the oracle until unhealthy, rule 2 | The liquidator nets at least the bounty | Bounty above the payout |
| `marginfiWithdrawAllWithFloor` | A marginfi account and a deposit | The whole position is withdrawn, at or above the floor | Floor above the position |
| `driftRebalanceExact` | A marginfi deposit; a Drift user and user stats | Drift's spot balance rises by exactly marginfi's payout | None |
| `driftSettleWhenProfitable` | A Drift perp position, and PnL from a moved oracle, rule 2 | Settle, and the reduce-only withdrawal lands | Withdrawal above the deposit: reverts, and no borrow is opened |

A scenario that shows a template cannot work against the real program is a finding, not a test
to skip. The template gets fixed, with a test, or its limit gets documented. Either way it is
reported.

## Refresh

- `pnpm snapshot:protocols` refetches everything at a new slot. It needs `SOLANA_RPC_URL` (the
  public endpoint works) and Jupiter's API.
- It prints what changed: program checksums, account sizes, and routes.
- When a protocol has upgraded its account layout, the offsets test and these scenarios fail
  together, which is the signal to re-derive the layout.

## CI

A new job:
1. Check out with LFS, restoring `.git/lfs` from the Actions cache, keyed on the snapshot
   manifest's hash.
2. Build Ballista for SBF.
3. Run `cargo test --manifest-path tests/protocols/Cargo.toml`.

## Known hard parts

- **Transaction size.** Jupiter routes carry many accounts. Quotes set `maxAccounts` so that a
  route, plus the template's own accounts, fits one v0 transaction with the route's lookup
  tables.
- **Oracle freshness.** The clock starts at the snapshot's block time, so every snapshotted
  oracle is fresh for Kamino, Drift, marginfi and Pyth.
- **Drift PnL.** A perp position has to be opened and filled through Drift's own instructions
  before a moved oracle can give it PnL.
- **Orca fees.** Fees accrue only from real swaps that trade inside the position's range.
- **Git LFS** is not installed on this machine. Installing it changes global git config, so the
  user is asked before it is set up.

## Out of scope

- The cookbook recipes. Their protocol calls are stood in by System transfers, and they already
  run under Mollusk.
- Devnet.
- The new runtime features; each phase adds its own.
