# Real-protocol tests, milestone 2: the Orca templates against the real Whirlpool program

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to carry out this plan task by task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run the two Orca templates as real signed transactions against the real Whirlpool
program in LiteSVM, on a committed mainnet snapshot of two SOL/USDC pools:
- `orcaCompoundFees`
- `orcaHarvestManyPositions`

Fix what the program shows is wrong with them, findings M1 to M6 of the research. Each fix lands
with a real test that failed before it.

**Architecture:** see the spec, `docs/superpowers/specs/2026-09-26-real-protocol-tests-design.md`,
and milestone 1's plan, `docs/superpowers/plans/2026-09-26-protocol-tests-harness.md`. This
milestone adds:
- its own snapshot, `tests/protocols/snapshot-orca/`, written by milestone 1's tool from a new
  manifest, `scripts/snapshot/manifests/orca.json`;
- `tests/protocols/src/orca.rs`, which does setup through Orca's own instructions and Orca's own
  math: `orca_whirlpools_client` 8.0.0 and `orca_whirlpools_core` 2.1.1. That counts as real;
- one test file per template;
- a static check of every Whirlpool call against Orca's client, and a file pinning the Orca
  behaviors the fixes rely on.

**Validated while planning.** Every code block in this plan ran once, against the real Whirlpool
program, before the plan was written.
- The manifest went through the real snapshot tool (slot 451,125,511), and the harness loaded
  the result.
- All 18 tests passed against the fixed templates.
- The tests also ran against each intermediate template the TDD steps pass through, so each
  step's "expect" is the failure that was observed.

The scratch copy is `m2val/`, in the scratchpad directory given under **Research** below:
- `orca.json` and `snapshot-orca/`: the manifest and a snapshot made from it;
- `js/`: the templates, compiled by the SDK, including every intermediate stage;
- `rs/`: the harness plus `src/orca.rs` and the tests;
- `rs/tests/orca_validation.rs`: the raw scenario runs and measurements.

The numbers quoted below come from those runs. Task 10 re-measures them.

**Research this plan depends on:** all paths are under
`/private/tmp/claude-501/-Users-jacob-Documents-ballista/06470e69-8486-4adc-b478-7e3c20bf99aa/scratchpad/`.
- `protocol-research/orca-pyth-jito.md`:
  - §1.2–1.3: both pools' addresses;
  - §2: instruction layouts and Whirlpool error codes;
  - §3.2: the findings M1–M6;
  - §4: the setup recipe (clock, wallets, crates).
- `protocol-research/cratecheck/orca-live/src/main.rs` and `protocol-research/orca-live-output.txt`:
  the research harness that `orca.rs` grew from.

**Base:** branch `claude/protocol-tests` once milestone 1's harness is committed on it (Task 1
checks). Work in a new worktree, `.claude/worktrees/protocol-orca`, on branch
`claude/protocol-orca`, and merge back at the end (Task 10). Build Ballista with
`cargo build-sbf --manifest-path programs/ballista/Cargo.toml`.

**Conventions:**
- Commit after each task. End each message with a blank line and the `Co-Authored-By` line your
  harness gives you.
- Never use `git stash`. To set work aside, make a WIP commit.
- Never edit `package.json` or `pnpm-lock.yaml`; another session owns them. `pnpm install
  --frozen-lockfile` and `pnpm fixtures` are fine: they change neither.
- Never edit `docs/`; another session owns it. Task 10's `FINDINGS.md` is what that session reads.
- Every template change is followed by `pnpm fixtures` before a Rust run: the LiteSVM tests read
  the payloads from `fixtures/protocol-examples.json`.
- Whirlpool's error codes (6000 and up, Anchor's) collide with Ballista's. Tests identify the
  failing program with `Failure.program`, never by the code alone (M6).
- Numbers in tests are exact where Orca's own math predicts them (`collect_fees_quote`, the
  liquidity quotes). Compute units and sizes are printed, never asserted.

## Decisions made while planning

| Question | Decision |
| --- | --- |
| Which pools | The compounder uses `Czfq…` (SOL/USDC, tick spacing 4), the liquid pool. The harvest uses `HJPj…` (tick spacing 64): it is thin, so its rows share one tick array, which keeps the transaction small, and test positions earn most of each fee |
| Tick arrays | Listed in the manifest, seven per pool: the current one and three on each side. `tests/orca_snapshot.rs` fails, naming the missing array, when a refresh finds a price outside them. The snapshot tool gains no derivation logic |
| Programs | Whirlpool, Token, Token-2022, Associated Token and Memo all come from mainnet, so none is LiteSVM's bundled copy. The Oracle PDAs stay absent: neither pool has adaptive fees |
| The harness | Unchanged apart from `pub mod orca;`. `Snapshot::load(dir)` already loads any directory, and `orca::SNAPSHOT_DIR` names this one |
| Reinvestment (M4) | `increase_liquidity_by_token_amounts_v2`, run only when both fees are above the floor. The input `liquidityAmount` gives way to `minSqrtPrice` and `maxSqrtPrice` (u128). The template gains the accounts `memoProgram`, `tokenMintA` and `tokenMintB` |
| An emptied position | Beyond M1–M6. A position its owner emptied with `decrease_liquidity` still has fees owed, and the compounder would pour them back in. It now collects them and deposits only while the position has liquidity (`an_emptied_position_is_collected_not_refilled`). If refilling is wanted, drop the `hasLiquidity` term from the deposit's `when` and that test |
| One `dustFloor` | Kept. It is compared with each token in that token's base units. Two floors would be more precise, since lamports and micro-USDC differ by 1,000 times, but milestone 1's harness test and both runners use the name. Recorded in FINDINGS |
| The harvest's `whirlpool` | Stays writable: each row's update writes it. M1 corrects only the flag on the `collect_fees` call, which no transaction can observe. So M1's failing test is static (Task 5) |
| M6 | Tests assert `Failure.program`, and `run-orca-harvest.ts::describeFailure` takes the logs. A doc comment in `programs/ballista/src/processor/execute.rs` claims invoked programs' codes are "never confused with Ballista's". Leave it to the program's owner, which keeps clear of `claude/runtime-extensions`; FINDINGS records it |

## Harness API this plan assumes

These signatures are from the files in `.claude/worktrees/protocol-tests/tests/protocols/src/`
on 2026-09-27. They were untracked then (milestone 1 Task 3). If the committed version differs,
adapt the calls and keep what each test checks.

- **`snapshot`**
  - `Snapshot::load(dir) -> Snapshot` checks every hash, takes any directory, and is `Clone`.
  - `.into_svm() -> LiteSVM` sets the snapshot's clock, writes ProgramData before Program, and
    loads Ballista from `target/deploy/ballista.so`.
- **`wallet`**
  - `keypair(&[u8; 32])`, `fund(svm, &address, lamports)`, `SOL` and `WSOL_MINT`.
  - `token_account(svm, owner, mint, amount) -> Address`, which needs the mint in the SVM.
  - `token_balance(svm, &address) -> u64`.
- **`tx`**
  - `send(svm, payer, signers, instructions, tables) -> Result<Outcome, Failure>`. It expires
    the blockhash first and asserts the transaction is at most 1,232 bytes.
  - `transaction(..)`, `wire_size(&transaction)` and `PACKET_DATA_SIZE`.
  - `Outcome { logs, compute_units, fee, size }`.
  - `Failure { program, code, err, logs, fee }`. `program` comes from the first
    `Program <id> failed` line, which names the innermost program that failed.
  - `ballista_error(&Failure)`.
- **`template`**
  - `examples()`, indexed by name, giving
    `Example { payload, fixed_accounts, inputs, row_inputs, batch_accounts, account_groups }`.
  - `upload(svm, creator, id, payload) -> Address`.
  - `Run::new(template, &example)`, then `.account(name, address, writable, signer)`, which
    panics unless the flags are the ones the template declares; `.input_u64(..)` and
    `.input_u128(..)`; `.row(|row| row.account(..))`; and `.build()`.
- **`ballista_sdk`**
  - `decode_ballista_error`, `ASSOCIATED_TOKEN_PROGRAM_ID`, `SYSTEM_PROGRAM_ID`,
    `TOKEN_PROGRAM_ID`.
  - `ballista_common::template::{ProgramView, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, ITERATION_ACCOUNT_BIT}`.
- **Milestone 1's own test.** `template.rs`'s unit test `batch_rows_follow_the_fixed_accounts`
  builds an `orcaHarvestManyPositions` run from whatever names the fixture declares, and binds
  `dustFloor`. Keep that input's name.

---

## File map

| Path | Responsibility |
| --- | --- |
| `.gitattributes` | `tests/protocols/snapshot*/programs/*.so` → LFS |
| `tests/protocols/Cargo.toml`, `Cargo.lock` | `orca_whirlpools_client = "=8.0.0"`, `orca_whirlpools_core = "=2.1.1"` |
| `scripts/snapshot/snapshot.mjs` | Ask Jupiter nothing when the manifests hold no routes |
| `scripts/snapshot/manifests/orca.json` | Both pools, their vaults and tick arrays, both mints, five programs |
| `tests/protocols/snapshot-orca/` | The snapshot; programs in LFS |
| `tests/protocols/src/orca.rs` | The snapshot's directory, pool decoding, and setup: positions, deposits, withdrawals, swaps and updates through Orca's client, fee quotes through Orca's core |
| `tests/protocols/src/lib.rs` | `pub mod orca;` |
| `tests/protocols/tests/orca_snapshot.rs` | The snapshot still brackets each pool's price |
| `tests/protocols/tests/orca_setup.rs` | The Orca behaviors the fixes rely on |
| `tests/protocols/tests/orca_cpis.rs` | Every Whirlpool call matches Orca's client (M1) |
| `tests/protocols/tests/orca_compound_fees.rs` | `orcaCompoundFees` (M2, M3, M4) |
| `tests/protocols/tests/orca_harvest_many_positions.rs` | `orcaHarvestManyPositions` (M2, M3, M5, M6) |
| `clients/js/examples/protocols/shared.ts` | New Orca constants; `ORCA_INCREASE_LIQUIDITY` goes |
| `clients/js/examples/protocols/orca-compound-fees.ts` | The fixed compounder |
| `clients/js/examples/protocols/orca-harvest-many-positions.ts` | The fixed harvest |
| `clients/js/examples/protocols/run-orca-harvest.ts` | Four-account rows, `getOrcaTickArrayAddress`, and `describeFailure(code, logs)` (M6) |
| `clients/js/src/protocol-semantics.test.ts` | The harvest runner's rows, tick arrays and attribution |
| `clients/rust/examples/protocol_runs.rs` | `run_orca_harvest`'s row, in the `#rows` region the docs include |
| `fixtures/protocol-examples.json` | Regenerated with `pnpm fixtures` |
| `tests/protocols/README.md` | The Orca snapshot and how to refresh it |
| `tests/protocols/FINDINGS.md` | A per-template summary for the docs session |

---

### Task 1: Worktree, Orca's crates, and LFS for every snapshot

- [ ] **Step 1: The worktree.**
  - The harness must be committed on `claude/protocol-tests` first (milestone 1 Task 3, "Add
    the protocol-test harness"). This must print a commit:
    `git -C /Users/jacob/Documents/ballista/.claude/worktrees/protocol-tests log --oneline -1 -- tests/protocols/src/template.rs`.
    If it prints nothing, stop and ask. Do not copy the untracked files.
  - Then, following superpowers:using-git-worktrees:

```bash
git -C /Users/jacob/Documents/ballista/.claude/worktrees/protocol-tests \
  worktree add ../protocol-orca -b claude/protocol-orca claude/protocol-tests
cd /Users/jacob/Documents/ballista/.claude/worktrees/protocol-orca
git lfs pull
pnpm install --frozen-lockfile
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
cargo test --manifest-path tests/protocols/Cargo.toml   # milestone 1's suite, green before you start
```

- [ ] **Step 2: LFS for every snapshot.**
  - In `.gitattributes`, replace milestone 1's line with:

```gitattributes
tests/protocols/snapshot*/programs/*.so filter=lfs diff=lfs merge=lfs -text
```

  - Check it. This prints `filter: lfs` for both paths:
    `git check-attr filter -- tests/protocols/snapshot/programs/x.so tests/protocols/snapshot-orca/programs/x.so`.
  - Milestone 3 makes the same change. The lines are identical, so a merge can take either.
- [ ] **Step 3: Orca's crates.**
  - Add them to `tests/protocols/Cargo.toml` under `[dependencies]`, not dev-dependencies,
    because `src/orca.rs` uses them:

```toml
# Orca's own client and math, for setup: instruction builders, PDAs, account decoders, quotes.
orca_whirlpools_client = "=8.0.0"
orca_whirlpools_core = "=2.1.1"
```

  - Run `cargo build --manifest-path tests/protocols/Cargo.toml --tests`.
  - What changes in `Cargo.lock`:
    - it gains the three `orca_whirlpools_*` crates, `ethnum`, `libm` and `memoffset`;
    - it gains a second, 3.x tree of some Solana crates (`solana-program 3.0.0` and the like);
    - existing dependency names become version-qualified;
    - no existing pin moves. Confirm that `git diff tests/protocols/Cargo.lock | grep -c '^-version'`
      prints 0.
  - The client's default feature `core-types` stays on: it converts the client's accounts into
    `orca_whirlpools_core`'s types. Its `fetch` feature stays off.
- [ ] **Step 4:** Commit: "Add Orca's crates to the protocol tests and keep every snapshot's programs
  in LFS".

### Task 2: The Orca snapshot

- [ ] **Step 1: The manifest,** `scripts/snapshot/manifests/orca.json`.
  - The seven tick arrays per pool were current on 2026-09-27: `Czfq…` was near tick −21,000 and
    `HJPj…` the same, about 122 USDC per SOL.
  - Keep the wallet seed. The loader checks that every snapshot names the same wallet.

```json
{
  "description": "Milestone 2 of the real-protocol tests: orcaCompoundFees and orcaHarvestManyPositions against the Whirlpool program. Two SOL/USDC pools under the main WhirlpoolsConfig: Czfq (tick spacing 4, the liquid one) for the compounder, and HJPj (tick spacing 64, thin, one tick array spans about 75% of price) for the harvest, whose rows then share one tick array. Each pool's seven tick arrays are the current one and three either side, as of the last refresh; tests/orca_snapshot.rs fails when the price has left them. No Oracle PDA is listed: neither pool uses adaptive fees, and swaps need the address only. The test wallet is unused by the Orca tests, which sign with their own seeded keys; every snapshot names the same one because the loader checks it.",
  "wallet": {
    "seed": "ballista-protocol-tests-wallet-1"
  },
  "programs": {
    "whirlpool": "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc",
    "token": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
    "token2022": "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
    "associatedToken": "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
    "memo": "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr"
  },
  "accounts": {
    "wsolMint": "So11111111111111111111111111111111111111112",
    "usdcMint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
    "czfqWhirlpool": "Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE",
    "czfqVaultA": "EUuUbDcafPrmVTD5M6qoJAoyyNbihBhugADAxRMn5he9",
    "czfqVaultB": "2WLWEuKDgkDUccTpbwYp1GToYktiSB1cXvreHUwiSUVP",
    "czfqTickArray-22176": "8NPFeBD52yqJWnsmBNma9qXXGEjXa6WatYcLMXjzSeyK",
    "czfqTickArray-21824": "D3461zSTVPNdBFPRk2b6zpqQ93g2LW5Kw2potgMdxNJP",
    "czfqTickArray-21472": "6hA1LN1fzCiXqymDiQXeBFn5da1b7STP1L7JmDc6hR3M",
    "czfqTickArray-21120": "FdtvWk8j5u1a64YK2Uxk9eXxKZJTwLHDGx8aJPbJyw2Q",
    "czfqTickArray-20768": "8fV1MQYaRiW9qDWinzYfNa3FLs8PXSdPxoytiwrcyJPD",
    "czfqTickArray-20416": "HKFHvjsaXWqm5eJpP6minENF6EDK4r86d7G3oYfqD4Q8",
    "czfqTickArray-20064": "8Yoy9SqpLRV1UkLiTkMkNqnuE1bcSAFiYC8jMjS7Niqp",
    "hjpjWhirlpool": "HJPjoWUrhoZzkNfRpHuieeFk9WcZWjwy6PBjZ81ngndJ",
    "hjpjVaultA": "3YQm7ujtXWJU2e9jhp2QGHpnn1ShXn12QjvzMvDgabpX",
    "hjpjVaultB": "2JTw1fE2wz1SymWUQ7UqpVtrTuKjcd6mWwYwUJUCh2rq",
    "hjpjTickArray-39424": "EVqGhR2ukNuqZNfvFFAitrX6UqrRm2r8ayKX9LH9xHzK",
    "hjpjTickArray-33792": "2Eh8HEeu45tCWxY6ruLLRN6VcTSD7bfshGj7bZA87Kne",
    "hjpjTickArray-28160": "A2W6hiA2nf16iqtbZt9vX8FJbiXjv3DBUG3DgTja61HT",
    "hjpjTickArray-22528": "CEstjhG1v4nUgvGDyFruYEbJ18X8XeN4sX1WFCLt4D5c",
    "hjpjTickArray-16896": "HoDhUt77EotPNLUfJuvCCLbmpiM1JR6WLqWxeDPR1xvK",
    "hjpjTickArray-11264": "81T5kNuPRkyVzhwbe2RpKR7wmQpGJ7RBkGPdTqyfa5vq",
    "hjpjTickArray-5632": "9K1HWrGKZKfjTnKfF621BmEQdai4FcUz9tsoF41jwz5B"
  }
}
```

- [ ] **Step 2: A failing check of the tool.** The manifest has no routes, yet the tool still asks
  Jupiter for its dex labels and passes a `minContextSlot` of `-Infinity`. With a bad key, the
  refresh fails for no reason:

```bash
JUPITER_API_KEY=not-a-key node scripts/snapshot/snapshot.mjs \
  scripts/snapshot/manifests/orca.json "$TMPDIR/orca-snapshot-check"
# snapshot failed: Jupiter /program-id-to-label: HTTP 401 {"code":401,"message":"Unauthorized"}
```

- [ ] **Step 3: Ask Jupiter nothing without routes.** In `scripts/snapshot/snapshot.mjs`:

```diff
-  // 1. Routes, from Jupiter.
-  const labels = await dexPrograms();
+  // 1. Routes, from Jupiter. A snapshot without routes asks Jupiter nothing.
+  const labels = plan.routes.size > 0 ? await dexPrograms() : new Map();
```

```diff
-  const minContextSlot = Math.max(...allLegs.map((leg) => Number(leg.quote.contextSlot)));
+  // No older than any quote; without quotes, any slot.
+  const minContextSlot =
+    allLegs.length > 0 ? Math.max(...allLegs.map((leg) => Number(leg.quote.contextSlot))) : undefined;
```

  - Rerun Step 2's command. It now reads 27 accounts at one slot and writes 5 programs and 30
    accounts. Delete the check's directory afterwards.
  - Run `node --test 'scripts/snapshot/*.test.mjs'`; it still passes.
  - If milestone 3 has already landed the same change on the base branch, drop this step.
- [ ] **Step 4: Take the snapshot.**

```bash
node scripts/snapshot/snapshot.mjs scripts/snapshot/manifests/orca.json tests/protocols/snapshot-orca
```

  - Expect in the summary:
    - five programs: Whirlpool, Token, Token-2022, Associated Token and Memo;
    - 30 accounts stored, and 0 referenced but absent.
  - Tick arrays are never closed, so a listed one cannot go missing. What can happen is that the
    price has moved past the listed arrays since this plan. Task 3's test catches that and names
    the array to add. Then replace that pool's seven arrays with the current one and three on
    each side:
    - `start = floor(tick / (88 × tick_spacing)) × 88 × tick_spacing`;
    - the address is `orca_whirlpools_client::get_tick_array_address(&pool, start, None)`, or
      `getOrcaTickArrayAddress` from Task 7.
  - If an array near the price was never initialized on mainnet, the tool reports it absent. Leave
    it out of the manifest, and have the test that needs it create it with Orca's own
    `initialize_tick_array`.
- [ ] **Step 5: Commit.** Run `git add .gitattributes scripts/snapshot tests/protocols/snapshot-orca`,
  then check that `git lfs ls-files` lists the five `snapshot-orca/programs/*.so` files. When the
  programs have not been redeployed, their LFS objects are the ones milestone 1 already stored, so
  nothing new is uploaded. Commit: "Snapshot both SOL/USDC Whirlpools for the Orca tests".

### Task 3: The Orca harness module

- [ ] **Step 1: The failing test,** `tests/protocols/tests/orca_snapshot.rs`. It fails to compile:
  there is no `orca` module yet.

```rust
//! The Orca snapshot holds what the Orca tests trade against, and each pool's price is still
//! inside the tick arrays it took. When a refresh fails here, the manifest's tick arrays have to
//! follow the price.

use {
    ballista_protocol_tests::orca::{self, SOL_USDC, SOL_USDC_THIN, TICKS_PER_ARRAY},
    ballista_sdk::{ASSOCIATED_TOKEN_PROGRAM_ID, TOKEN_PROGRAM_ID},
    orca_whirlpools_client as oc,
    solana_clock::Clock,
};

#[test]
fn both_pools_and_the_tick_arrays_around_their_prices_are_in_the_snapshot() {
    let svm = orca::svm();
    let now = u64::try_from(svm.get_sysvar::<Clock>().unix_timestamp).unwrap();
    for address in [SOL_USDC, SOL_USDC_THIN] {
        let pool = orca::pool(&svm, address);
        let state = orca::whirlpool(&svm, &address);
        // Orca refuses a clock older than the pool's last update: InvalidTimestamp, 6022.
        assert!(
            now >= state.reward_last_updated_timestamp,
            "{address}: the clock is behind the pool"
        );
        let span = TICKS_PER_ARRAY * i32::from(pool.tick_spacing);
        for offset in [-span, 0, span] {
            let tick = state.tick_current_index + offset;
            let array = orca::tick_array(&pool, tick);
            assert!(
                svm.get_account(&array).is_some(),
                "{address} is at tick {}, and the snapshot lacks tick array {array}, which holds \
                 tick {tick}: add it to scripts/snapshot/manifests/orca.json and refresh",
                state.tick_current_index
            );
        }
        // Neither pool has an adaptive-fee Oracle. Swaps name its address, which must stay empty.
        let (oracle, _) = oc::get_oracle_address(&address, None).unwrap();
        assert!(
            svm.get_account(&oracle).is_none(),
            "{address} has an Oracle now"
        );
        for token in [pool.mint_a, pool.vault_a, pool.mint_b, pool.vault_b] {
            let owner = svm.get_account(&token).map(|account| account.owner);
            assert_eq!(owner, Some(TOKEN_PROGRAM_ID), "{address}: {token}");
        }
    }
    for program in [
        orca::WHIRLPOOL,
        TOKEN_PROGRAM_ID,
        orca::TOKEN_2022_PROGRAM,
        ASSOCIATED_TOKEN_PROGRAM_ID,
        orca::MEMO_PROGRAM,
    ] {
        let executable = svm
            .get_account(&program)
            .is_some_and(|account| account.executable);
        assert!(executable, "{program} is not a program in the SVM");
    }
}
```

- [ ] **Step 2: `tests/protocols/src/orca.rs`.**
  - Add `pub mod orca;` to `src/lib.rs`, with a bullet in its module list: "[`orca`] opens
    Orca positions, adds liquidity and swaps through Orca's own instructions, and quotes fees
    with Orca's own math."
  - Everything here goes through Orca's instructions or Orca's quotes. The one direct write is
    wallets' token balances, through `wallet` (write rule 1).
  - `swap_tick_arrays` follows Orca's `sparse_swap.rs`. For B to A, the start is shifted one tick
    spacing up.

```rust
//! Orca Whirlpools for the Orca templates' tests: the snapshot they run on, and setup through
//! Orca's own instructions (built with `orca_whirlpools_client`) and Orca's own math
//! (`orca_whirlpools_core`). Nothing here writes Orca state; wallets' balances go through
//! [`crate::wallet`] (write rule 1).
//!
//! The snapshot holds two SOL/USDC pools under the main WhirlpoolsConfig. [`SOL_USDC`] (tick
//! spacing 4) is the liquid one. [`SOL_USDC_THIN`] (tick spacing 64) is thin, and one of its tick
//! arrays spans about 75% of the price, so positions near the price share one.

use {
    crate::{
        snapshot::Snapshot,
        tx::{self, Failure, Outcome},
        wallet::{fund, keypair, token_account, SOL, WSOL_MINT},
    },
    ballista_sdk::{ASSOCIATED_TOKEN_PROGRAM_ID, SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID},
    litesvm::LiteSVM,
    orca_whirlpools_client as oc, orca_whirlpools_core as oq,
    sha2::{Digest, Sha256},
    solana_address::Address,
    solana_keypair::Keypair,
    solana_signer::Signer,
    std::sync::OnceLock,
};

/// The Orca snapshot, written by `scripts/snapshot/snapshot.mjs` from
/// `scripts/snapshot/manifests/orca.json`.
pub const SNAPSHOT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/snapshot-orca");

/// The Whirlpool program.
pub const WHIRLPOOL: Address = oc::WHIRLPOOL_ID;
/// SOL/USDC at tick spacing 4 and a 0.04% fee: the liquid pool.
pub const SOL_USDC: Address =
    Address::from_str_const("Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE");
/// SOL/USDC at tick spacing 64 and a 0.30% fee: a thin pool.
pub const SOL_USDC_THIN: Address =
    Address::from_str_const("HJPjoWUrhoZzkNfRpHuieeFk9WcZWjwy6PBjZ81ngndJ");
pub const USDC_MINT: Address =
    Address::from_str_const("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
/// Base units per USDC.
pub const USDC: u64 = 1_000_000;
/// SPL Memo, which Orca's v2 instructions take.
pub const MEMO_PROGRAM: Address =
    Address::from_str_const("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");
pub const TOKEN_2022_PROGRAM: Address =
    Address::from_str_const("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
/// Ticks in one tick array.
pub const TICKS_PER_ARRAY: i32 = 88;

/// The NFT metadata authority `open_position_with_token_extensions` names.
const METADATA_UPDATE_AUTHORITY: Address =
    Address::from_str_const("3axbTs2z5GBy6usVbNVoqEgZMng3vZvMnAoX29BFfwhr");
const RENT_SYSVAR: Address = Address::from_str_const("SysvarRent111111111111111111111111111111111");

/// A LiteSVM holding the Orca snapshot, with Ballista built from source. The snapshot is read and
/// checked once per test binary.
pub fn svm() -> LiteSVM {
    static SNAPSHOT: OnceLock<Snapshot> = OnceLock::new();
    SNAPSHOT
        .get_or_init(|| Snapshot::load(SNAPSHOT_DIR))
        .clone()
        .into_svm()
}

/// A fixed 32-byte key seed named by `label`: fixed keys keep addresses and compute units the
/// same from run to run.
pub fn seed(label: &str) -> [u8; 32] {
    let mut seed = [0; 32];
    seed.copy_from_slice(&Sha256::digest(label.as_bytes()));
    seed
}

/// A pool's addresses.
#[derive(Clone, Copy, Debug)]
pub struct Pool {
    pub address: Address,
    pub tick_spacing: u16,
    pub mint_a: Address,
    pub mint_b: Address,
    pub vault_a: Address,
    pub vault_b: Address,
}

/// A pool as the SVM holds it now.
///
/// # Panics
///
/// If `pool` is not a Whirlpool in the SVM.
pub fn whirlpool(svm: &LiteSVM, pool: &Address) -> oc::Whirlpool {
    let account = svm
        .get_account(pool)
        .unwrap_or_else(|| panic!("whirlpool {pool} is not in the SVM"));
    oc::Whirlpool::from_bytes(&account.data)
        .unwrap_or_else(|error| panic!("{pool} is not a Whirlpool: {error}"))
}

pub fn pool(svm: &LiteSVM, address: Address) -> Pool {
    let state = whirlpool(svm, &address);
    Pool {
        address,
        tick_spacing: state.tick_spacing,
        mint_a: state.token_mint_a,
        mint_b: state.token_mint_b,
        vault_a: state.token_vault_a,
        vault_b: state.token_vault_b,
    }
}

/// The tick array holding `tick`: `["tick_array", pool, start]`, where `start` is its first tick.
pub fn tick_array(pool: &Pool, tick: i32) -> Address {
    let start = oq::get_tick_array_start_tick_index(tick, pool.tick_spacing);
    oc::get_tick_array_address(&pool.address, start, None)
        .expect("a tick array address")
        .0
}

/// Initializable ticks `from` and `to` ticks from the current one, rounded outward.
pub fn range(svm: &LiteSVM, pool: &Pool, from: i32, to: i32) -> (i32, i32) {
    let tick = whirlpool(svm, &pool.address).tick_current_index;
    (
        oq::get_initializable_tick_index(tick + from, pool.tick_spacing, Some(false)),
        oq::get_initializable_tick_index(tick + to, pool.tick_spacing, Some(true)),
    )
}

/// A keypair with a wSOL and a USDC account: the two tokens both pools trade.
pub struct TokenWallet {
    pub keypair: Keypair,
    /// wSOL, the pools' token A.
    pub token_a: Address,
    /// USDC, the pools' token B.
    pub token_b: Address,
}

/// A wallet whose associated token accounts hold `sol` lamports of wSOL and `usdc` base units of
/// USDC (write rule 1), with 100 SOL besides for fees and rent.
pub fn token_wallet(svm: &mut LiteSVM, seed: &[u8; 32], sol: u64, usdc: u64) -> TokenWallet {
    let keypair = keypair(seed);
    fund(svm, &keypair.pubkey(), 100 * SOL);
    let token_a = token_account(svm, &keypair.pubkey(), &WSOL_MINT, sol);
    let token_b = token_account(svm, &keypair.pubkey(), &USDC_MINT, usdc);
    TokenWallet {
        keypair,
        token_a,
        token_b,
    }
}

/// The token program that holds a position's NFT.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Nft {
    Token,
    Token2022,
}

#[derive(Clone, Copy, Debug)]
pub struct Position {
    pub address: Address,
    /// The owner's token account holding the position's NFT.
    pub token_account: Address,
    pub lower: i32,
    pub upper: i32,
}

/// Opens a position on `[lower, upper]` for `owner`, who pays. `mint` becomes its NFT's mint.
pub fn open_position(
    svm: &mut LiteSVM,
    pool: &Pool,
    owner: &TokenWallet,
    mint: &Keypair,
    (lower, upper): (i32, i32),
    nft: Nft,
) -> Position {
    let owner_address = owner.keypair.pubkey();
    let (position, bump) =
        oc::get_position_address(&mint.pubkey(), None).expect("a position address");
    let nft_program = match nft {
        Nft::Token => TOKEN_PROGRAM_ID,
        Nft::Token2022 => TOKEN_2022_PROGRAM,
    };
    let token_account = Address::find_program_address(
        &[
            owner_address.as_ref(),
            nft_program.as_ref(),
            mint.pubkey().as_ref(),
        ],
        &ASSOCIATED_TOKEN_PROGRAM_ID,
    )
    .0;
    let instruction = match nft {
        Nft::Token => oc::OpenPosition {
            funder: owner_address,
            owner: owner_address,
            position,
            position_mint: mint.pubkey(),
            position_token_account: token_account,
            whirlpool: pool.address,
            token_program: TOKEN_PROGRAM_ID,
            system_program: SYSTEM_PROGRAM_ID,
            rent: RENT_SYSVAR,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
        }
        .instruction(oc::OpenPositionInstructionArgs {
            position_bump: bump,
            tick_lower_index: lower,
            tick_upper_index: upper,
        }),
        Nft::Token2022 => oc::OpenPositionWithTokenExtensions {
            funder: owner_address,
            owner: owner_address,
            position,
            position_mint: mint.pubkey(),
            position_token_account: token_account,
            whirlpool: pool.address,
            token2022_program: TOKEN_2022_PROGRAM,
            system_program: SYSTEM_PROGRAM_ID,
            associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
            metadata_update_auth: METADATA_UPDATE_AUTHORITY,
        }
        .instruction(oc::OpenPositionWithTokenExtensionsInstructionArgs {
            tick_lower_index: lower,
            tick_upper_index: upper,
            with_token_metadata_extension: false,
        }),
    };
    tx::send(svm, &owner.keypair, &[mint], &[instruction], &[])
        .unwrap_or_else(|failure| panic!("open_position failed: {failure:?}"));
    Position {
        address: position,
        token_account,
        lower,
        upper,
    }
}

/// Adds `liquidity` to `position` with Orca's `increase_liquidity`, capped 1% above Orca's quote.
pub fn deposit(
    svm: &mut LiteSVM,
    pool: &Pool,
    owner: &TokenWallet,
    position: &Position,
    liquidity: u128,
) {
    let sqrt_price = whirlpool(svm, &pool.address).sqrt_price;
    let quote = oq::increase_liquidity_quote(
        liquidity.into(),
        100,
        sqrt_price.into(),
        position.lower,
        position.upper,
        None,
        None,
    )
    .expect("an increase_liquidity quote");
    let instruction = oc::IncreaseLiquidity {
        whirlpool: pool.address,
        token_program: TOKEN_PROGRAM_ID,
        position_authority: owner.keypair.pubkey(),
        position: position.address,
        position_token_account: position.token_account,
        token_owner_account_a: owner.token_a,
        token_owner_account_b: owner.token_b,
        token_vault_a: pool.vault_a,
        token_vault_b: pool.vault_b,
        tick_array_lower: tick_array(pool, position.lower),
        tick_array_upper: tick_array(pool, position.upper),
    }
    .instruction(oc::IncreaseLiquidityInstructionArgs {
        liquidity_amount: liquidity,
        token_max_a: quote.token_max_a,
        token_max_b: quote.token_max_b,
    });
    tx::send(svm, &owner.keypair, &[], &[instruction], &[])
        .unwrap_or_else(|failure| panic!("increase_liquidity failed: {failure:?}"));
}

/// Removes all of `position`'s liquidity with Orca's `decrease_liquidity`, which also records the
/// fees it had earned.
pub fn withdraw_all(svm: &mut LiteSVM, pool: &Pool, owner: &TokenWallet, position: &Position) {
    let liquidity = position_state(svm, position).liquidity;
    let instruction = oc::DecreaseLiquidity {
        whirlpool: pool.address,
        token_program: TOKEN_PROGRAM_ID,
        position_authority: owner.keypair.pubkey(),
        position: position.address,
        position_token_account: position.token_account,
        token_owner_account_a: owner.token_a,
        token_owner_account_b: owner.token_b,
        token_vault_a: pool.vault_a,
        token_vault_b: pool.vault_b,
        tick_array_lower: tick_array(pool, position.lower),
        tick_array_upper: tick_array(pool, position.upper),
    }
    .instruction(oc::DecreaseLiquidityInstructionArgs {
        liquidity_amount: liquidity,
        token_min_a: 0,
        token_min_b: 0,
    });
    tx::send(svm, &owner.keypair, &[], &[instruction], &[])
        .unwrap_or_else(|failure| panic!("decrease_liquidity failed: {failure:?}"));
}

/// Swaps `amount` of the input token through `pool`: token A for B when `a_to_b`. No price limit
/// and no minimum out; these swaps exist to pay fees.
pub fn swap(svm: &mut LiteSVM, pool: &Pool, trader: &TokenWallet, amount: u64, a_to_b: bool) {
    let tick = whirlpool(svm, &pool.address).tick_current_index;
    let [tick_array0, tick_array1, tick_array2] = swap_tick_arrays(pool, tick, a_to_b);
    let instruction = oc::Swap {
        token_program: TOKEN_PROGRAM_ID,
        token_authority: trader.keypair.pubkey(),
        whirlpool: pool.address,
        token_owner_account_a: trader.token_a,
        token_vault_a: pool.vault_a,
        token_owner_account_b: trader.token_b,
        token_vault_b: pool.vault_b,
        tick_array0,
        tick_array1,
        tick_array2,
        oracle: oc::get_oracle_address(&pool.address, None)
            .expect("an oracle address")
            .0,
    }
    .instruction(oc::SwapInstructionArgs {
        amount,
        other_amount_threshold: 0,
        sqrt_price_limit: 0,
        amount_specified_is_input: true,
        a_to_b,
    });
    tx::send(svm, &trader.keypair, &[], &[instruction], &[])
        .unwrap_or_else(|failure| panic!("swap failed: {failure:?}"));
}

/// The three tick arrays a swap from `tick` walks, laid out as Orca's `sparse_swap.rs` does: from
/// the current array downward for A to B, and upward from one tick spacing higher for B to A. An
/// array that does not exist reads as one without initialized ticks.
fn swap_tick_arrays(pool: &Pool, tick: i32, a_to_b: bool) -> [Address; 3] {
    let spacing = i32::from(pool.tick_spacing);
    let span = TICKS_PER_ARRAY * spacing;
    let (from, step) = if a_to_b {
        (tick, -span)
    } else {
        (tick + spacing, span)
    };
    let start = oq::get_tick_array_start_tick_index(from, pool.tick_spacing);
    [0, 1, 2].map(|index| {
        oc::get_tick_array_address(&pool.address, start + index * step, None)
            .expect("a tick array address")
            .0
    })
}

/// Orca's `update_fees_and_rewards` for `position`. It needs no signature; `payer` pays.
pub fn update_fees(
    svm: &mut LiteSVM,
    pool: &Pool,
    payer: &Keypair,
    position: &Position,
) -> Result<Outcome, Failure> {
    let instruction = oc::UpdateFeesAndRewards {
        whirlpool: pool.address,
        position: position.address,
        tick_array_lower: tick_array(pool, position.lower),
        tick_array_upper: tick_array(pool, position.upper),
    }
    .instruction();
    tx::send(svm, payer, &[], &[instruction], &[])
}

/// A position's liquidity and the fees it records as owed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PositionState {
    pub liquidity: u128,
    pub fee_owed_a: u64,
    pub fee_owed_b: u64,
}

pub fn position_state(svm: &LiteSVM, position: &Position) -> PositionState {
    let account = svm
        .get_account(&position.address)
        .unwrap_or_else(|| panic!("position {} does not exist", position.address));
    let decoded = oc::Position::from_bytes(&account.data).expect("a Whirlpool position");
    PositionState {
        liquidity: decoded.liquidity,
        fee_owed_a: decoded.fee_owed_a,
        fee_owed_b: decoded.fee_owed_b,
    }
}

/// The fees `update_fees_and_rewards` would record for `position` now, from Orca's
/// `collect_fees_quote`: `(fee_owed_a, fee_owed_b)`.
pub fn fees_owed_now(svm: &LiteSVM, pool: &Pool, position: &Position) -> (u64, u64) {
    let tick = |index: i32| -> oq::TickFacade {
        let account = svm
            .get_account(&tick_array(pool, index))
            .unwrap_or_else(|| panic!("the tick array holding tick {index} is not in the SVM"));
        let array: oq::TickArrayFacade = oc::FixedTickArray::from_bytes(&account.data)
            .expect("a fixed tick array")
            .into();
        let offset = oq::get_tick_index_in_array(index, array.start_tick_index, pool.tick_spacing)
            .expect("the tick is in its array");
        array.ticks[offset as usize]
    };
    let state = oc::Position::from_bytes(&svm.get_account(&position.address).unwrap().data)
        .expect("a Whirlpool position");
    let quote = oq::collect_fees_quote(
        whirlpool(svm, &pool.address).into(),
        state.into(),
        tick(position.lower),
        tick(position.upper),
        None,
        None,
    )
    .expect("a collect_fees quote");
    (quote.fee_owed_a, quote.fee_owed_b)
}

/// How many times `program` was invoked one level down: in a run, the template's CPIs to it.
pub fn cpis_to(logs: &[String], program: &Address) -> usize {
    let invoked = format!("Program {program} invoke [2]");
    logs.iter().filter(|line| **line == invoked).count()
}
```

- [ ] **Step 3:** Run `cargo test --manifest-path tests/protocols/Cargo.toml --test orca_snapshot`.
  It passes. If it names a missing tick array, go back to Task 2 Step 4.
- [ ] **Step 4:** Commit: "Add the Orca module to the protocol-test harness".

### Task 4: The Orca behaviors the fixes rely on

- [ ] **Step 1: `tests/protocols/tests/orca_setup.rs`.** These call Whirlpools directly, without
  Ballista, and pass at once: they pin Orca's behavior, not a template's. Each is the root of one
  finding:
  - the fees a position records are stale until an update (M2);
  - an empty collect succeeds (M5);
  - an update on a position without liquidity fails with 6012, which is why the templates skip it;
  - inside its range, a position takes liquidity only in both tokens (M4).

```rust
//! What Orca's own instructions do, which the Orca templates' fixes rest on. These call Whirlpools
//! directly, without Ballista: if Orca changes any of it, the templates' tests fail too, and these
//! say why.

use {
    ballista_protocol_tests::{
        orca::{self, Nft, Pool, Position, TokenWallet, SOL_USDC, USDC, WHIRLPOOL},
        tx,
        wallet::{keypair, token_balance, SOL},
    },
    ballista_sdk::TOKEN_PROGRAM_ID,
    litesvm::LiteSVM,
    orca_whirlpools_client as oc, orca_whirlpools_core as oq,
    solana_signer::Signer,
};

/// A position 60 ticks either side of the price, holding a quarter of the pool's liquidity.
fn funded_position(label: &str) -> (LiteSVM, Pool, TokenWallet, TokenWallet, Position) {
    let mut svm = orca::svm();
    let pool = orca::pool(&svm, SOL_USDC);
    let owner = orca::token_wallet(
        &mut svm,
        &orca::seed(&format!("{label} owner")),
        10_000 * SOL,
        2_000_000 * USDC,
    );
    let trader = orca::token_wallet(
        &mut svm,
        &orca::seed(&format!("{label} trader")),
        100_000 * SOL,
        20_000_000 * USDC,
    );
    let mint = keypair(&orca::seed(&format!("{label} position")));
    let range = orca::range(&svm, &pool, -60, 60);
    let position = orca::open_position(&mut svm, &pool, &owner, &mint, range, Nft::Token);
    let liquidity = orca::whirlpool(&svm, &SOL_USDC).liquidity / 4;
    orca::deposit(&mut svm, &pool, &owner, &position, liquidity);
    (svm, pool, owner, trader, position)
}

/// The root of M2: `fee_owed_*` is what the last update recorded, not what swaps have earned since.
#[test]
fn fees_owed_rise_only_when_the_position_is_updated() {
    let (mut svm, pool, _, trader, position) = funded_position("setup stale");
    orca::swap(&mut svm, &pool, &trader, 200 * SOL, true);
    orca::swap(&mut svm, &pool, &trader, 30_000 * USDC, false);

    let earned = orca::fees_owed_now(&svm, &pool, &position);
    assert!(earned.0 > 0 && earned.1 > 0, "{earned:?}");
    let stale = orca::position_state(&svm, &position);
    assert_eq!((stale.fee_owed_a, stale.fee_owed_b), (0, 0));
    // Permissionless: the trader updates the owner's position.
    orca::update_fees(&mut svm, &pool, &trader.keypair, &position).unwrap();
    let updated = orca::position_state(&svm, &position);
    assert_eq!((updated.fee_owed_a, updated.fee_owed_b), earned);
}

/// The root of M5: an empty collect is not refused, so skipping it saves compute, not a revert.
#[test]
fn collecting_with_nothing_owed_succeeds_and_moves_nothing() {
    let (mut svm, pool, owner, _, position) = funded_position("setup empty collect");
    let collect = oc::CollectFees {
        whirlpool: pool.address,
        position_authority: owner.keypair.pubkey(),
        position: position.address,
        position_token_account: position.token_account,
        token_owner_account_a: owner.token_a,
        token_vault_a: pool.vault_a,
        token_owner_account_b: owner.token_b,
        token_vault_b: pool.vault_b,
        token_program: TOKEN_PROGRAM_ID,
    }
    .instruction();
    let before = (
        token_balance(&svm, &owner.token_a),
        token_balance(&svm, &owner.token_b),
    );
    tx::send(&mut svm, &owner.keypair, &[], &[collect], &[]).unwrap();
    let after = (
        token_balance(&svm, &owner.token_a),
        token_balance(&svm, &owner.token_b),
    );
    assert_eq!(after, before);
}

/// Why the templates skip the update for a position without liquidity.
#[test]
fn a_position_without_liquidity_cannot_be_updated() {
    let mut svm = orca::svm();
    let pool = orca::pool(&svm, SOL_USDC);
    let owner = orca::token_wallet(&mut svm, &orca::seed("setup empty owner"), SOL, USDC);
    let mint = keypair(&orca::seed("setup empty position"));
    let range = orca::range(&svm, &pool, -60, 60);
    let position = orca::open_position(&mut svm, &pool, &owner, &mint, range, Nft::Token);

    let failure = orca::update_fees(&mut svm, &pool, &owner.keypair, &position).unwrap_err();
    assert_eq!(
        (failure.program, failure.code),
        (WHIRLPOOL, Some(6012)),
        "LiquidityZero: {failure:?}"
    );
}

/// The root of M4: inside its range, a position takes liquidity only in both tokens. With one cap
/// at zero, `increase_liquidity` fails whatever liquidity it is asked for, and
/// `increase_liquidity_by_token_amounts_v2` works out zero liquidity and fails.
#[test]
fn in_range_liquidity_needs_both_tokens() {
    let (mut svm, pool, owner, _, position) = funded_position("setup one-sided");
    let by_liquidity = oc::IncreaseLiquidity {
        whirlpool: pool.address,
        token_program: TOKEN_PROGRAM_ID,
        position_authority: owner.keypair.pubkey(),
        position: position.address,
        position_token_account: position.token_account,
        token_owner_account_a: owner.token_a,
        token_owner_account_b: owner.token_b,
        token_vault_a: pool.vault_a,
        token_vault_b: pool.vault_b,
        tick_array_lower: orca::tick_array(&pool, position.lower),
        tick_array_upper: orca::tick_array(&pool, position.upper),
    }
    .instruction(oc::IncreaseLiquidityInstructionArgs {
        liquidity_amount: 1_000_000,
        token_max_a: SOL,
        token_max_b: 0,
    });
    let failure = tx::send(&mut svm, &owner.keypair, &[], &[by_liquidity], &[]).unwrap_err();
    assert_eq!(
        (failure.program, failure.code),
        (WHIRLPOOL, Some(6017)),
        "TokenMaxExceeded: {failure:?}"
    );

    let by_amounts = oc::IncreaseLiquidityByTokenAmountsV2 {
        whirlpool: pool.address,
        token_program_a: TOKEN_PROGRAM_ID,
        token_program_b: TOKEN_PROGRAM_ID,
        memo_program: orca::MEMO_PROGRAM,
        position_authority: owner.keypair.pubkey(),
        position: position.address,
        position_token_account: position.token_account,
        token_mint_a: pool.mint_a,
        token_mint_b: pool.mint_b,
        token_owner_account_a: owner.token_a,
        token_owner_account_b: owner.token_b,
        token_vault_a: pool.vault_a,
        token_vault_b: pool.vault_b,
        tick_array_lower: orca::tick_array(&pool, position.lower),
        tick_array_upper: orca::tick_array(&pool, position.upper),
    }
    .instruction(oc::IncreaseLiquidityByTokenAmountsV2InstructionArgs {
        method: oc::IncreaseLiquidityMethod::ByTokenAmounts {
            token_max_a: SOL,
            token_max_b: 0,
            min_sqrt_price: oq::MIN_SQRT_PRICE,
            max_sqrt_price: oq::MAX_SQRT_PRICE,
        },
        remaining_accounts_info: None,
    });
    let failure = tx::send(&mut svm, &owner.keypair, &[], &[by_amounts], &[]).unwrap_err();
    assert_eq!(
        (failure.program, failure.code),
        (WHIRLPOOL, Some(6012)),
        "LiquidityZero: {failure:?}"
    );
}
```

- [ ] **Step 2:** Run `cargo test --manifest-path tests/protocols/Cargo.toml --test orca_setup`,
  then commit: "Pin the Whirlpool behaviors the Orca templates depend on".

### Task 5: Every Whirlpool call matches Orca's client (M1)

- [ ] **Step 1: The failing test,** `tests/protocols/tests/orca_cpis.rs`.
  - For each Whirlpool call the two templates make, it compares the accounts, their order and
    their signer and writable flags with what `orca_whirlpools_client` builds for the same
    instruction.
  - It knows the four instructions the templates use now or will use. Any other panics with
    "add it here".
  - A run can never catch M1: the runtime accepts an account passed with more privilege than the
    callee declares.

```rust
//! Every Whirlpool call the Orca templates make passes the accounts that Orca's own client passes
//! for the same instruction: the same accounts, in the same order, with the same signer and
//! writable flags.
//!
//! A run cannot check the flags. The runtime accepts an account passed with more privilege than
//! the callee declares, so a writable flag the callee does not need is never refused; it only
//! write-locks the account. So this reads the compiled templates.

use {
    ballista_protocol_tests::{orca::WHIRLPOOL, template::examples},
    ballista_sdk::ballista_common::template::{
        ProgramView, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, ITERATION_ACCOUNT_BIT,
    },
    orca_whirlpools_client as oc,
    solana_address::Address,
    solana_instruction::AccountMeta,
    std::collections::HashMap,
};

/// A stand-in address per account name, so a list built from names compares by name.
fn key(name: &str) -> Address {
    assert!(name.len() <= 32, "{name} is longer than an address");
    let mut bytes = [0; 32];
    bytes[..name.len()].copy_from_slice(name.as_bytes());
    Address::new_from_array(bytes)
}

fn name_of(address: &Address) -> String {
    String::from_utf8_lossy(address.as_ref())
        .trim_end_matches('\0')
        .to_string()
}

/// The instruction's name and the accounts Orca's client builds for it, each named as the
/// templates name it.
fn orca_accounts(discriminator: &[u8]) -> (&'static str, Vec<AccountMeta>) {
    let k = key;
    if discriminator == oc::COLLECT_FEES_DISCRIMINATOR {
        let instruction = oc::CollectFees {
            whirlpool: k("whirlpool"),
            position_authority: k("positionAuthority"),
            position: k("position"),
            position_token_account: k("positionTokenAccount"),
            token_owner_account_a: k("tokenOwnerAccountA"),
            token_vault_a: k("tokenVaultA"),
            token_owner_account_b: k("tokenOwnerAccountB"),
            token_vault_b: k("tokenVaultB"),
            token_program: k("tokenProgram"),
        }
        .instruction();
        ("collect_fees", instruction.accounts)
    } else if discriminator == oc::UPDATE_FEES_AND_REWARDS_DISCRIMINATOR {
        let instruction = oc::UpdateFeesAndRewards {
            whirlpool: k("whirlpool"),
            position: k("position"),
            tick_array_lower: k("tickArrayLower"),
            tick_array_upper: k("tickArrayUpper"),
        }
        .instruction();
        ("update_fees_and_rewards", instruction.accounts)
    } else if discriminator == oc::INCREASE_LIQUIDITY_DISCRIMINATOR {
        let instruction = oc::IncreaseLiquidity {
            whirlpool: k("whirlpool"),
            token_program: k("tokenProgram"),
            position_authority: k("positionAuthority"),
            position: k("position"),
            position_token_account: k("positionTokenAccount"),
            token_owner_account_a: k("tokenOwnerAccountA"),
            token_owner_account_b: k("tokenOwnerAccountB"),
            token_vault_a: k("tokenVaultA"),
            token_vault_b: k("tokenVaultB"),
            tick_array_lower: k("tickArrayLower"),
            tick_array_upper: k("tickArrayUpper"),
        }
        .instruction(oc::IncreaseLiquidityInstructionArgs {
            liquidity_amount: 0,
            token_max_a: 0,
            token_max_b: 0,
        });
        ("increase_liquidity", instruction.accounts)
    } else if discriminator == oc::INCREASE_LIQUIDITY_BY_TOKEN_AMOUNTS_V2_DISCRIMINATOR {
        let instruction = oc::IncreaseLiquidityByTokenAmountsV2 {
            whirlpool: k("whirlpool"),
            token_program_a: k("tokenProgram"),
            token_program_b: k("tokenProgram"),
            memo_program: k("memoProgram"),
            position_authority: k("positionAuthority"),
            position: k("position"),
            position_token_account: k("positionTokenAccount"),
            token_mint_a: k("tokenMintA"),
            token_mint_b: k("tokenMintB"),
            token_owner_account_a: k("tokenOwnerAccountA"),
            token_owner_account_b: k("tokenOwnerAccountB"),
            token_vault_a: k("tokenVaultA"),
            token_vault_b: k("tokenVaultB"),
            tick_array_lower: k("tickArrayLower"),
            tick_array_upper: k("tickArrayUpper"),
        }
        .instruction(oc::IncreaseLiquidityByTokenAmountsV2InstructionArgs {
            method: oc::IncreaseLiquidityMethod::ByTokenAmounts {
                token_max_a: 0,
                token_max_b: 0,
                min_sqrt_price: 0,
                max_sqrt_price: 0,
            },
            remaining_accounts_info: None,
        });
        (
            "increase_liquidity_by_token_amounts_v2",
            instruction.accounts,
        )
    } else {
        panic!("a Whirlpool instruction with discriminator {discriminator:02x?}: add it here");
    }
}

fn flags(meta: &AccountMeta) -> &'static str {
    match (meta.is_signer, meta.is_writable) {
        (true, true) => "a writable signer",
        (true, false) => "a read-only signer",
        (false, true) => "writable",
        (false, false) => "read-only",
    }
}

#[test]
fn every_whirlpool_call_passes_the_accounts_orcas_client_does() {
    let examples = examples();
    let mut differences = Vec::new();
    for name in ["orcaCompoundFees", "orcaHarvestManyPositions"] {
        let example = &examples[name];
        let view = ProgramView::parse(&example.payload).unwrap();
        let names: HashMap<u8, &str> = example
            .fixed_accounts
            .iter()
            .enumerate()
            .map(|(index, name)| (index as u8, name.as_str()))
            .chain(
                example
                    .batch_accounts
                    .iter()
                    .enumerate()
                    .map(|(index, name)| (index as u8 | ITERATION_ACCOUNT_BIT, name.as_str())),
            )
            .collect();
        for cpi in view.cpis {
            let program = &view.accounts[usize::from(cpi.program_account)];
            let pinned = view.pubkeys[usize::from(program.address_index)].bytes;
            assert_eq!(
                pinned,
                WHIRLPOOL.to_bytes(),
                "{name} calls a program other than Whirlpools"
            );
            let segment = &view.data_segments[cpi.segment_start()];
            let discriminator = &view.blob[segment.offset()..segment.offset() + 8];
            let (instruction, expected) = orca_accounts(discriminator);
            let records = &view.cpi_accounts[cpi.account_start()..][..usize::from(cpi.account_len)];
            let passed: Vec<AccountMeta> = records
                .iter()
                .map(|record| AccountMeta {
                    pubkey: key(names[&record.account]),
                    is_signer: record.flags & ACCOUNT_SIGNER != 0,
                    is_writable: record.flags & ACCOUNT_WRITABLE != 0,
                })
                .collect();
            if passed.len() != expected.len() {
                differences.push(format!(
                    "{name} {instruction}: {} accounts, and Orca's client passes {}",
                    passed.len(),
                    expected.len()
                ));
                continue;
            }
            for (index, (passed, expected)) in passed.iter().zip(&expected).enumerate() {
                if passed.pubkey != expected.pubkey {
                    differences.push(format!(
                        "{name} {instruction}: account {index} is {}, and Orca's client passes {}",
                        name_of(&passed.pubkey),
                        name_of(&expected.pubkey)
                    ));
                } else if (passed.is_signer, passed.is_writable)
                    != (expected.is_signer, expected.is_writable)
                {
                    differences.push(format!(
                        "{name} {instruction}: {} is {} here and {} in Orca's client",
                        name_of(&passed.pubkey),
                        flags(passed),
                        flags(expected)
                    ));
                }
            }
        }
    }
    assert!(differences.is_empty(), "\n{}", differences.join("\n"));
}
```

  - Run `cargo test --manifest-path tests/protocols/Cargo.toml --test orca_cpis`. Expect:

```text
orcaCompoundFees collect_fees: whirlpool is writable here and read-only in Orca's client
orcaHarvestManyPositions collect_fees: whirlpool is writable here and read-only in Orca's client
```

- [ ] **Step 2: The fix.**
  - In both templates' `collect_fees` invoke, set the whirlpool's entry to
    `{ account: account.fixed('whirlpool'), signer: false, writable: false }`.
  - Leave both schemas' `whirlpool: { writable: true }`. The compounder's other calls write the
    pool, and so will each harvest row's update in Task 7.
  - So the fix changes no transaction's locks. It makes the call match Orca's IDL.
- [ ] **Step 3:** Run `pnpm fixtures`, then the test again: it passes. Also run
  `cargo test -p ballista-common every_shared_fixture_parses_and_verifies`. Commit: "Pass the
  whirlpool read-only to collect_fees, as Orca declares it".

### Task 6: `orcaCompoundFees`: update first, collect either fee, reinvest by token amounts

The test file is written once, in Step 1. Steps 2–4 change the template and are staged so that
each finding fails a real run before its fix. After each template edit, run `pnpm fixtures`, then
`cargo test --manifest-path tests/protocols/Cargo.toml --test orca_compound_fees --test orca_cpis`.
The static test from Task 5 checks every call a step adds.

- [ ] **Step 1: The tests,** `tests/protocols/tests/orca_compound_fees.rs`.
  - This is the final file. Until Step 4 the template still takes `liquidityAmount`, so first
    use the transitional `compound` below in place of the file's, together with its helper
    `fitting_liquidity`. The tests' bodies stay the same throughout.
  - Setup: the position spans 60 ticks either side of `Czfq…`'s price and holds a quarter of the
    pool's liquidity. Real swaps then pay it fees: 200 SOL sold and 30,000 USDC sold. Only
    wallets' balances are written.

```rust
//! `orcaCompoundFees` against the real Whirlpool program. Fees come from real swaps through the
//! SOL/USDC pool, and the template has to collect and reinvest them.

use {
    ballista_protocol_tests::{
        orca::{self, Nft, Pool, Position, PositionState, TokenWallet, SOL_USDC, USDC, WHIRLPOOL},
        template::{examples, upload, Example, Run},
        tx::{self, ballista_error, Failure, Outcome},
        wallet::{fund, keypair, token_balance, SOL},
    },
    ballista_sdk::TOKEN_PROGRAM_ID,
    litesvm::LiteSVM,
    orca_whirlpools_core as oq,
    solana_address::Address,
    solana_signer::Signer,
};

const EXAMPLE: &str = "orcaCompoundFees";

/// The pool as snapshotted, a position 60 ticks either side of the price holding a quarter of the
/// pool's liquidity, a trader, and the template.
struct Setup {
    svm: LiteSVM,
    pool: Pool,
    owner: TokenWallet,
    trader: TokenWallet,
    position: Position,
    template: Address,
}

fn setup(example: &Example, funded: bool) -> Setup {
    let mut svm = orca::svm();
    let pool = orca::pool(&svm, SOL_USDC);
    let owner = orca::token_wallet(
        &mut svm,
        &orca::seed("compound owner"),
        10_000 * SOL,
        2_000_000 * USDC,
    );
    let trader = orca::token_wallet(
        &mut svm,
        &orca::seed("compound trader"),
        100_000 * SOL,
        20_000_000 * USDC,
    );
    let mint = keypair(&orca::seed("compound position"));
    let range = orca::range(&svm, &pool, -60, 60);
    let position = orca::open_position(&mut svm, &pool, &owner, &mint, range, Nft::Token);
    if funded {
        let liquidity = orca::whirlpool(&svm, &SOL_USDC).liquidity / 4;
        orca::deposit(&mut svm, &pool, &owner, &position, liquidity);
    }
    let creator = keypair(b"ballista-protocol-tests-creator1");
    fund(&mut svm, &creator.pubkey(), 10 * SOL);
    let template = upload(&mut svm, &creator, 1, &example.payload);
    Setup {
        svm,
        pool,
        owner,
        trader,
        position,
        template,
    }
}

/// The sqrt-price bounds Orca's SDK would sign with at `bps` of slippage.
fn slippage_bounds(setup: &Setup, bps: u16) -> (u128, u128) {
    let sqrt_price = orca::whirlpool(&setup.svm, &SOL_USDC).sqrt_price;
    let bounds = oq::get_sqrt_price_slippage_bounds(sqrt_price.into(), bps);
    (bounds.min_sqrt_price.into(), bounds.max_sqrt_price.into())
}

/// Runs the template for the setup's position, signed by its owner.
fn compound(
    setup: &mut Setup,
    example: &Example,
    dust_floor: u64,
    (min_sqrt_price, max_sqrt_price): (u128, u128),
) -> Result<Outcome, Failure> {
    let Setup {
        pool,
        owner,
        position,
        ..
    } = &*setup;
    let run = Run::new(setup.template, example)
        .account("whirlpoolProgram", WHIRLPOOL, false, false)
        .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
        .account("memoProgram", orca::MEMO_PROGRAM, false, false)
        .account("positionAuthority", owner.keypair.pubkey(), false, true)
        .account("whirlpool", pool.address, true, false)
        .account("position", position.address, true, false)
        .account("positionTokenAccount", position.token_account, false, false)
        .account("tokenMintA", pool.mint_a, false, false)
        .account("tokenMintB", pool.mint_b, false, false)
        .account("tokenOwnerAccountA", owner.token_a, true, false)
        .account("tokenOwnerAccountB", owner.token_b, true, false)
        .account("tokenVaultA", pool.vault_a, true, false)
        .account("tokenVaultB", pool.vault_b, true, false)
        .account(
            "tickArrayLower",
            orca::tick_array(pool, position.lower),
            true,
            false,
        )
        .account(
            "tickArrayUpper",
            orca::tick_array(pool, position.upper),
            true,
            false,
        )
        .input_u64("dustFloor", dust_floor)
        .input_u128("minSqrtPrice", min_sqrt_price)
        .input_u128("maxSqrtPrice", max_sqrt_price)
        .build();
    let payer = setup.owner.keypair.insecure_clone();
    tx::send(&mut setup.svm, &payer, &[], &[run], &[])
}

fn balances(setup: &Setup) -> (u64, u64) {
    (
        token_balance(&setup.svm, &setup.owner.token_a),
        token_balance(&setup.svm, &setup.owner.token_b),
    )
}

fn state(setup: &Setup) -> PositionState {
    orca::position_state(&setup.svm, &setup.position)
}

#[test]
fn two_sided_fees_are_collected_and_compounded() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example, true);
    orca::swap(&mut setup.svm, &setup.pool, &setup.trader, 200 * SOL, true);
    orca::swap(
        &mut setup.svm,
        &setup.pool,
        &setup.trader,
        30_000 * USDC,
        false,
    );
    let before = state(&setup);
    // M2: the position does not show what it earned until something updates it.
    assert_eq!((before.fee_owed_a, before.fee_owed_b), (0, 0));
    let (owed_a, owed_b) = orca::fees_owed_now(&setup.svm, &setup.pool, &setup.position);
    let sqrt_price = orca::whirlpool(&setup.svm, &SOL_USDC).sqrt_price;
    let (lower, upper) = (setup.position.lower, setup.position.upper);
    let from_a =
        oq::increase_liquidity_quote_a(owed_a, 0, sqrt_price.into(), lower, upper, None, None)
            .unwrap();
    let from_b =
        oq::increase_liquidity_quote_b(owed_b, 0, sqrt_price.into(), lower, upper, None, None)
            .unwrap();
    let (a0, b0) = balances(&setup);

    let bounds = slippage_bounds(&setup, 100);
    let outcome =
        compound(&mut setup, example, 0, bounds).unwrap_or_else(|failure| panic!("{failure:?}"));

    let after = state(&setup);
    assert_eq!(
        (after.fee_owed_a, after.fee_owed_b),
        (0, 0),
        "the fees were collected"
    );
    // M4: the program buys the most liquidity both fees allow at the price it runs at.
    assert_eq!(
        after.liquidity - before.liquidity,
        from_a.liquidity_delta.min(from_b.liquidity_delta)
    );
    let (a1, b1) = balances(&setup);
    let (kept_a, kept_b) = (a1 - a0, b1 - b0);
    assert!(
        kept_a < owed_a && kept_b < owed_b,
        "some of each fee was deposited"
    );
    assert!(
        kept_a == 0 || kept_b == 0,
        "one fee was used whole: kept ({kept_a}, {kept_b})"
    );
    assert_eq!(orca::cpis_to(&outcome.logs, &WHIRLPOOL), 3);
    println!(
        "two-sided: {} CU, {} bytes; fees ({owed_a}, {owed_b}), kept ({kept_a}, {kept_b})",
        outcome.compute_units, outcome.size
    );
}

/// A position that earned in one token only: collected, not compounded, and the run lands.
fn one_sided(a_to_b: bool, label: &str) {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example, true);
    // Fees are paid in the token sold: A for A-to-B swaps, B for B-to-A.
    let amount = if a_to_b { 50 * SOL } else { 5_000 * USDC };
    orca::swap(&mut setup.svm, &setup.pool, &setup.trader, amount, a_to_b);
    let owed = orca::fees_owed_now(&setup.svm, &setup.pool, &setup.position);
    assert_eq!(owed.0 == 0, !a_to_b, "{owed:?}");
    assert_eq!(owed.1 == 0, a_to_b, "{owed:?}");
    let before = state(&setup);
    let (a0, b0) = balances(&setup);

    let bounds = slippage_bounds(&setup, 100);
    let outcome =
        compound(&mut setup, example, 0, bounds).unwrap_or_else(|failure| panic!("{failure:?}"));

    let (a1, b1) = balances(&setup);
    assert_eq!((a1 - a0, b1 - b0), owed, "the whole fee reached the wallet");
    let after = state(&setup);
    assert_eq!(after.liquidity, before.liquidity, "nothing was reinvested");
    assert_eq!((after.fee_owed_a, after.fee_owed_b), (0, 0));
    assert_eq!(
        orca::cpis_to(&outcome.logs, &WHIRLPOOL),
        2,
        "update and collect only"
    );
    println!("{label}: {} CU, fees {owed:?}", outcome.compute_units);
}

#[test]
fn fees_in_token_a_alone_are_collected_not_compounded() {
    one_sided(true, "A only");
}

#[test]
fn fees_in_token_b_alone_are_collected_not_compounded() {
    one_sided(false, "B only");
}

#[test]
fn no_fees_lands_without_collecting() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example, true);
    let before = (state(&setup), balances(&setup));

    let bounds = slippage_bounds(&setup, 100);
    let outcome =
        compound(&mut setup, example, 0, bounds).unwrap_or_else(|failure| panic!("{failure:?}"));

    assert_eq!((state(&setup), balances(&setup)), before);
    assert_eq!(
        orca::cpis_to(&outcome.logs, &WHIRLPOOL),
        1,
        "the update only"
    );
    println!("no fees: {} CU", outcome.compute_units);
}

#[test]
fn a_position_without_liquidity_lands() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example, false);

    let bounds = slippage_bounds(&setup, 100);
    let outcome =
        compound(&mut setup, example, 0, bounds).unwrap_or_else(|failure| panic!("{failure:?}"));

    assert_eq!(orca::cpis_to(&outcome.logs, &WHIRLPOOL), 0);
}

#[test]
fn a_price_move_inside_the_bounds_still_lands() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example, true);
    orca::swap(&mut setup.svm, &setup.pool, &setup.trader, 200 * SOL, true);
    orca::swap(
        &mut setup.svm,
        &setup.pool,
        &setup.trader,
        30_000 * USDC,
        false,
    );
    let signed_with = slippage_bounds(&setup, 100);
    // Someone trades between signing and landing.
    orca::swap(
        &mut setup.svm,
        &setup.pool,
        &setup.trader,
        1_000 * SOL,
        true,
    );
    let before = state(&setup);

    compound(&mut setup, example, 0, signed_with).unwrap_or_else(|failure| panic!("{failure:?}"));

    assert!(state(&setup).liquidity > before.liquidity);
}

#[test]
fn a_price_outside_the_bounds_fails_in_whirlpools() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example, true);
    orca::swap(&mut setup.svm, &setup.pool, &setup.trader, 200 * SOL, true);
    orca::swap(
        &mut setup.svm,
        &setup.pool,
        &setup.trader,
        30_000 * USDC,
        false,
    );
    let sqrt_price = orca::whirlpool(&setup.svm, &SOL_USDC).sqrt_price;
    let before = (state(&setup), balances(&setup));

    let failure = compound(&mut setup, example, 0, (sqrt_price + 1, sqrt_price + 2)).unwrap_err();

    assert_eq!(
        (failure.program, failure.code),
        (WHIRLPOOL, Some(6069)),
        "PriceSlippageOutOfBounds: {failure:?}"
    );
    assert_eq!(ballista_error(&failure), None);
    // The update and the collect reverted with it.
    assert_eq!((state(&setup), balances(&setup)), before);
}

#[test]
fn an_emptied_position_is_collected_not_refilled() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example, true);
    orca::swap(&mut setup.svm, &setup.pool, &setup.trader, 200 * SOL, true);
    orca::swap(
        &mut setup.svm,
        &setup.pool,
        &setup.trader,
        30_000 * USDC,
        false,
    );
    // Withdrawing records what the position earned.
    orca::withdraw_all(&mut setup.svm, &setup.pool, &setup.owner, &setup.position);
    let emptied = state(&setup);
    assert_eq!(emptied.liquidity, 0);
    assert!(emptied.fee_owed_a > 0 && emptied.fee_owed_b > 0);
    let (a0, b0) = balances(&setup);

    let bounds = slippage_bounds(&setup, 100);
    let outcome =
        compound(&mut setup, example, 0, bounds).unwrap_or_else(|failure| panic!("{failure:?}"));

    assert_eq!(
        state(&setup),
        PositionState {
            liquidity: 0,
            fee_owed_a: 0,
            fee_owed_b: 0
        }
    );
    let (a1, b1) = balances(&setup);
    assert_eq!((a1 - a0, b1 - b0), (emptied.fee_owed_a, emptied.fee_owed_b));
    assert_eq!(
        orca::cpis_to(&outcome.logs, &WHIRLPOOL),
        1,
        "the collect only"
    );
}
```

  The transitional run builder, for Steps 1–3:

```rust
/// Runs the template for the setup's position, signed by its owner.
///
/// Until Step 4 the template takes the liquidity to add rather than price bounds, so this works
/// the liquidity out the way a careful caller would: from the fees the position will show once the
/// template has updated it. `_bounds` is for Step 4's template.
fn compound(
    setup: &mut Setup,
    example: &Example,
    dust_floor: u64,
    _bounds: (u128, u128),
) -> Result<Outcome, Failure> {
    let Setup {
        pool,
        owner,
        position,
        ..
    } = &*setup;
    let (owed_a, owed_b) = orca::fees_owed_now(&setup.svm, pool, position);
    // One-sided fees fit no liquidity, and a caller does not ask for none.
    let liquidity = fitting_liquidity(&setup.svm, pool, position, owed_a, owed_b).max(1);
    let run = Run::new(setup.template, example)
        .account("whirlpoolProgram", WHIRLPOOL, false, false)
        .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
        .account("positionAuthority", owner.keypair.pubkey(), false, true)
        .account("whirlpool", pool.address, true, false)
        .account("position", position.address, true, false)
        .account("positionTokenAccount", position.token_account, false, false)
        .account("tokenOwnerAccountA", owner.token_a, true, false)
        .account("tokenOwnerAccountB", owner.token_b, true, false)
        .account("tokenVaultA", pool.vault_a, true, false)
        .account("tokenVaultB", pool.vault_b, true, false)
        .account(
            "tickArrayLower",
            orca::tick_array(pool, position.lower),
            true,
            false,
        )
        .account(
            "tickArrayUpper",
            orca::tick_array(pool, position.upper),
            true,
            false,
        )
        .input_u128("liquidityAmount", liquidity)
        .input_u64("dustFloor", dust_floor)
        .build();
    let payer = setup.owner.keypair.insecure_clone();
    tx::send(&mut setup.svm, &payer, &[], &[run], &[])
}

/// The most liquidity whose deposit, rounded up as the program rounds it, fits `(cap_a, cap_b)`
/// at the current price.
fn fitting_liquidity(
    svm: &LiteSVM,
    pool: &Pool,
    position: &Position,
    cap_a: u64,
    cap_b: u64,
) -> u128 {
    let sqrt_price = orca::whirlpool(svm, &pool.address).sqrt_price;
    let (lower, upper) = (position.lower, position.upper);
    let quote_a =
        oq::increase_liquidity_quote_a(cap_a, 0, sqrt_price.into(), lower, upper, None, None);
    let quote_b =
        oq::increase_liquidity_quote_b(cap_b, 0, sqrt_price.into(), lower, upper, None, None);
    let mut liquidity = quote_a
        .unwrap()
        .liquidity_delta
        .min(quote_b.unwrap().liquidity_delta);
    while liquidity > 0 {
        let quote = oq::increase_liquidity_quote(
            liquidity.into(),
            0,
            sqrt_price.into(),
            lower,
            upper,
            None,
            None,
        )
        .unwrap();
        if quote.token_est_a <= cap_a && quote.token_est_b <= cap_b {
            break;
        }
        liquidity -= 1;
    }
    liquidity
}
```

  Against the current template, 7 of 8 fail; `a_position_without_liquidity_lands` passes. The
  template reads fees that nothing has updated, so it sees none (M2):
  - `two_sided_fees_are_collected_and_compounded`: the liquidity rose by 0;
  - both `fees_in_token_*` tests: "the whole fee reached the wallet", `(0, 0)`;
  - `no_fees_lands_without_collecting`: 0 Whirlpool calls, not 1;
  - `a_price_move…` and `a_price_outside…`: the run landed as a no-op;
  - `an_emptied_position…`: `decrease_liquidity` did record its fees, so the template collects
    them and pours them straight back into the emptied position.

  Save the failure output for Task 10.
- [ ] **Step 2: M2: update before reading.**
  - In `orca-compound-fees.ts`, add `const position = account.fixed('position');`, and import
    `ORCA_UPDATE_FEES_AND_REWARDS` from `./shared.js`.
  - Insert this before the two `let`s, in place of their comment:

```ts
    step.invoke({
      program: account.fixed('whirlpoolProgram'),
      accounts: [
        { account: account.fixed('whirlpool'), signer: false, writable: true },
        { account: position, signer: false, writable: true },
        { account: account.fixed('tickArrayLower'), signer: false, writable: false },
        { account: account.fixed('tickArrayUpper'), signer: false, writable: false },
      ],
      data: [data.literal(ORCA_UPDATE_FEES_AND_REWARDS)],
      label: 'updateFees',
    }),
    // Read after the update, which makes them current, and before the collect, which zeroes them.
```

  - Add to `shared.ts`, beside `ORCA_COLLECT_FEES`:

```ts
/**
 * `update_fees_and_rewards()`: folds the pool's fee growth into a position's `fee_owed_*`. Needs
 * no signature; fails with `LiquidityZero` (6012) on a position without liquidity.
 */
export const ORCA_UPDATE_FEES_AND_REWARDS = anchorDiscriminator('update_fees_and_rewards');
```

  - Expect `a_position_without_liquidity_lands` to fail:
    `whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc failed with code 6012`, Whirlpool's
    `LiquidityZero`. Read as Ballista's, 6012 would be `TypeMismatch`.
  - Then skip the update without liquidity. Put this before the update, and add
    `when: expression.variable('hasLiquidity')` to the update:

```ts
    step.let(
      'hasLiquidity',
      expression.greaterThan(
        expression.accountData(position, ORCA_POSITION.liquidity, 'u128'),
        expression.u128(0),
      ),
      'readLiquidity',
    ),
```

  - Expect these to pass: `two_sided`, `no_fees`, `a_position_without_liquidity_lands` and
    `a_price_move_inside_the_bounds_still_lands`. The last passes only because the transitional
    builder works out the liquidity after the move; Step 4 makes it mean what it says.
  - Expect these to fail:
    - `fees_in_token_a_alone…` in Whirlpools with 6017, `TokenMaxExceeded`, which Ballista
      would read as `InvalidPdaDerivation`. `increase_liquidity` cannot take liquidity with a
      zero cap on B, and the collect reverts with it. That is M4;
    - `fees_in_token_b_alone…` with "the whole fee reached the wallet", `(0, 0)`: the guard
      reads only A. That is M3;
    - `a_price_outside_the_bounds…` and `an_emptied_position…`: no bounds, and the emptied
      position is refilled.
- [ ] **Step 3: M3: collect when either fee is above the floor.** After the two reads:

```ts
    step.let(
      'earnedA',
      expression.greaterThan(expression.variable('owedA'), expression.input('dustFloor')),
    ),
    step.let(
      'earnedB',
      expression.greaterThan(expression.variable('owedB'), expression.input('dustFloor')),
    ),
```

  - Give `collectFees` the guard `when: expression.or(expression.variable('earnedA'),
    expression.variable('earnedB'))`.
  - Leave `compoundFees`'s guard on A for now.
  - Expect `fees_in_token_b_alone…` to pass. `fees_in_token_a_alone…` still fails with 6017.
- [ ] **Step 4: M4: reinvest by token amounts, only when both fees are above the floor and the
  position has liquidity.**
  - Put the file's own `compound` back, deleting the transitional one and `fitting_liquidity`.
  - Replace the template with the version below.
  - In `shared.ts`, add these, and delete `ORCA_INCREASE_LIQUIDITY`, which nothing uses any
    more. Put `MEMO_PROGRAM` with the programs, the rest with the instructions:

```ts
/** SPL Memo. Orca's v2 instructions take it, for Token-2022 transfers that require a memo. */
export const MEMO_PROGRAM = 'MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr' as const;
```

```ts
/**
 * `increase_liquidity_by_token_amounts_v2(method: IncreaseLiquidityMethod, remaining_accounts_info:
 * Option<RemainingAccountsInfo>)`. It takes `increase_liquidity_v2`'s accounts: whirlpool,
 * token_program_a, token_program_b, memo_program, position_authority, position,
 * position_token_account, token_mint_a, token_mint_b, token_owner_account_a,
 * token_owner_account_b, token_vault_a, token_vault_b, tick_array_lower, tick_array_upper.
 */
export const ORCA_INCREASE_LIQUIDITY_BY_TOKEN_AMOUNTS_V2 = anchorDiscriminator(
  'increase_liquidity_by_token_amounts_v2',
);
/**
 * `IncreaseLiquidityMethod::ByTokenAmounts { token_max_a: u64, token_max_b: u64, min_sqrt_price:
 * u128, max_sqrt_price: u128 }`, the enum's only variant, as its one-byte Borsh tag.
 */
export const ORCA_BY_TOKEN_AMOUNTS = Uint8Array.of(0);
```

```ts
/**
 * Collect an Orca Whirlpool position's fees and reinvest them in the position.
 *
 * A position's `fee_owed_a` and `fee_owed_b` hold what its last update recorded, not what it has
 * earned since. Swaps raise the pool's fee growth, and `update_fees_and_rewards` is what folds that
 * growth into the position. Orca's own SDK calls it before every collect, and so does this
 * template, before it reads the fees. The update needs no signature. It fails with `LiquidityZero`
 * (6012) on a position without liquidity, which earns nothing, so it is skipped for one.
 *
 * The reinvestment is `increase_liquidity_by_token_amounts_v2`, as in Orca's SDK. Given the two
 * fees as caps, the program works out the most liquidity they buy at the price when the block
 * runs, so nothing is topped up from the wallet and a price move within the bounds below does not
 * matter. One fee is used up and part of the other stays in the wallet. `increase_liquidity`, by
 * contrast, takes a liquidity chosen at signing and fails with `TokenMaxExceeded` (6017) once the
 * price has moved or the fees are in one token.
 *
 * While the price is inside the position's range, liquidity takes both tokens: with either cap at
 * zero the program works out none and fails with `LiquidityZero`. So fees in one token are
 * collected and not reinvested. Nor are the fees of a position emptied with `decrease_liquidity`:
 * they are collected, and the position stays empty.
 *
 * `minSqrtPrice` and `maxSqrtPrice` bound the pool price the deposit accepts. Orca's
 * `get_sqrt_price_slippage_bounds` computes them from a price and a tolerance. Outside them the
 * deposit fails with `PriceSlippageOutOfBounds` (6069), and the whole run reverts with it.
 *
 * `collect_fees` and the pinned token program are SPL Token's, so both of the pool's mints must be
 * SPL Token mints, as SOL and USDC are.
 *
 * With nothing above `dustFloor` in either token, the collect and the deposit are skipped and the
 * run lands: a scheduled compounder that finds nothing to do should not revert and burn the fee.
 *
 * Offsets come from `Position`, declared as `whirlpool, position_mint, liquidity,
 * tick_lower_index, tick_upper_index, fee_growth_checkpoint_a, fee_owed_a,
 * fee_growth_checkpoint_b, fee_owed_b, reward_infos` with `LEN = 8 + 136 + 72`.
 */
import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  data,
  defineTemplate,
  expression,
  step,
} from '../../src/index.js';
import {
  MEMO_PROGRAM,
  OPTION_NONE,
  ORCA_BY_TOKEN_AMOUNTS,
  ORCA_COLLECT_FEES,
  ORCA_INCREASE_LIQUIDITY_BY_TOKEN_AMOUNTS_V2,
  ORCA_POSITION,
  ORCA_UPDATE_FEES_AND_REWARDS,
  ORCA_WHIRLPOOL,
  addressBytes,
} from './shared.js';

const position = account.fixed('position');

export const orcaCompoundFees = defineTemplate({
  inputs: {
    /** Fees at or below this, in either token's base units, are not worth collecting. */
    dustFloor: { type: 'u64' },
    /** The lowest pool sqrt price (Q64.64) the deposit accepts. */
    minSqrtPrice: { type: 'u128' },
    /** The highest pool sqrt price (Q64.64) the deposit accepts. */
    maxSqrtPrice: { type: 'u128' },
  },
  accounts: {
    whirlpoolProgram: { executable: true, address: addressBytes(ORCA_WHIRLPOOL) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    memoProgram: { executable: true, address: addressBytes(MEMO_PROGRAM) },
    positionAuthority: { signer: true },
    whirlpool: { writable: true },
    /** Owner-pinned so `liquidity` and the owed fees are read from a real Whirlpool position. */
    position: {
      writable: true,
      owner: addressBytes(ORCA_WHIRLPOOL),
      minDataLength: ORCA_POSITION.length,
    },
    positionTokenAccount: {},
    tokenMintA: {},
    tokenMintB: {},
    tokenOwnerAccountA: { writable: true },
    tokenOwnerAccountB: { writable: true },
    tokenVaultA: { writable: true },
    tokenVaultB: { writable: true },
    tickArrayLower: { writable: true },
    tickArrayUpper: { writable: true },
  },
  steps: [
    step.let(
      'hasLiquidity',
      expression.greaterThan(
        expression.accountData(position, ORCA_POSITION.liquidity, 'u128'),
        expression.u128(0),
      ),
      'readLiquidity',
    ),
    step.invoke({
      program: account.fixed('whirlpoolProgram'),
      accounts: [
        { account: account.fixed('whirlpool'), signer: false, writable: true },
        { account: position, signer: false, writable: true },
        { account: account.fixed('tickArrayLower'), signer: false, writable: false },
        { account: account.fixed('tickArrayUpper'), signer: false, writable: false },
      ],
      data: [data.literal(ORCA_UPDATE_FEES_AND_REWARDS)],
      when: expression.variable('hasLiquidity'),
      label: 'updateFees',
    }),
    // Read after the update, which makes them current, and before the collect, which zeroes them.
    step.let('owedA', expression.accountData(position, ORCA_POSITION.feeOwedA, 'u64'), 'readFeesOwedA'),
    step.let('owedB', expression.accountData(position, ORCA_POSITION.feeOwedB, 'u64'), 'readFeesOwedB'),
    step.let(
      'earnedA',
      expression.greaterThan(expression.variable('owedA'), expression.input('dustFloor')),
    ),
    step.let(
      'earnedB',
      expression.greaterThan(expression.variable('owedB'), expression.input('dustFloor')),
    ),
    step.invoke({
      program: account.fixed('whirlpoolProgram'),
      accounts: [
        { account: account.fixed('whirlpool'), signer: false, writable: false },
        { account: account.fixed('positionAuthority'), signer: true, writable: false },
        { account: position, signer: false, writable: true },
        { account: account.fixed('positionTokenAccount'), signer: false, writable: false },
        { account: account.fixed('tokenOwnerAccountA'), signer: false, writable: true },
        { account: account.fixed('tokenVaultA'), signer: false, writable: true },
        { account: account.fixed('tokenOwnerAccountB'), signer: false, writable: true },
        { account: account.fixed('tokenVaultB'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
      ],
      data: [data.literal(ORCA_COLLECT_FEES)],
      // Either fee is worth collecting.
      when: expression.or(expression.variable('earnedA'), expression.variable('earnedB')),
      label: 'collectFees',
    }),
    step.invoke({
      program: account.fixed('whirlpoolProgram'),
      accounts: [
        { account: account.fixed('whirlpool'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('memoProgram'), signer: false, writable: false },
        { account: account.fixed('positionAuthority'), signer: true, writable: false },
        { account: position, signer: false, writable: true },
        { account: account.fixed('positionTokenAccount'), signer: false, writable: false },
        { account: account.fixed('tokenMintA'), signer: false, writable: false },
        { account: account.fixed('tokenMintB'), signer: false, writable: false },
        { account: account.fixed('tokenOwnerAccountA'), signer: false, writable: true },
        { account: account.fixed('tokenOwnerAccountB'), signer: false, writable: true },
        { account: account.fixed('tokenVaultA'), signer: false, writable: true },
        { account: account.fixed('tokenVaultB'), signer: false, writable: true },
        { account: account.fixed('tickArrayLower'), signer: false, writable: true },
        { account: account.fixed('tickArrayUpper'), signer: false, writable: true },
      ],
      data: [
        data.literal(ORCA_INCREASE_LIQUIDITY_BY_TOKEN_AMOUNTS_V2),
        data.literal(ORCA_BY_TOKEN_AMOUNTS),
        data.encode('u64', expression.variable('owedA')),
        data.encode('u64', expression.variable('owedB')),
        data.encode('u128', expression.input('minSqrtPrice')),
        data.encode('u128', expression.input('maxSqrtPrice')),
        data.literal(OPTION_NONE),
      ],
      // In range, liquidity needs both tokens; an emptied position stays empty.
      when: expression.and(
        expression.variable('hasLiquidity'),
        expression.and(expression.variable('earnedA'), expression.variable('earnedB')),
      ),
      label: 'compoundFees',
    }),
  ],
});

export const compiled = compileTemplate(orcaCompoundFees);
```

  - All 8 pass. The run's data is 58 bytes: the discriminator, the tag, two caps, two bounds and
    `None`.
  - In `two_sided…`, the liquidity added equals the smaller of Orca's two quotes, exactly. The
    B fee is partly left in the wallet: 264,429 of 2,086,987 units in the planning run.
- [ ] **Step 5:** Run `pnpm --dir clients/js check` and
  `cargo test --manifest-path tests/protocols/Cargo.toml`. Commit: "Update an Orca position's fees
  before compounding them, and reinvest by token amounts".

### Task 7: `orcaHarvestManyPositions`: update each row, collect either fee

- [ ] **Step 1: The row grows to four accounts. Test the runner first.**
  - In `clients/js/src/protocol-semantics.test.ts`:
    - add `AccountRole` and `address` to the `@solana/kit` import;
    - import `buildOrcaHarvestRun` and `getOrcaTickArrayAddress` from
      `'../examples/protocols/run-orca-harvest.js'`;
    - add:

```ts
describe('the Orca harvest runner', () => {
  const decoder = getAddressDecoder();
  const key = (byte: number): Address => decoder.decode(new Uint8Array(32).fill(byte));
  const accounts = {
    positionAuthority: key(2),
    whirlpool: key(3),
    tokenOwnerAccountA: key(4),
    tokenOwnerAccountB: key(5),
    tokenVaultA: key(6),
    tokenVaultB: key(7),
  };

  test('passes each row as the position, its NFT account and its two tick arrays', async () => {
    const instruction = await buildOrcaHarvestRun({
      creator: key(1),
      templateId: 0,
      accounts,
      positions: [
        { position: key(10), positionTokenAccount: key(11), tickArrayLower: key(12), tickArrayUpper: key(13) },
        { position: key(20), positionTokenAccount: key(21), tickArrayLower: key(12), tickArrayUpper: key(13) },
      ],
      dustFloor: 5n,
    });
    const metas = (instruction.accounts ?? []).map((meta) => [meta.address, meta.role]);
    // The template, then eight fixed accounts. Each row's update writes the pool.
    expect(metas[4]).toEqual([key(3), AccountRole.WRITABLE]);
    expect(metas.slice(9)).toEqual([
      [key(10), AccountRole.WRITABLE],
      [key(11), AccountRole.READONLY],
      [key(12), AccountRole.READONLY],
      [key(13), AccountRole.READONLY],
      [key(20), AccountRole.WRITABLE],
      [key(21), AccountRole.READONLY],
      [key(12), AccountRole.READONLY],
      [key(13), AccountRole.READONLY],
    ]);
  });

  test('finds the tick array holding a tick, as mainnet derives it', async () => {
    const solUsdc = address('Czfq3xZZDmsdGdUyrNLtRhGc47cXcZtLG4crryfu44zE');
    expect(await getOrcaTickArrayAddress(solUsdc, -20_980, 4)).toBe('FdtvWk8j5u1a64YK2Uxk9eXxKZJTwLHDGx8aJPbJyw2Q');
    // An array starts at a multiple of 88 tick spacings; the tick below it is in the previous one.
    expect(await getOrcaTickArrayAddress(solUsdc, -21_120, 4)).toBe('FdtvWk8j5u1a64YK2Uxk9eXxKZJTwLHDGx8aJPbJyw2Q');
    expect(await getOrcaTickArrayAddress(solUsdc, -21_121, 4)).toBe('6hA1LN1fzCiXqymDiQXeBFn5da1b7STP1L7JmDc6hR3M');
    const thin = address('HJPjoWUrhoZzkNfRpHuieeFk9WcZWjwy6PBjZ81ngndJ');
    expect(await getOrcaTickArrayAddress(thin, -20_989, 64)).toBe('CEstjhG1v4nUgvGDyFruYEbJ18X8XeN4sX1WFCLt4D5c');
  });
});
```

  - Run `pnpm --dir clients/js exec vitest run src/protocol-semantics.test.ts`. Both tests fail:
    the runner passes two accounts per row, and has no `getOrcaTickArrayAddress`.
  - Then grow the row, in three places. It stays within Ballista's limits, which the compiler
    checks:
    - a stride of 4 row accounts, where the limit is 8;
    - 8 fixed accounts plus 4 × 12 rows = 56 runtime accounts, where the limit is 120;
    - after Step 3, 2 calls × 12 rows = 24 expanded CPIs, where the limit is 64.

    The final template compiles to 478 bytes, 9 registers and 12 instructions.
    - **The template.** Add to the row, after `positionTokenAccount`:

```ts
      /** The tick array holding the position's lower tick; `update_fees_and_rewards` reads it. */
      tickArrayLower: {},
      /** The tick array holding the position's upper tick. */
      tickArrayUpper: {},
```

    - **`run-orca-harvest.ts`.** `HarvestRow` gains `tickArrayLower` and `tickArrayUpper`,
      `batchRows` passes them, and `getOrcaTickArrayAddress` is new. `describeFailure` is
      unchanged until Task 8:

```ts
/**
 * Build the Orca harvest run: the other run-side shape, batch rows.
 *
 * `orca-harvest-many-positions.ts` declares a row of four accounts and up to twelve iterations.
 * The caller passes one record per position and the iteration count follows from how many were
 * passed — there is no count in the instruction data to get wrong.
 *
 * Every other example on this page binds accounts by name and needs nothing beyond
 * `buildKitRunInstruction`; this one and `run-jupiter-deposit.ts` are the two that do not.
 */
import {
  address,
  getAddressEncoder,
  getProgramDerivedAddress,
  type Address,
  type Instruction,
} from '@solana/kit';

import { explainRunError } from '../../src/index.js';
import { BALLISTA_ADDRESS, buildKitRunInstruction, getTemplateAddress } from '../../src/kit.js';
import { compiled } from './orca-harvest-many-positions.js';
import { ORCA_WHIRLPOOL } from './shared.js';

/** One position, the token account holding its NFT, and the tick arrays holding its two bounds. */
export interface HarvestRow {
  position: Address;
  positionTokenAccount: Address;
  /** The tick array holding the position's lower tick: `getOrcaTickArrayAddress`. */
  tickArrayLower: Address;
  /** The tick array holding the position's upper tick. */
  tickArrayUpper: Address;
}

export interface HarvestAccounts {
  positionAuthority: Address;
  whirlpool: Address;
  tokenOwnerAccountA: Address;
  tokenOwnerAccountB: Address;
  tokenVaultA: Address;
  tokenVaultB: Address;
}

/** The template's declared ceiling; more positions than this need a second run. */
export const MAX_POSITIONS_PER_RUN = 12;

/** Ticks in one Whirlpool tick array. */
const TICKS_PER_ARRAY = 88;

/**
 * The tick array holding `tickIndex` in a pool whose tick spacing is `tickSpacing`: the PDA
 * `["tick_array", whirlpool, start]`, where `start` is the array's first tick as a decimal string.
 */
export async function getOrcaTickArrayAddress(
  whirlpool: Address,
  tickIndex: number,
  tickSpacing: number,
): Promise<Address> {
  const span = TICKS_PER_ARRAY * tickSpacing;
  const start = Math.floor(tickIndex / span) * span;
  const [tickArray] = await getProgramDerivedAddress({
    programAddress: address(ORCA_WHIRLPOOL),
    seeds: ['tick_array', getAddressEncoder().encode(whirlpool), String(start)],
  });
  return tickArray;
}

export async function buildOrcaHarvestRun(input: {
  creator: Address;
  templateId: number;
  accounts: HarvestAccounts;
  positions: readonly HarvestRow[];
  dustFloor: bigint;
}): Promise<Instruction> {
  if (input.positions.length === 0) {
    throw new Error('The template declares minIterations 1; pass at least one position');
  }
  if (input.positions.length > MAX_POSITIONS_PER_RUN) {
    throw new Error(
      `${input.positions.length} positions exceeds the template's ${MAX_POSITIONS_PER_RUN}; split the run`,
    );
  }

  const [templateAddress] = await getTemplateAddress(input.creator, input.templateId);
  return buildKitRunInstruction({
    compiled,
    programAddress: BALLISTA_ADDRESS,
    templateAddress,
    inputs: { dustFloor: input.dustFloor },
    accounts: {
      whirlpoolProgram: { address: address(ORCA_WHIRLPOOL) },
      tokenProgram: { address: address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA') },
      positionAuthority: { address: input.accounts.positionAuthority },
      whirlpool: { address: input.accounts.whirlpool },
      tokenOwnerAccountA: { address: input.accounts.tokenOwnerAccountA },
      tokenOwnerAccountB: { address: input.accounts.tokenOwnerAccountB },
      tokenVaultA: { address: input.accounts.tokenVaultA },
      tokenVaultB: { address: input.accounts.tokenVaultB },
    },
    // One record per row, in order. The run's iteration count is derived from the account list.
    batchRows: input.positions.map((row) => ({
      position: { address: row.position },
      positionTokenAccount: { address: row.positionTokenAccount },
      tickArrayLower: { address: row.tickArrayLower },
      tickArrayUpper: { address: row.tickArrayUpper },
    })),
  });
}

/**
 * A harvest cannot fail because a position had earned nothing — that row is skipped. A failure
 * here is a real one: a position that is not owned by Whirlpools, or a wrong vault.
 */
export function describeFailure(code: number): string {
  const explanation = explainRunError(code, compiled);
  return explanation ? explanation.message : `code ${code} came from Whirlpools, not Ballista`;
}
```

    - **`clients/rust/examples/protocol_runs.rs`.** Replace the `#rows` region and update
      `main`:

```rust
// #region rows
/// Shape three: batch rows. The iteration count comes from how many rows are passed, so there is
/// no count in the instruction data to get wrong.
///
/// This is `orca-harvest-many-positions`. Its row is a position, the token account holding the
/// position's NFT, and the tick arrays holding the position's lower and upper ticks.
pub struct HarvestRow {
    pub position: Pubkey,
    pub position_token_account: Pubkey,
    pub tick_array_lower: Pubkey,
    pub tick_array_upper: Pubkey,
}

pub fn run_orca_harvest(
    template: Pubkey,
    authority: Pubkey,
    whirlpool: Pubkey,
    owner_accounts: (Pubkey, Pubkey),
    vaults: (Pubkey, Pubkey),
    rows: &[HarvestRow],
    dust_floor: u64,
) -> Instruction {
    assert!(!rows.is_empty(), "the template declares minIterations 1");
    assert!(rows.len() <= 12, "the template declares maxIterations 12");

    let inputs = RunInputs::new().u64(dust_floor).finish();
    let mut accounts = vec![
        AccountMeta::new_readonly(ORCA_WHIRLPOOL, false),
        AccountMeta::new_readonly(TOKEN_PROGRAM, false),
        AccountMeta::new_readonly(authority, true),
        // Each row's update_fees_and_rewards writes the pool.
        AccountMeta::new(whirlpool, false),
        AccountMeta::new(owner_accounts.0, false),
        AccountMeta::new(owner_accounts.1, false),
        AccountMeta::new(vaults.0, false),
        AccountMeta::new(vaults.1, false),
    ];
    // One row after another, each in the order the row schema declares.
    for row in rows {
        accounts.push(AccountMeta::new(row.position, false));
        accounts.push(AccountMeta::new_readonly(row.position_token_account, false));
        accounts.push(AccountMeta::new_readonly(row.tick_array_lower, false));
        accounts.push(AccountMeta::new_readonly(row.tick_array_upper, false));
    }
    run_instruction(template, accounts, &inputs)
}
// #endregion rows
```

```rust
    let rows: Vec<HarvestRow> = (0..5)
        .map(|_| HarvestRow {
            position: key(),
            position_token_account: key(),
            tick_array_lower: key(),
            tick_array_upper: key(),
        })
        .collect();
    let harvest = run_orca_harvest(
        template,
        key(),
        key(),
        (key(), key()),
        (key(), key()),
        &rows,
        10_000,
    );
    println!("orca harvest      {} accounts, {} rows", harvest.accounts.len(), rows.len());
```

  - Then run `pnpm fixtures`. These pass:
    - `pnpm --dir clients/js check`;
    - `cargo run -p ballista-sdk --example protocol_runs`, which prints 29 accounts for 5 rows;
    - `cargo test --manifest-path tests/protocols/Cargo.toml --lib`, including milestone 1's
      `batch_rows_follow_the_fixed_accounts`.
  - The docs page includes the `#rows` region, so it shows the new row without an edit to
    `docs/`.
- [ ] **Step 2: The tests,** `tests/protocols/tests/orca_harvest_many_positions.rs`.
  - The four rows are on `HJPj…`, all in one tick array:
    - `both`: earning A and B;
    - `only_b`: earning B only. It is opened between the SOL sale and the USDC sale, and held as
      a Token-2022 NFT, so both NFT programs are covered;
    - `out_of_range`: above the price;
    - `empty`: without liquidity.

```rust
//! `orcaHarvestManyPositions` against the real Whirlpool program: positions that earned in both
//! tokens, in one token, and not at all, and one without liquidity, harvested in one run.

use {
    ballista_protocol_tests::{
        orca::{self, Nft, Pool, Position, TokenWallet, SOL_USDC, SOL_USDC_THIN, USDC, WHIRLPOOL},
        template::{examples, upload, Example, Run},
        tx::{self, ballista_error, Failure, Outcome},
        wallet::{fund, keypair, token_balance, SOL},
    },
    ballista_sdk::{decode_ballista_error, TOKEN_PROGRAM_ID},
    litesvm::LiteSVM,
    solana_address::Address,
    solana_compute_budget_interface::ComputeBudgetInstruction,
    solana_instruction::Instruction,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const EXAMPLE: &str = "orcaHarvestManyPositions";

/// The thin pool, an owner, a trader and the template. Rows share one tick array there.
struct Setup {
    svm: LiteSVM,
    pool: Pool,
    /// The pool's in-range liquidity as snapshotted.
    pool_liquidity: u128,
    owner: TokenWallet,
    trader: TokenWallet,
    template: Address,
}

fn setup(example: &Example) -> Setup {
    let mut svm = orca::svm();
    let pool = orca::pool(&svm, SOL_USDC_THIN);
    let owner = orca::token_wallet(
        &mut svm,
        &orca::seed("harvest owner"),
        10_000 * SOL,
        2_000_000 * USDC,
    );
    let trader = orca::token_wallet(
        &mut svm,
        &orca::seed("harvest trader"),
        100_000 * SOL,
        20_000_000 * USDC,
    );
    let creator = keypair(b"ballista-protocol-tests-creator1");
    fund(&mut svm, &creator.pubkey(), 10 * SOL);
    let template = upload(&mut svm, &creator, 1, &example.payload);
    let pool_liquidity = orca::whirlpool(&svm, &SOL_USDC_THIN).liquidity;
    Setup {
        svm,
        pool,
        pool_liquidity,
        owner,
        trader,
        template,
    }
}

impl Setup {
    /// A position of the owner's on `range`, holding `liquidity`.
    fn position(&mut self, label: &str, range: (i32, i32), nft: Nft, liquidity: u128) -> Position {
        let mint = keypair(&orca::seed(label));
        let position =
            orca::open_position(&mut self.svm, &self.pool, &self.owner, &mint, range, nft);
        if liquidity > 0 {
            orca::deposit(&mut self.svm, &self.pool, &self.owner, &position, liquidity);
        }
        position
    }

    fn balances(&self) -> (u64, u64) {
        (
            token_balance(&self.svm, &self.owner.token_a),
            token_balance(&self.svm, &self.owner.token_b),
        )
    }
}

/// The harvest over `rows`, each paired with the pool it belongs to, signed by `authority`.
fn harvest(
    setup: &Setup,
    example: &Example,
    authority: &Keypair,
    rows: &[(&Position, &Pool)],
    dust_floor: u64,
) -> Instruction {
    let mut run = Run::new(setup.template, example)
        .account("whirlpoolProgram", WHIRLPOOL, false, false)
        .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
        .account("positionAuthority", authority.pubkey(), false, true)
        .account("whirlpool", setup.pool.address, true, false)
        .account("tokenOwnerAccountA", setup.owner.token_a, true, false)
        .account("tokenOwnerAccountB", setup.owner.token_b, true, false)
        .account("tokenVaultA", setup.pool.vault_a, true, false)
        .account("tokenVaultB", setup.pool.vault_b, true, false)
        .input_u64("dustFloor", dust_floor);
    for (position, pool) in rows {
        run = run.row(|row| {
            row.account("position", position.address, true, false)
                .account("positionTokenAccount", position.token_account, false, false)
                .account(
                    "tickArrayLower",
                    orca::tick_array(pool, position.lower),
                    false,
                    false,
                )
                .account(
                    "tickArrayUpper",
                    orca::tick_array(pool, position.upper),
                    false,
                    false,
                )
        });
    }
    run.build()
}

fn send(
    setup: &mut Setup,
    signer: &Keypair,
    instructions: &[Instruction],
) -> Result<Outcome, Failure> {
    tx::send(&mut setup.svm, signer, &[], instructions, &[])
}

/// Two earning rows (one in both tokens, one only in B and held as a Token-2022 NFT), one out of
/// range, one without liquidity.
struct Rows {
    both: Position,
    only_b: Position,
    out_of_range: Position,
    empty: Position,
}

fn four_rows(setup: &mut Setup) -> Rows {
    let spacing = i32::from(setup.pool.tick_spacing);
    let around = orca::range(&setup.svm, &setup.pool, -10 * spacing, 10 * spacing);
    let above = orca::range(&setup.svm, &setup.pool, 20 * spacing, 30 * spacing);
    // Ten times the pool's own liquidity, so these rows earn most of each fee.
    let liquidity = 10 * setup.pool_liquidity;
    let both = setup.position("harvest both", around, Nft::Token, liquidity);
    let out_of_range = setup.position("harvest above", above, Nft::Token, liquidity);
    let empty = setup.position("harvest empty", around, Nft::Token, 0);
    // Sold SOL pays fees in token A to the positions in range now.
    orca::swap(&mut setup.svm, &setup.pool, &setup.trader, 20 * SOL, true);
    let only_b = setup.position("harvest only b", around, Nft::Token2022, liquidity);
    // Sold USDC pays fees in token B, to `only_b` too.
    orca::swap(
        &mut setup.svm,
        &setup.pool,
        &setup.trader,
        2_000 * USDC,
        false,
    );
    Rows {
        both,
        only_b,
        out_of_range,
        empty,
    }
}

#[test]
fn only_the_rows_that_earned_collect() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let Rows {
        both,
        only_b,
        out_of_range,
        empty,
    } = four_rows(&mut setup);
    let earned_both = orca::fees_owed_now(&setup.svm, &setup.pool, &both);
    let earned_b = orca::fees_owed_now(&setup.svm, &setup.pool, &only_b);
    assert!(earned_both.0 > 0 && earned_both.1 > 0, "{earned_both:?}");
    assert!(earned_b.0 == 0 && earned_b.1 > 0, "{earned_b:?}");
    assert_eq!(
        orca::fees_owed_now(&setup.svm, &setup.pool, &out_of_range),
        (0, 0)
    );
    let untouched =
        [out_of_range, empty].map(|position| orca::position_state(&setup.svm, &position));
    let before = setup.balances();

    let pool = setup.pool;
    let owner = setup.owner.keypair.insecure_clone();
    let run = harvest(
        &setup,
        example,
        &owner,
        &[
            (&both, &pool),
            (&only_b, &pool),
            (&out_of_range, &pool),
            (&empty, &pool),
        ],
        0,
    );
    let outcome = send(&mut setup, &owner, &[run]).unwrap_or_else(|failure| panic!("{failure:?}"));

    let after = setup.balances();
    assert_eq!(
        (after.0 - before.0, after.1 - before.1),
        (earned_both.0 + earned_b.0, earned_both.1 + earned_b.1)
    );
    for position in [both, only_b] {
        let state = orca::position_state(&setup.svm, &position);
        assert_eq!((state.fee_owed_a, state.fee_owed_b), (0, 0));
    }
    assert_eq!(
        [out_of_range, empty].map(|position| orca::position_state(&setup.svm, &position)),
        untouched
    );
    // Three updates (not the empty row) and two collects.
    assert_eq!(orca::cpis_to(&outcome.logs, &WHIRLPOOL), 5);
    println!(
        "four rows: {} CU, {} bytes",
        outcome.compute_units, outcome.size
    );
}

/// M6: Whirlpools numbers its errors from 6000, as Ballista does, so only the logs say who refused.
#[test]
fn a_stranger_cannot_collect_and_whirlpools_says_so() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let Rows { both, .. } = four_rows(&mut setup);
    let stranger = keypair(&orca::seed("harvest stranger"));
    fund(&mut setup.svm, &stranger.pubkey(), SOL);

    let pool = setup.pool;
    let run = harvest(&setup, example, &stranger, &[(&both, &pool)], 0);
    let failure = send(&mut setup, &stranger, &[run]).unwrap_err();

    assert_eq!(
        (failure.program, failure.code),
        (WHIRLPOOL, Some(6019)),
        "MissingOrInvalidDelegate: {failure:?}"
    );
    assert_eq!(ballista_error(&failure), None);
    // The same number, read as Ballista's, names an unrelated failure.
    assert_eq!(
        decode_ballista_error(6019).map(|decoded| decoded.name),
        Some("ReturnDataMismatch")
    );
}

/// M5: skipping rows does not keep a harvest from reverting; a row Whirlpools refuses reverts all.
#[test]
fn a_row_from_another_pool_reverts_the_whole_harvest() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let Rows { both, .. } = four_rows(&mut setup);
    let other_pool = orca::pool(&setup.svm, SOL_USDC);
    let mint = keypair(&orca::seed("harvest foreign"));
    let range = orca::range(&setup.svm, &other_pool, -40, 40);
    let foreign = orca::open_position(
        &mut setup.svm,
        &other_pool,
        &setup.owner,
        &mint,
        range,
        Nft::Token,
    );
    orca::deposit(
        &mut setup.svm,
        &other_pool,
        &setup.owner,
        &foreign,
        1_000_000_000,
    );
    let before = setup.balances();

    let pool = setup.pool;
    let owner = setup.owner.keypair.insecure_clone();
    let run = harvest(
        &setup,
        example,
        &owner,
        &[(&both, &pool), (&foreign, &other_pool)],
        0,
    );
    let failure = send(&mut setup, &owner, &[run]).unwrap_err();

    // Anchor's ConstraintHasOne: the position's whirlpool is not the run's.
    assert_eq!(
        (failure.program, failure.code),
        (WHIRLPOOL, Some(2001)),
        "{failure:?}"
    );
    assert_eq!(
        setup.balances(),
        before,
        "the first row's collect reverted too"
    );
}

/// Ten earning rows fit one legacy transaction, with a compute budget; eleven do not.
#[test]
fn ten_rows_fit_one_legacy_transaction() {
    let examples = examples();
    let example = &examples[EXAMPLE];
    let mut setup = setup(example);
    let spacing = i32::from(setup.pool.tick_spacing);
    let around = orca::range(&setup.svm, &setup.pool, -10 * spacing, 10 * spacing);
    let liquidity = setup.pool_liquidity;
    let positions: Vec<Position> = (0..11)
        .map(|index| {
            setup.position(
                &format!("harvest many {index}"),
                around,
                Nft::Token,
                liquidity,
            )
        })
        .collect();
    orca::swap(&mut setup.svm, &setup.pool, &setup.trader, 20 * SOL, true);
    orca::swap(
        &mut setup.svm,
        &setup.pool,
        &setup.trader,
        2_000 * USDC,
        false,
    );
    let pool = setup.pool;
    let owner = setup.owner.keypair.insecure_clone();
    let rows: Vec<(&Position, &Pool)> =
        positions.iter().map(|position| (position, &pool)).collect();
    let budget = ComputeBudgetInstruction::set_compute_unit_limit(400_000);

    let eleven = harvest(&setup, example, &owner, &rows, 0);
    let transaction = tx::transaction(&setup.svm, &owner, &[], &[budget.clone(), eleven], &[]);
    assert!(
        tx::wire_size(&transaction) > tx::PACKET_DATA_SIZE,
        "eleven rows now fit: update the header"
    );

    let ten = harvest(&setup, example, &owner, &rows[..10], 0);
    let outcome =
        send(&mut setup, &owner, &[budget, ten]).unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(orca::cpis_to(&outcome.logs, &WHIRLPOOL), 20);
    println!(
        "ten rows: {} CU, {} bytes",
        outcome.compute_units, outcome.size
    );
}
```

  - All four fail against the template: nothing was updated, so nothing collects (M2).
    - `only_the_rows…` receives `(0, 0)`;
    - the stranger's run and the foreign row's run both land, so there is no failure to
      `unwrap_err`;
    - `ten_rows…` makes 0 Whirlpool calls, not 20.
- [ ] **Step 3: M2: update each row.**
  - Make this the first step inside `step.forEach`. Put `const position =
    account.iteration('position');` at the top of the file, and import
    `ORCA_UPDATE_FEES_AND_REWARDS` from `./shared.js`:

```ts
        step.invoke({
          program: account.fixed('whirlpoolProgram'),
          accounts: [
            { account: account.fixed('whirlpool'), signer: false, writable: true },
            { account: position, signer: false, writable: true },
            { account: account.iteration('tickArrayLower'), signer: false, writable: false },
            { account: account.iteration('tickArrayUpper'), signer: false, writable: false },
          ],
          data: [data.literal(ORCA_UPDATE_FEES_AND_REWARDS)],
          label: 'updateIfLiquid',
        }),
```

  - Expect `only_the_rows…` to fail in Whirlpools with 6012 at the `empty` row: one row without
    liquidity reverts the whole harvest. The other three pass.
  - Then add the guard to the update:

```ts
          when: expression.greaterThan(
            expression.accountData(position, ORCA_POSITION.liquidity, 'u128'),
            expression.u128(0),
          ),
```

  - Expect `only_the_rows…` to receive `(47454545, 2485714)` against the expected
    `(47454545, 4971428)`. The B-only row's fee stayed behind, because the guard reads only A:
    that is M3. (These numbers are from the planning snapshot.)
- [ ] **Step 4: M3: collect when either fee is above the floor.** Add the helper under
  `position`, and give `collectIfWorthIt` the new guard:

```ts
const aboveFloor = (offset: number) =>
  expression.greaterThan(
    expression.accountData(position, offset, 'u64'),
    expression.input('dustFloor'),
  );
```

```ts
          // This row's own fees, just updated, decide whether it collects.
          when: expression.or(aboveFloor(ORCA_POSITION.feeOwedA), aboveFloor(ORCA_POSITION.feeOwedB)),
```

  All four pass. The four-row run makes five Whirlpool calls: three updates and two collects.
- [ ] **Step 5: M5: the header says what the guard is for.**
  - Replace the header with the one in the final file below: the guard saves compute and does
    not prevent reverts, and it states the row limits `ten_rows…` measures.
  - Also give `dustFloor` and `whirlpool` their doc comments.
  - The payload does not change.

```ts
/**
 * Harvest fees from a page of Orca positions, skipping the ones that earned nothing.
 *
 * A liquidity manager holds dozens of positions. Most have earned something since the last
 * harvest and some have not, and which is which depends on trades that land after the transaction
 * is signed.
 *
 * So each row first calls `update_fees_and_rewards`, which folds the pool's fee growth into the
 * position. Without it, `fee_owed_a` and `fee_owed_b` hold only what the last update recorded, and
 * a row that had earned would look empty. Then the row collects if either fee is above
 * `dustFloor`. The update is skipped for a position without liquidity, where it fails with
 * `LiquidityZero` (6012) and has nothing to record.
 *
 * Skipping a row saves compute, about 11,000 units per collect, and leaves dust alone. It does not
 * prevent reverts: `collect_fees` with nothing owed succeeds and moves nothing. What reverts the
 * whole harvest is a row Whirlpools refuses, such as a position the signer does not hold
 * (`MissingOrInvalidDelegate`, 6019) or one from another pool (`ConstraintHasOne`, 2001). Those do
 * not depend on trades, so filter them out before building the run.
 *
 * A row is the position, the token account holding its NFT, and the tick arrays holding its lower
 * and upper ticks. A row that collects costs about 24,000 compute units, so eight fit the default
 * limit of 200,000 and more need a compute budget. Rows share keys when they share tick arrays: then
 * about ten fit a legacy transaction, and twelve, the template's limit, need a lookup table.
 *
 * Offsets come from `Position`, `LEN = 8 + 136 + 72`: `liquidity` at 72, `fee_owed_a` at 112 and
 * `fee_owed_b` at 136.
 */
import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  data,
  defineTemplate,
  expression,
  step,
} from '../../src/index.js';
import {
  ORCA_COLLECT_FEES,
  ORCA_POSITION,
  ORCA_UPDATE_FEES_AND_REWARDS,
  ORCA_WHIRLPOOL,
  addressBytes,
} from './shared.js';

const position = account.iteration('position');
const aboveFloor = (offset: number) =>
  expression.greaterThan(
    expression.accountData(position, offset, 'u64'),
    expression.input('dustFloor'),
  );

export const orcaHarvestManyPositions = defineTemplate({
  inputs: {
    /** Fees at or below this, in either token's base units, are left for a later harvest. */
    dustFloor: { type: 'u64' },
  },
  accounts: {
    whirlpoolProgram: { executable: true, address: addressBytes(ORCA_WHIRLPOOL) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    positionAuthority: { signer: true },
    /** Written by each row's `update_fees_and_rewards`. */
    whirlpool: { writable: true },
    tokenOwnerAccountA: { writable: true },
    tokenOwnerAccountB: { writable: true },
    tokenVaultA: { writable: true },
    tokenVaultB: { writable: true },
  },
  batch: {
    maxIterations: 12,
    minIterations: 1,
    row: {
      position: {
        writable: true,
        owner: addressBytes(ORCA_WHIRLPOOL),
        minDataLength: ORCA_POSITION.length,
      },
      positionTokenAccount: {},
      /** The tick array holding the position's lower tick; `update_fees_and_rewards` reads it. */
      tickArrayLower: {},
      /** The tick array holding the position's upper tick. */
      tickArrayUpper: {},
    },
  },
  steps: [
    step.forEach(
      [
        step.invoke({
          program: account.fixed('whirlpoolProgram'),
          accounts: [
            { account: account.fixed('whirlpool'), signer: false, writable: true },
            { account: position, signer: false, writable: true },
            { account: account.iteration('tickArrayLower'), signer: false, writable: false },
            { account: account.iteration('tickArrayUpper'), signer: false, writable: false },
          ],
          data: [data.literal(ORCA_UPDATE_FEES_AND_REWARDS)],
          when: expression.greaterThan(
            expression.accountData(position, ORCA_POSITION.liquidity, 'u128'),
            expression.u128(0),
          ),
          label: 'updateIfLiquid',
        }),
        step.invoke({
          program: account.fixed('whirlpoolProgram'),
          accounts: [
            { account: account.fixed('whirlpool'), signer: false, writable: false },
            { account: account.fixed('positionAuthority'), signer: true, writable: false },
            { account: position, signer: false, writable: true },
            { account: account.iteration('positionTokenAccount'), signer: false, writable: false },
            { account: account.fixed('tokenOwnerAccountA'), signer: false, writable: true },
            { account: account.fixed('tokenVaultA'), signer: false, writable: true },
            { account: account.fixed('tokenOwnerAccountB'), signer: false, writable: true },
            { account: account.fixed('tokenVaultB'), signer: false, writable: true },
            { account: account.fixed('tokenProgram'), signer: false, writable: false },
          ],
          data: [data.literal(ORCA_COLLECT_FEES)],
          // This row's own fees, just updated, decide whether it collects.
          when: expression.or(aboveFloor(ORCA_POSITION.feeOwedA), aboveFloor(ORCA_POSITION.feeOwedB)),
          label: 'collectIfWorthIt',
        }),
      ],
      { label: 'everyPosition' },
    ),
  ],
});

export const compiled = compileTemplate(orcaHarvestManyPositions);
```

  Run `pnpm fixtures`, `pnpm --dir clients/js check` and
  `cargo test --manifest-path tests/protocols/Cargo.toml`. Commit: "Update each Orca position before
  harvesting it, and collect either fee".

### Task 8: Name the program that refused a harvest (M6)

- [ ] **Step 1: The failing tests,** at the end of `clients/js/src/protocol-semantics.test.ts`.
  - The first test's logs are a real harvest signed by someone who does not hold the position.
    They were captured from `a_stranger_cannot_collect_and_whirlpools_says_so`.
  - Also import `describeFailure` from `run-orca-harvest.js`.

```ts
describe('the Orca harvest runner names the program that refused', () => {
  const BALLISTA = 'BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD';
  const WHIRLPOOLS = 'whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc';

  test('blames Whirlpools for its own code, though Ballista uses the same number', () => {
    const logs = [
      `Program ${BALLISTA} invoke [1]`,
      `Program ${WHIRLPOOLS} invoke [2]`,
      'Program log: Instruction: UpdateFeesAndRewards',
      `Program ${WHIRLPOOLS} consumed 7835 of 196554 compute units`,
      `Program ${WHIRLPOOLS} success`,
      `Program ${WHIRLPOOLS} invoke [2]`,
      'Program log: Instruction: CollectFees',
      'Program log: AnchorError occurred. Error Code: MissingOrInvalidDelegate. Error Number: 6019. Error Message: Position token account has a missing or invalid delegate.',
      `Program ${WHIRLPOOLS} consumed 7400 of 186548 compute units`,
      `Program ${WHIRLPOOLS} failed: custom program error: 0x1783`,
      `Program ${BALLISTA} consumed 20852 of 200000 compute units`,
      `Program ${BALLISTA} failed: custom program error: 0x1783`,
    ];
    expect(describeFailure(6019, logs)).toBe('code 6019 came from Whirlpools, not Ballista');
  });

  test('explains a refusal in Ballista by its account or step', () => {
    const logs = [
      `Program ${BALLISTA} invoke [1]`,
      `Program ${BALLISTA} consumed 1200 of 200000 compute units`,
      `Program ${BALLISTA} failed: custom program error: 0x81784`,
    ];
    expect(describeFailure((8 << 16) | 6020, logs)).toBe(
      'AccountConstraintFailed: account position in row 0 does not satisfy its constraint',
    );
  });

  test('does not guess without logs', () => {
    expect(describeFailure(6019, [])).toBe('code 6019; the logs name no program that failed');
  });
});
```

  - `vitest` runs them without type checking, so they run before the signature changes.
  - The first and third fail: today's `describeFailure` ignores the logs and reads 6019 as
    Ballista's `ReturnDataMismatch`.
- [ ] **Step 2: The fix.** In `run-orca-harvest.ts`, replace `describeFailure` and its comment
  with these two functions, then run `pnpm --dir clients/js check`:

```ts
/** The program named by the first `Program <id> failed: …` log line: the innermost that failed. */
export function failedProgram(logs: readonly string[]): string | undefined {
  for (const line of logs) {
    const match = /^Program ([1-9A-HJ-NP-Za-km-z]{32,44}) failed: /.exec(line);
    if (match) return match[1];
  }
  return undefined;
}

/**
 * Says which program refused a harvest, and where, from the failed transaction's code and logs.
 *
 * Whirlpools numbers its errors from 6000, as Ballista does, so the code alone cannot say who
 * refused: 6019 is Whirlpools' `MissingOrInvalidDelegate` and Ballista's `ReturnDataMismatch`. The
 * logs can. A failing CPI logs `Program <id> failed` first and every caller repeats the code after
 * it, so the first such line names the program the code belongs to.
 */
export function describeFailure(code: number, logs: readonly string[]): string {
  const program = failedProgram(logs);
  if (program === undefined) return `code ${code}; the logs name no program that failed`;
  if (program !== BALLISTA_ADDRESS) {
    return `code ${code} came from ${program === ORCA_WHIRLPOOL ? 'Whirlpools' : program}, not Ballista`;
  }
  const explanation = explainRunError(code, compiled);
  return explanation ? explanation.message : `code ${code} came from Ballista`;
}
```

- [ ] **Step 3:** Commit: "Name the program that refused an Orca harvest from its logs".

### Task 9: README and CI

- [ ] **Step 1: `tests/protocols/README.md`.**
  - Add a row to the snapshot table: `snapshot-orca/` holds the same files for the Orca tests —
    both SOL/USDC Whirlpools, their vaults and tick arrays, and five programs.
  - Add its refresh command beside milestone 1's:

```bash
node scripts/snapshot/snapshot.mjs scripts/snapshot/manifests/orca.json tests/protocols/snapshot-orca
```

  - Add one sentence: when `tests/orca_snapshot.rs` fails after a refresh, the price has left the
    listed tick arrays; list the ones its message names.
- [ ] **Step 2: CI.**
  - If milestone 1's CI job (its Task 9) is on the base branch, change its LFS cache key to
    `hashFiles('tests/protocols/snapshot*/manifest.json')`. Nothing else is needed:
    `cargo test --manifest-path tests/protocols/Cargo.toml` runs the new tests.
  - If the job is not there yet, say so in FINDINGS for whoever writes it.
- [ ] **Step 3:** Commit: "Document the Orca snapshot".

### Task 10: Findings, and hand back

- [ ] **Step 1: Re-measure.**
  - Run `cargo test --manifest-path tests/protocols/Cargo.toml -- --nocapture --test-threads=1`.
  - Take the compute units and sizes the tests print. Planning-run values:

| Run | Compute units | Bytes | Whirlpool calls |
| --- | --- | --- | --- |
| compound, fees in both tokens | 42,991 | 674 | 3 |
| compound, fees in one token | 25,059–25,067 | | 2 |
| compound, no fees | 11,740 | | 1 |
| compound, no liquidity | 2,543 | | 0 |
| compound, emptied position | 15,652 | | 1 |
| harvest, the four rows | 59,621 | 747 | 5 |
| harvest, ten earning rows | 237,672 | 1,195 (budget instruction included) | 20 |
| harvest, per earning row | about 23,600 | 68 | 2 |
| Orca: `update_fees_and_rewards` | 7,835 | | |
| Orca: `collect_fees`, nothing owed | 11,375 | | |

  - Harvest limits, from `m2val/rs/tests/orca_validation.rs::harvest_capacity`:
    - eight earning rows need 190,465 CU, under the default 200,000;
    - eleven rows are 1,263 bytes, over the 1,232-byte limit;
    - the old two-account row fit eleven. At twelve it was 1,235 bytes, so the declared maximum
      of 12 never fit a legacy transaction.
- [ ] **Step 2: `tests/protocols/FINDINGS.md`.** If milestone 1 already wrote the file, add an
  `## Orca` part; otherwise create it with a one-line intro and this part. Per template:
  - **`orcaCompoundFees`**
    - As written, it did nothing. It reads `fee_owed_*` without an update, so it always saw 0
      and both calls were skipped (M2).
    - Once something else has updated the position, its `increase_liquidity` fails with 6017
      unless the caller's liquidity fits the fees at the landing price. It fails for any
      liquidity when the fees are in one token, and takes the collect down with it (M4).
    - Fixed:
      - it updates first, skipping positions without liquidity;
      - it collects when either fee is above `dustFloor` (M3);
      - it reinvests with `increase_liquidity_by_token_amounts_v2`, bounded by
        `minSqrtPrice`/`maxSqrtPrice`, only when both fees are above the floor and the position
        still has liquidity;
      - `collect_fees` takes the whirlpool read-only (M1).
    - Interface changes for the docs:
      - `liquidityAmount` is gone; `minSqrtPrice` and `maxSqrtPrice` are new (u128, Q64.64);
      - the accounts gain `memoProgram` (pinned `MemoSq4…`), `tokenMintA` and `tokenMintB`;
      - the fixture has the order.
    - The docs page's claims that are now false:
      - "the caps ... top-up is exact": the deposit is the most liquidity the fees buy, and one
        fee is partly left in the wallet;
      - "they are read before collecting": still true, but only after the update;
      - "Set them too high and it tops up from the wallet": no longer possible.
    - Remaining limits:
      - SPL Token pools only;
      - a full-range position with dust-level fees can compute zero liquidity and fail with 6012.
        A `dustFloor` of a few thousand base units avoids it;
      - one `dustFloor` for two tokens.
  - **`orcaHarvestManyPositions`**
    - As written, it collected nothing, for the same reason (M2). Its guard read only token A
      (M3).
    - Fixed:
      - each row updates, skipping rows without liquidity, then collects when either fee is above
        the floor;
      - the row is `position`, `positionTokenAccount`, `tickArrayLower`, `tickArrayUpper`;
      - `collect_fees` takes the whirlpool read-only (M1). The whirlpool stays writable in the
        schema because updates write it, so M1 changes no locks.
    - M5, for the docs: the guard saves compute (about 11,000 CU per skipped collect) and never
      prevented a revert. `collect_fees` with nothing owed succeeds. What reverts the batch is a
      refused row: a stranger's position (6019) or another pool's (2001).
    - The docs page's claims that are now false:
      - "one `collect_fees` per position reverts the whole batch on the first position the
        protocol refuses", given as the reason for the `when`;
      - "from that row's own `fee_owed_a`".
    - The Rust example's `#rows` region changed: the docs page shows it.
    - Limits:
      - Ballista's own hold with room: a stride of 4 (limit 8), 56 runtime accounts at 12 rows
        (limit 120), 24 CPIs (limit 64);
      - about 23,600 CU per earning row, so 8 rows fit the default 200,000;
      - 10 rows fit a legacy transaction when the rows share tick arrays, and each extra
        distinct tick array costs 32 bytes;
      - 11–12 rows need a lookup table.
  - **M6, both.**
    - Whirlpool's codes collide with Ballista's: 6012 reads as `TypeMismatch`, 6017 as
      `InvalidPdaDerivation`, 6019 as `ReturnDataMismatch`.
    - Tests assert the failing program from the logs.
    - `run-orca-harvest.ts::describeFailure` now takes the logs.
    - Left to the program's owner: the doc comment on `RunError` in `execute.rs` ("never confused
      with Ballista's") does not hold for Anchor callees.
  - **Evidence.**
    - Quote each finding's failing output, saved in Task 6 Step 1 and at each step.
    - List the commits.
    - Name the snapshot's slot.
- [ ] **Step 3: Commit:** "Record what the Orca templates did against the real program".
- [ ] **Step 4: Hand back.** Use superpowers:finishing-a-development-branch.
  - First merge the latest `claude/protocol-tests` into `claude/protocol-orca`, and resolve there:
    - `fixtures/protocol-examples.json`: never merge by hand. Take either side and run
      `pnpm fixtures`;
    - `tests/protocols/Cargo.toml`: keep both sides' dependencies;
    - `tests/protocols/Cargo.lock`: take the base's, then run
      `cargo build --manifest-path tests/protocols/Cargo.toml --tests`, which adds what is missing
      without moving existing pins;
    - `tests/protocols/src/lib.rs`: keep every `pub mod`;
    - `.gitattributes`: the lines are identical;
    - `FINDINGS.md` and `README.md`: keep both parts.
  - Run the whole suite and `pnpm --dir clients/js check`.
  - Then merge `claude/protocol-orca` into `claude/protocol-tests` from its own worktree, but only
    if that worktree is clean. If another session has uncommitted work there, stop and hand the
    branch to the user instead.
