# Real-protocol tests, milestone 3: the Kamino and marginfi templates, and retiring Drift

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to carry out this plan task by task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** The lending templates run as real signed transactions in LiteSVM, against Kamino Lend,
Kamino Farms, marginfi v2 and Jupiter as they are deployed on mainnet:
- `jupiterDepositExactOutput`, rewritten;
- `kaminoRepaySwapOutput`, rewritten;
- `kaminoLiquidateWithProof`, rewritten;
- `marginfiWithdrawAllWithFloor`, which gains a health-check account group;
- `marginfiToKaminoRebalance`, new, replacing `driftRebalanceExact`.

`driftSettleWhenProfitable` is deleted. Drift v2's program was replaced by a withdraw-only drain
program (research `drift.md` §0), so neither Drift template can run against the real program.

Every fault the research found gets a real test that fails first, and a fix.

**Architecture:** see the spec, `docs/superpowers/specs/2026-09-26-real-protocol-tests-design.md`,
and milestone 1's plan, `docs/superpowers/plans/2026-09-26-protocol-tests-harness.md`. This
milestone adds:
- `tests/protocols/snapshot-lending/`, its own snapshot, written by `scripts/snapshot/snapshot.mjs`
  from `scripts/snapshot/manifests/lending.json`;
- `tests/protocols/src/{kamino,marginfi,lending}.rs`, and Scope writes in `oracle.rs`;
- `klend-interface` by git rev, and no marginfi crate (see Decision 3).

Tests still write only wallet balances, oracle price accounts (here, Scope's) and the clock.

**Research this plan depends on.** All paths are under
`/private/tmp/claude-501/-Users-jacob-Documents-ballista/06470e69-8486-4adc-b478-7e3c20bf99aa/scratchpad/`.
- `protocol-research/kamino-marginfi.md`. It has:
  - addresses (§2);
  - instruction layouts (§3);
  - what each template gets wrong (§4);
  - setup recipes (§5);
  - freshness and the clock (§6);
  - offsets (appendix A) and error codes (appendix B).
- `protocol-research/drift.md` §0: why Drift is retired.
- `m3-probe/`, the feasibility probe run while writing this plan:
  - `crate/tests/probe.rs` runs the setup recipes and the contract checks below against the real
    programs. Tasks 2 to 4 adapt its code.
  - `lending.json` is the draft manifest, and `snapshot-lending/` the scratch snapshot it produced
    (slot 451,127,679).
  - `m3-size/size-lending.mjs` estimates transaction sizes with the snapshot tool's own estimator.
- `m3-cratecheck/` holds the dependency-resolution probes behind Decision 3.
- `protocol-research/klend/` and `protocol-research/marginfi/` are the source trees at the verified
  commits.

**Base:** a new worktree, `.claude/worktrees/protocol-lending`, on branch `claude/protocol-lending`,
cut from `claude/protocol-tests` at `4c6b938` or later. The harness landed in `645cb8b`. Build
Ballista with `cargo build-sbf --manifest-path programs/ballista/Cargo.toml`.

**Conventions:**
- Commit after each task. End each message with a blank line and the `Co-Authored-By` line your
  own harness gives you.
- Never use `git stash`.
- Never edit `package.json` or `pnpm-lock.yaml`. Run the existing scripts: `pnpm fixtures`,
  `pnpm --dir clients/js check`, `pnpm --dir clients/js test`, `pnpm check:docs`.
- Never edit anything under `docs/`; that belongs to the docs session. Task 11 leaves it notes.
- Identify a failing program from the logs (`tx::Failure::program`), never from its code alone.
  klend, marginfi and Ballista all use codes from 6000 up.

---

## Decisions

1. **Refreshes go in the transaction, before the Ballista run. No template refreshes.**
   - klend's `_v2` handlers never look at instruction positions. They check `LastUpdate`: the
     reserve and obligation must have been refreshed in the current slot and not marked stale
     since. A top-level refresh before the run satisfies that exactly as well as one inside the
     template. The probe showed both halves: a v2 deposit through Ballista landed after top-level
     refreshes, and a deposit sent in a new slot without them failed with `ObligationStale`
     (6017).
   - The alternative is a template that refreshes, and it would carry klend's whole refresh
     protocol:
     - `refresh_obligation` takes every reserve the obligation holds, deposits then borrows, in the
       obligation's own order;
     - each `refresh_reserve` takes six accounts, with that reserve's own oracle setup (Scope here;
       Pyth or Switchboard elsewhere);
     - so the template would need a batch of six-account refresh rows plus a group, and would be
       tied to one market's oracle setup.
     A runner already reads the obligation to build the run, and klend-interface's
     `refresh_all_for_obligation` and Kamino's SDKs build exactly this prelude.
   - The templates guarantee amounts: the exact deposit, the exact repayment, the liquidator's
     payout, the floor. None of them depends on where the refresh sits. klend refuses a stale
     reserve or obligation whoever forgot to refresh it.
   - It costs no extra bytes: the refreshes' accounts are already in the transaction.
   - So `kaminoRepaySwapOutput` and `kaminoLiquidateWithProof` lose their refresh steps. Each
     Kamino template's header says what goes before the run. `protocol_runs.rs` gains
     `kamino_refreshes`, and a semantics test pins "one Kamino call per template, and it is not a
     refresh".
2. **klend's optional farm accounts travel as an account group, `farmAccounts`.**
   - v2 deposit, repay and liquidation end in farm accounts:
     - when the reserve has the farm, they must be writable;
     - when it does not, they must be klend's own program ID, which is read-only.
   - A template's declared CPI slot has one fixed writable flag, so it cannot serve both cases:
     - Declared writable, the klend ID fails. klend is invoked at the top level of the same
       transaction, which demotes it to read-only. Then either Ballista's writable constraint or
       the runtime's CPI privilege check refuses it.
     - Declared read-only, a real farm account fails klend's `mut` constraint.
   - Group members are forwarded with the transaction's own writable flag, so one template serves
     both. The probe forwarded v2 deposit's whole tail as a group, and it landed.
   - A group can only follow the declared accounts, so each group is klend's whole tail:
     - deposit, 3 accounts: the farm user state, the farm, Farms;
     - repay, 4: the farm user state, the farm, the lending market authority, Farms;
     - liquidation, 5: the collateral farm pair, the debt farm pair, Farms.
   - v2 deposit's `placeholder_user_destination_collateral` is always "none", so the template puts
     its own `kamino` account there, read-only.
3. **marginfi has no crate in this suite.**
   - `marginfi-type-crate` at the deployed commit `33c67987a6` cannot share a build with LiteSVM
     `6a550a1`. It pins `solana-instruction = "=3.4.0"`, and LiteSVM needs `~3.5.0`. Measured in
     `m3-cratecheck/mfi-only` and `mfi-nolock`: `failed to select a version for solana-instruction`.
   - `src/marginfi.rs` therefore writes the three instructions it sends from the source (research
     §3.2) and reads the handful of fields it needs at the research's offsets (appendix A, printed
     by `offset_of!` against the type crate).
   - A unit test checks those offsets against the snapshot's real banks, so a layout move fails
     there first.
4. **klend-interface by git rev `a08760976f`, converted at `kamino.rs`'s boundary.**
   - It resolves and builds beside the harness (`m3-cratecheck/klend-only`).
   - It speaks solana 2.x types. The 2.x crates are added under renamed keys
     (`solana-pubkey-v2`, `solana-instruction-v2`), and `kamino::{address, pubkey, instruction}`
     convert field by field.
   - Only its low-level `instructions::*` builders are used for anything that moves tokens, with
     vaults read from the `Reserve`. Its high-level helpers derive vaults from seeds, and the main
     market's SOL and USDC vaults are not those PDAs (research §2); a unit test pins that.
   - Refreshes use its helpers, which never touch vaults.
5. **One Jupiter route, `solToUsdc` (1 SOL), serves both swap templates.**
   - The deposit test deposits the route's USDC into the USDC reserve.
   - The repay test's obligation borrows three times the route's output, so one repayment never
     clears the debt, and the fall in debt is measurable.
6. **The clock.**
   - `lending::svm()` stamps the four Scope entries the reserves read with the snapshot's clock
     (rule 2; prices unchanged). Every Kamino price is then fresh, however long before the snapshot
     Scope last updated it; SOL's window is 120 s.
   - `lending::next_slot()` warps one slot and zero seconds before every template transaction:
     - Freshness then has to come from the template's own transaction, because LiteSVM never
       advances the slot by itself.
     - No interest accrues and no oracle ages between setup and run.
   - marginfi's SOL feed (70 s) is read only by the two-balance health check. There, marginfi values
     an asset with a failing oracle at zero rather than failing (`state/marginfi_account.rs`,
     "Skip stale oracles for Initial requirement"), so the suite does not stamp it.
   - Stamping marginfi's feeds would also need `oracle.rs` to accept their owner,
     `rec2HHDDnjLfj4kE7VyEtFA1HPGQLK33259532cRyHp`. Do that only if a scenario ever holds a
     liability.
7. **Naming the requirement a run failed at: milestone 1's helper.**
   - When this plan was written, milestone 1's Task 5 had these uncommitted in the
     `protocol-tests` worktree:
     - `labels` in `protocol-examples.json`, program counter to step label;
     - `Example::labels` and `Example::label_at(pc)`;
     - `tx::assert_requirement_failed(&failure, &example, label)`.
   - The failure tests here call `tx::assert_requirement_failed`.
   - Task 5 adds these only if they have not landed, under the same names so nothing forks.
8. **SPL Token only.** Every template pins `tokenProgram` to the SPL Token program and passes it
   in each token-program slot. Token-2022 reserves and banks are out of scope. The one Token-2022
   detail, marginfi's mint first in the remaining accounts, is documented and not tested.

## Evidence gathered while planning (`m3-probe`)

| Check | Result |
| --- | --- |
| klend-interface @`a08760976f` beside the harness | Resolves and builds; conversion probe passes |
| marginfi-type-crate @`33c67987a6` beside LiteSVM @`6a550a1` | `=3.4.0` against `~3.5.0`: no build |
| The draft manifest, snapshotted | Slot 451,127,679. klend was deployed at 440,486,775 and marginfi at 444,313,123, both verified builds. Farms at 444,035,168. `solToUsdc` is 21 accounts via Whirlpool |
| Open an obligation: metadata, obligation, two farm users | 128,163 CU, 739 B |
| Deposit 10 SOL; borrow 300 USDC, each with its refreshes | 93,794 CU; 91,734 CU |
| `refresh_reserve`: 3 accounts; Scope in the Pyth slot; 5 accounts | klend 3005; 6054; 3005 |
| `refresh_obligation`: no reserves; reserves not refreshed | klend 6006; 6009 |
| v2 deposit into the farmed USDC reserve, farm slots set to "none" | klend 6120 |
| Deposit with no `refresh_obligation` in the slot | klend 6017 |
| v1 deposit through Ballista; v2 through Ballista | klend 6080; lands, 96,508 CU, 822 B |
| Liquidation after SOL falls 12%, repaying 8.53 USDC | 81,344,321 lamports paid to the liquidity account, 0 cTokens left; break-even 79,539,128; 179,384 CU, 987 B |
| marginfi `withdraw_all` of USDC with SOL also deposited | No remaining accounts: marginfi 6008. `[SOL bank, SOL oracle]`: lands, 72,218 CU, and 99,999,999 of the 100 USDC comes back |
| Size estimate with this route | Deposit run with its refreshes ≈ 1,032 B; repay ≈ 952 B. No extra lookup table needed |

## Harness API this plan uses (landed in `645cb8b`)

- `snapshot`:
  - `Snapshot::load(dir)`, `.into_svm()`, `.route(name)`, `.account(&addr)`;
  - `Leg { in_amount, out_amount, other_amount_threshold, instructions.swap, route.args, lookup_tables, source_token_account, destination_token_account }`;
  - `warp(svm, slots, seconds)`.
- `wallet`: `wallet()`, `keypair(seed)`, `fund`, `token_account(svm, owner, mint, amount) -> ATA`,
  `token_balance`, `SOL`, `WSOL_MINT`.
- `oracle`: `pyth_price`, `set_pyth_price`, `PYTH_RECEIVER`.
- `tx`:
  - `send(svm, payer, signers, instructions, tables) -> Result<Outcome, Failure>`;
  - `Outcome { logs, compute_units, fee, size }`;
  - `Failure { program, code, err, logs, fee }`;
  - `ballista_error(&Failure) -> Option<(name, context)>`.
- `template`:
  - `examples()[name] -> &Example { payload, fixed_accounts, inputs, account_groups, … }`;
  - `upload(svm, creator, id, payload)`;
  - `Run::new(template, example).account(name, address, writable, signer).input_u64(..).input_bytes(..).group(name, metas).build()`.
    It panics on an unknown, missing or twice-bound name, and on flags other than the declared ones.
- In progress in milestone 1 when this plan was written. Use them once they land, and add the
  ones this plan needs (Task 5) if they do not:
  - `Snapshot::svm(&self)`: `into_svm` without consuming the snapshot;
  - `Snapshot::named(name)`: an account or program by its manifest name;
  - `Example::labels` and `Example::label_at(pc)`;
  - `tx::assert_requirement_failed(&failure, &example, label)`.
- Things to update when they change:
  - `template.rs`'s unit tests pin `jupiterDepositExactOutput`'s order
    (`a_run_built_by_name_matches_one_built_by_hand`, `a_missing_name_panics`). Task 6 updates
    them.
  - `every_example_lines_up_with_its_payload` pins the count, 12. Task 10 updates it.

## Merging

- **The rest of milestone 1**, on `claude/protocol-tests`, may still land Tasks 5 to 10. Merge
  that branch in when it does. Expect conflicts in:
  - `protocol-semantics.test.ts` and `shared.ts`: keep both sides;
  - `fixtures/protocol-examples.json`: resolve the sources, then `pnpm fixtures`;
  - `template.rs`: if its Task 5 adds a label lookup, keep one;
  - `FINDINGS.md`: append;
  - `.github/workflows/ci.yml`.
- **Milestone 2 (Orca)**, planned in parallel, touches:
  - `.gitattributes`: both make the same one-line change, so it merges cleanly;
  - `tests/protocols/Cargo.toml` and `Cargo.lock`: keep both sides' dependencies, then run
    `cargo check --manifest-path tests/protocols/Cargo.toml` to settle the lock;
  - `src/lib.rs`: keep both module lists;
  - `README.md`, `FINDINGS.md`, `protocol-semantics.test.ts`;
  - the fixtures: regenerate them;
  - the CI cache key.

---

## File map

| Path | Change |
| --- | --- |
| `.gitattributes` | `tests/protocols/snapshot*/programs/*.so` → LFS |
| `scripts/snapshot/manifests/lending.json` | New: programs, accounts and the route for this milestone |
| `tests/protocols/snapshot-lending/` | New snapshot (programs through LFS) |
| `tests/protocols/README.md` | The second snapshot and how to refresh it |
| `tests/protocols/Cargo.toml`, `Cargo.lock` | `klend-interface` by git rev; the two renamed solana 2.x crates |
| `tests/protocols/src/lib.rs` | `pub mod kamino; pub mod lending; pub mod marginfi;` |
| `tests/protocols/src/oracle.rs` | Scope: `scope_price`, `set_scope_price` (rule 2) |
| `tests/protocols/src/kamino.rs` | New: conversion, reserve and obligation readers, PDAs, instructions, refreshes, farm groups |
| `tests/protocols/src/marginfi.rs` | New: instructions, bank and account readers, health group |
| `tests/protocols/src/lending.rs` | New: the lending snapshot, `svm()`, `next_slot()`, setup recipes |
| `tests/protocols/src/template.rs` | `Example::labels` and `label_at`, unless milestone 1 added them; the deposit-order and count tests |
| `tests/protocols/tests/kamino_contract.rs` | New: what klend requires of a caller |
| `tests/protocols/tests/marginfi_contract.rs` | New: what marginfi's withdrawal requires |
| `tests/protocols/tests/{jupiter_deposit_exact_output,kamino_repay_swap_output,kamino_liquidate_with_proof,marginfi_withdraw_all_with_floor,marginfi_to_kamino_rebalance}.rs` | New: one per template |
| `clients/js/examples/protocols/{jupiter-deposit-exact-output,kamino-repay-swap-output,kamino-liquidate-with-proof,marginfi-withdraw-all-with-floor}.ts` | Fixed |
| `clients/js/examples/protocols/marginfi-to-kamino-rebalance.ts` | New |
| `clients/js/examples/protocols/drift-{rebalance-exact,settle-when-profitable}.ts` | Deleted |
| `clients/js/examples/protocols/{index,shared,run-jupiter-deposit}.ts` | Exports; constants (drop `DRIFT_*`, `u16Bytes`; add `SYSVAR_INSTRUCTIONS`, `KAMINO_FARMS`); the deposit runner |
| `clients/js/src/protocol-examples.test.ts` | `labels`, unless milestone 1 added it; count 12 → 11 |
| `clients/js/src/protocol-semantics.test.ts` | Kamino and marginfi call shapes; the deposit runner; Drift removed |
| `common/src/template/verify.rs` | Count 12 → 11 |
| `clients/rust/examples/protocol_runs.rs` | `#group` for the new deposit; new `#refresh` region; "eleven" |
| `tests/protocols/FINDINGS.md` | This milestone's section and notes for the docs session |

---

### Task 0: Worktree and baseline

- [ ] **Step 1: Create the worktree.** Use the superpowers:using-git-worktrees skill, or:

```bash
git -C /Users/jacob/Documents/ballista worktree add \
  /Users/jacob/Documents/ballista/.claude/worktrees/protocol-lending \
  -b claude/protocol-lending claude/protocol-tests
cd /Users/jacob/Documents/ballista/.claude/worktrees/protocol-lending
git log --oneline -3          # 4c6b938 or later; 645cb8b "Add the protocol-test harness" is an ancestor
git lfs pull                  # the milestone-1 programs as ELFs, not pointers
pnpm install --frozen-lockfile
```

- [ ] **Step 2: Baseline.** Record which of these pass before anything changes:

```bash
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
cargo test --manifest-path tests/protocols/Cargo.toml
pnpm --dir clients/js check && pnpm --dir clients/js test
cargo test -p ballista-common every_shared_fixture_parses_and_verifies
```

- [ ] **Step 3: What milestone 1 has landed.** Check for the helpers that were uncommitted when
  this plan was written (see "Harness API"):
  - `labels` in `fixtures/protocol-examples.json`;
  - `Example::label_at`;
  - `tx::assert_requirement_failed`;
  - `Snapshot::svm(&self)` and `Snapshot::named`.

  Write down what exists. Task 5 adds only what is missing. If milestone 1's label commit has
  landed on `claude/protocol-tests` by now, cut the worktree from it.

No commit.

### Task 1: The lending snapshot

- [ ] **Step 1: LFS for every snapshot directory.** In `.gitattributes`, replace the one line
  with:

```text
tests/protocols/snapshot*/programs/*.so filter=lfs diff=lfs merge=lfs -text
```

  Check it:

```bash
git check-attr filter -- tests/protocols/snapshot/programs/a.so tests/protocols/snapshot-lending/programs/a.so
# both: filter: lfs
```

- [ ] **Step 2: Write `scripts/snapshot/manifests/lending.json`.**
  - Scope's program is left out on purpose: klend reads its price account and never invokes it.
  - The three AMMs are listed for the same reason as in milestone 1.

```json
{
  "description": "Milestone 3 of the real-protocol tests: jupiterDepositExactOutput, kaminoRepaySwapOutput, kaminoLiquidateWithProof, marginfiWithdrawAllWithFloor and marginfiToKaminoRebalance, against Kamino Lend's main market and marginfi's main group. Kamino Farms is taken because klend invokes it on every deposit, repayment and liquidation that touches a reserve with a farm. Scope's program is not: klend reads Scope's price account but never invokes Scope. Vaults, collateral mints and farms are listed by address because these reserves' vaults are not the seed PDAs klend-interface derives; each Reserve stores its own. All three AMMs in jupiter.dexes are listed so the program set stays the same from one refresh to the next, whichever of them Jupiter picks.",
  "wallet": {
    "seed": "ballista-protocol-tests-wallet-1",
    "description": "The tests sign with Keypair::new_from_array(*b\"ballista-protocol-tests-wallet-1\"). Jupiter builds the route for this wallet, so the route's token accounts are its associated token accounts, which the tests write themselves."
  },
  "programs": {
    "kamino": "KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD",
    "kaminoFarms": "FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr",
    "marginfi": "MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA",
    "jupiter": "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4",
    "token": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
    "associatedToken": "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
    "whirlpool": "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc",
    "raydiumClmm": "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK",
    "meteoraDlmm": "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo"
  },
  "accounts": {
    "usdcMint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
    "wsolMint": "So11111111111111111111111111111111111111112",
    "kaminoMainMarket": "7u3HeHxYDLhnCoErrtycNokbQYbWGzLs6JSDqGAv5PfF",
    "kaminoSolReserve": "d4A2prbA2whesmvHaL88BH6Ewn5N4bTSU2Ze8P6Bc4Q",
    "kaminoSolSupplyVault": "GafNuUXj9rxGLn4y79dPu6MHSuPWeJR6UtTWuexpGh3U",
    "kaminoSolFeeVault": "3JNof8s453bwG5UqiXBLJc77NRQXezYYEBbk3fqnoKph",
    "kaminoSolCollateralMint": "2UywZrUdyqs5vDchy7fKQJKau2RVyuzBev2XKGPDSiX1",
    "kaminoSolCollateralSupply": "8NXMyRD91p3nof61BTkJvrfpGTASHygz1cUvc3HvwyGS",
    "kaminoSolCollateralFarm": "955xWFhSDcDiUgUr4sBRtCpTLiMd4H5uZLAmgtP3R3sX",
    "kaminoUsdcReserve": "D6q6wuQSrifJKZYpR1M8R4YawnLDtDsMmWM1NbBmgJ59",
    "kaminoUsdcSupplyVault": "Bgq7trRgVMeq33yt235zM2onQ4bRDBsY5EWiTetF4qw6",
    "kaminoUsdcFeeVault": "BbDUrk1bVtSixgQsPLBJFZEF7mwGstnD5joA1WzYvYFX",
    "kaminoUsdcCollateralMint": "B8V6WVjPxW1UGwVDfxH2d2r8SyT4cqn7dQRK6XneVa7D",
    "kaminoUsdcCollateralSupply": "3DzjXRfxRm6iejfyyMynR4tScddaanrePJ1NJU2XnPPL",
    "kaminoUsdcCollateralFarm": "JAvnB9AKtgPsTEoKmn24Bq64UMoYcrtWtq42HHBdsPkh",
    "scopePrices": "3t4JZcueEzTbVP6kLxXrL3VpWx45jDer4eqysweBchNH",
    "marginfiMainGroup": "4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8",
    "marginfiUsdcBank": "2s37akK2eyBbp8DZgCm7RtsaEz8eJP3Nxd4urLHQv7yB",
    "marginfiUsdcVault": "7jaiZR5Sk8hdYN9MxTpczTcwbWpb5WEoxSANuUwveuat",
    "marginfiUsdcOracle": "6HAuqASbHEh4w4REJEUUUCginTLfj1kwCh215ZLtMkrT",
    "marginfiSolBank": "CCKtUs6Cgwo4aaQUmBPmyoApH2gUDErxNZCAntD6LYGh",
    "marginfiSolVault": "2eicbpitfJXDwqCuFAmPgDP7t2oUotnAzbGzRKLMgSLe",
    "marginfiSolOracle": "7AviUf9nL62mcxNbQGKm4nKDQnPjswo6c5MX4D57HmyE"
  },
  "jupiter": {
    "description": "Classic AMMs only, as in milestone 1: they are deterministic and change only when traded.",
    "dexes": ["Whirlpool", "Raydium CLMM", "Meteora DLMM"],
    "maxAccounts": 30,
    "slippageBps": 50
  },
  "routes": {
    "solToUsdc": {
      "description": "1 SOL for USDC: what jupiterDepositExactOutput deposits into Kamino's USDC reserve, and what kaminoRepaySwapOutput repays a USDC borrow with.",
      "legs": [
        {
          "inputMint": "So11111111111111111111111111111111111111112",
          "outputMint": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
          "amount": 1000000000
        }
      ]
    }
  }
}
```

- [ ] **Step 3: Snapshot.**

```bash
node scripts/snapshot/snapshot.mjs scripts/snapshot/manifests/lending.json tests/protocols/snapshot-lending
```

  Check the summary it prints:
  - **Nine programs.** klend must still be deployed at slot 440,486,775 and marginfi at
    444,313,123: the verified builds that the crate pin and the offsets come from. If either has
    moved:
    1. stop;
    2. find the new verified commit on `verify.osec.io/status/<program id>`;
    3. re-check research §3 and appendix A against it;
    4. move the `klend-interface` rev before going on.
  - **Record Farms' deploy slot** in the commit message. It was 444,035,168 while this plan was
    written.
  - **No manifest account may be absent;** the tool refuses to write if one is. An absent account
    that only the route references is fine.
  - **The route's template-run estimate must leave at least 250 bytes spare.**
    - The deposit adds about 12 static keys beyond the tool's two-account allowance, about 200
      bytes (`m3-size/size-lending.mjs`).
    - If it leaves less, set `"maxAccounts": 24` and run again.

- [ ] **Step 4: README.** In `tests/protocols/README.md`:
  - add a row under "The snapshot": `snapshot-lending/`, holding the Kamino and marginfi
    templates' snapshot, from `manifests/lending.json`;
  - add its refresh command next to milestone 1's:
    `node scripts/snapshot/snapshot.mjs scripts/snapshot/manifests/lending.json tests/protocols/snapshot-lending`.
- [ ] **Step 5: CI cache key.** Check whether `.github/workflows/ci.yml` has milestone 1's
  protocol-test job yet (its Task 9).
  - If it does, key its LFS cache on `hashFiles('tests/protocols/snapshot*/manifest.json')`.
  - If it does not, note it for Task 11.
- [ ] **Step 6: Commit** "Snapshot Kamino, marginfi and a Jupiter route for the lending templates":

```bash
git add .gitattributes scripts/snapshot/manifests/lending.json tests/protocols/snapshot-lending tests/protocols/README.md
git lfs ls-files | grep snapshot-lending     # nine .so files
git commit
git show --stat HEAD                          # the .so files are ~130-byte pointers
```

### Task 2: Scope and Kamino in the harness

Adapt `m3-probe/crate/tests/probe.rs`: its `svm`, `refreshes`, `open_obligation`, `deposit_ix` and
`borrow_ix` are this task's functions, already run against the real programs.

- [ ] **Step 1: Dependencies.** Add these to `tests/protocols/Cargo.toml`'s `[dependencies]`, then
  run `cargo check --manifest-path tests/protocols/Cargo.toml` so `Cargo.lock` picks them up:

```toml
# Kamino Lend's instruction builders and account layouts, at the build deployed on mainnet
# (verify.osec.io: Kamino-Finance/klend@a08760976f = release/v1.25.0). crates.io's 0.6.0 predates
# it. It speaks solana 2.x; src/kamino.rs converts at its boundary.
klend-interface = { git = "https://github.com/Kamino-Finance/klend", rev = "a08760976f51a3a58c4a0c6ea27b4a0e565bca79" }
# The 2.x types klend-interface speaks, renamed so they cannot be mistaken for this crate's own.
solana-pubkey-v2 = { package = "solana-pubkey", version = "2.1" }
solana-instruction-v2 = { package = "solana-instruction", version = "2.1" }
# No marginfi crate: at the deployed commit its type crate pins solana-instruction =3.4.0, and
# LiteSVM needs ~3.5. See src/marginfi.rs.
```

- [ ] **Step 2: A failing Scope test, then Scope in `oracle.rs`.**
  - Add the test first, `a_written_scope_price_reads_back_and_nothing_else_changes`.
    `lending.rs` does not exist yet, so:
    - load `concat!(env!("CARGO_MANIFEST_DIR"), "/snapshot-lending")` directly;
    - copy the Scope account `3t4JZcueEzTbVP6kLxXrL3VpWx45jDer4eqysweBchNH` into a bare SVM, the
      way `sol_usd()` copies the Pyth feed.
  - The test:
    - reads entry 3 (SOL) through `scope_price`, and asserts `exp == 8` and `value > 0`;
    - writes a new value, slot and timestamp with `set_scope_price`, and reads them back;
    - asserts that the account's owner, lamports and length are unchanged, and that no byte
      changed outside that entry's `value`, `slot` and `unix_timestamp`: `exp` stays.
  - A second test, `a_scope_write_refuses_another_account`, is `#[should_panic(expected = "is not a Scope OraclePrices account")]`.
    It copies the USDC mint into the bare SVM and passes it to `set_scope_price`.
  - It fails to compile. Then add the following, and make the module doc say "moves a Pyth or a
    Scope price":

```rust
/// Scope, whose `OraclePrices` accounts Kamino's reserves price from. klend reads them and never
/// invokes Scope, so the program is not in the snapshot.
pub const SCOPE: Address = Address::from_str_const("HFn8GnPADiny6XqUoWE8uRPPxb29ikn4yTuPa9MF2fWJ");
/// `OraclePrices`: discriminator, `oracle_mappings`, then 512 `DatedPrice` entries of 56 bytes.
/// klend casts the account with `bytemuck::from_bytes`, so its length must never change.
pub const SCOPE_PRICES_LEN: usize = 28_712;
const SCOPE_DISCRIMINATOR: [u8; 8] = [0x59, 0x80, 0x76, 0xdd, 0x06, 0x48, 0xb4, 0x92];
const SCOPE_FIRST_ENTRY: usize = 40;
const SCOPE_ENTRY_LEN: usize = 56;
/// Within an entry, each a little-endian u64: `price.value`, `price.exp`, `last_updated_slot`
/// (klend ignores it) and `unix_timestamp` (klend measures a price's age from it).
const SCOPE_VALUE: usize = 0;
const SCOPE_EXP: usize = 8;
const SCOPE_SLOT: usize = 16;
const SCOPE_TIMESTAMP: usize = 24;

/// One Scope entry: the price is `value / 10^exp` dollars.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScopePrice {
    pub value: u64,
    pub exp: u64,
    pub slot: u64,
    pub unix_timestamp: u64,
}

/// Entry `index` of the `OraclePrices` account at `prices`.
///
/// # Panics
///
/// If `prices` is not a Scope `OraclePrices` account.
pub fn scope_price(svm: &LiteSVM, prices: &Address, index: usize) -> ScopePrice {
    let data = scope_prices(svm, prices).data;
    let at = SCOPE_FIRST_ENTRY + SCOPE_ENTRY_LEN * index;
    let read = |offset: usize| u64::from_le_bytes(field(&data, at + offset));
    ScopePrice {
        value: read(SCOPE_VALUE),
        exp: read(SCOPE_EXP),
        slot: read(SCOPE_SLOT),
        unix_timestamp: read(SCOPE_TIMESTAMP),
    }
}

/// Writes entry `index`'s value, slot and time (write rule 2). Its exponent and every other byte
/// stay as they were.
///
/// # Panics
///
/// If `prices` is not a Scope `OraclePrices` account.
pub fn set_scope_price(svm: &mut LiteSVM, prices: &Address, index: usize, value: u64, slot: u64, unix_timestamp: u64) {
    let mut account = scope_prices(svm, prices);
    let at = SCOPE_FIRST_ENTRY + SCOPE_ENTRY_LEN * index;
    for (offset, word) in [(SCOPE_VALUE, value), (SCOPE_SLOT, slot), (SCOPE_TIMESTAMP, unix_timestamp)] {
        account.data[at + offset..at + offset + 8].copy_from_slice(&word.to_le_bytes());
    }
    svm.set_account(*prices, account)
        .unwrap_or_else(|error| panic!("writing Scope prices {prices} failed: {error:?}"));
}

fn scope_prices(svm: &LiteSVM, prices: &Address) -> Account {
    let account = svm
        .get_account(prices)
        .unwrap_or_else(|| panic!("Scope prices {prices} are not in the SVM"));
    assert!(
        account.owner == SCOPE
            && account.data.len() == SCOPE_PRICES_LEN
            && account.data[..8] == SCOPE_DISCRIMINATOR,
        "{prices} is not a Scope OraclePrices account"
    );
    account
}
```

- [ ] **Step 3: `src/kamino.rs`.** Write it in full, as follows:

```rust
//! Kamino Lend (klend): instructions for setup and the contract tests, and the account fields the
//! scenarios read.
//!
//! Instructions come from `klend-interface`, pinned to the build deployed on mainnet.
//! - It speaks solana 2.x (`solana-pubkey` 2, `solana-instruction` 2), and this crate 3.x/4.x.
//!   [`address`], [`pubkey`] and [`instruction`] convert field by field; nothing outside this
//!   module sees a 2.x type.
//! - Anything that moves tokens uses its low-level `instructions::*` builders, with the vaults read
//!   from the `Reserve`. Its high-level helpers derive vaults from seeds, and the main market's SOL
//!   and USDC reserves predate those seeds. Refreshes use the helpers, which never touch vaults.
//! - klend is built with Anchor 0.29 without `allow-missing-optionals`, so every optional account
//!   must be present, with klend's own program ID meaning "none". klend-interface does that.

use {
    klend_interface::{self as klend, state},
    litesvm::LiteSVM,
    solana_address::Address,
    solana_instruction::{AccountMeta, Instruction},
    solana_pubkey_v2::Pubkey,
};

pub const KLEND: Address = Address::from_str_const("KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD");
/// Kamino Farms, which klend invokes whenever a deposit, repayment or liquidation touches a
/// reserve with a farm.
pub const FARMS: Address = Address::from_str_const("FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr");
/// Every v2 lending instruction takes the instructions sysvar (only v1 reads it).
pub const INSTRUCTIONS_SYSVAR: Address =
    Address::from_str_const("Sysvar1nstructions1111111111111111111111111");

/// How many accounts each v2 instruction takes before its farm tail, which the templates forward
/// as `farmAccounts`.
pub const DEPOSIT_DECLARED: usize = 14;
pub const REPAY_DECLARED: usize = 9;
pub const LIQUIDATE_DECLARED: usize = 20;

/// `deposit_reserve_liquidity_and_obligation_collateral`, v1: its accounts are v2's first 14.
const DEPOSIT_V1: [u8; 8] = [0x81, 0xc7, 0x04, 0x02, 0xde, 0x27, 0x1a, 0x2e];

// ------------------------------------------------------------------------------- conversion

pub fn address(key: &Pubkey) -> Address {
    Address::new_from_array(key.to_bytes())
}

pub fn pubkey(address: &Address) -> Pubkey {
    Pubkey::new_from_array(address.to_bytes())
}

pub fn instruction(instruction: solana_instruction_v2::Instruction) -> Instruction {
    Instruction {
        program_id: address(&instruction.program_id),
        accounts: instruction
            .accounts
            .into_iter()
            .map(|meta| AccountMeta {
                pubkey: address(&meta.pubkey),
                is_signer: meta.is_signer,
                is_writable: meta.is_writable,
            })
            .collect(),
        data: instruction.data,
    }
}

// --------------------------------------------------------------------------------- accounts

fn account_data(svm: &LiteSVM, address: &Address) -> Vec<u8> {
    svm.get_account(address)
        .unwrap_or_else(|| panic!("{address} is not in the SVM"))
        .data
}

pub fn reserve(svm: &LiteSVM, address: &Address) -> state::Reserve {
    *klend::from_account_data::<state::Reserve>(&account_data(svm, address))
        .unwrap_or_else(|error| panic!("{address} is not a klend reserve: {error:?}"))
}

pub fn obligation(svm: &LiteSVM, address: &Address) -> state::Obligation {
    *klend::from_account_data::<state::Obligation>(&account_data(svm, address))
        .unwrap_or_else(|error| panic!("{address} is not a klend obligation: {error:?}"))
}

/// The accounts a reserve names, read from it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReserveAccounts {
    pub lending_market: Address,
    pub liquidity_mint: Address,
    pub supply_vault: Address,
    pub fee_vault: Address,
    pub collateral_mint: Address,
    pub collateral_supply: Address,
    pub token_program: Address,
    pub farm_collateral: Option<Address>,
    pub farm_debt: Option<Address>,
    pub scope_prices: Option<Address>,
}

pub fn reserve_accounts(svm: &LiteSVM, reserve: &Address) -> ReserveAccounts {
    let stored = self::reserve(svm, reserve);
    let named = |key: Pubkey| (key != Pubkey::default()).then(|| address(&key));
    ReserveAccounts {
        lending_market: address(&stored.lending_market),
        liquidity_mint: address(&stored.liquidity.mint_pubkey),
        supply_vault: address(&stored.liquidity.supply_vault),
        fee_vault: address(&stored.liquidity.fee_vault),
        collateral_mint: address(&stored.collateral.mint_pubkey),
        collateral_supply: address(&stored.collateral.supply_vault),
        token_program: address(&stored.liquidity.token_program),
        farm_collateral: named(stored.farm_collateral),
        farm_debt: named(stored.farm_debt),
        scope_prices: named(stored.config.token_info.scope_configuration.price_feed),
    }
}

/// The cTokens `obligation` holds from `reserve`, or zero.
pub fn deposited(svm: &LiteSVM, obligation: &Address, reserve: &Address) -> u64 {
    self::obligation(svm, obligation)
        .deposits
        .iter()
        .find(|deposit| address(&deposit.deposit_reserve) == *reserve)
        .map_or(0, |deposit| deposit.deposited_amount)
}

/// What `obligation` owes `reserve`: a fraction with 60 fractional bits, or zero.
pub fn borrowed_sf(svm: &LiteSVM, obligation: &Address, reserve: &Address) -> u128 {
    self::obligation(svm, obligation)
        .borrows
        .iter()
        .find(|borrow| address(&borrow.borrow_reserve) == *reserve)
        .map_or(0, |borrow| u128::from(borrow.borrowed_amount_sf))
}

/// Whether klend would liquidate `obligation` as its last refresh left it: its debt, adjusted by
/// borrow factor, has reached its unhealthy borrow value.
pub fn is_liquidatable(svm: &LiteSVM, obligation: &Address) -> bool {
    let stored = self::obligation(svm, obligation);
    u128::from(stored.borrow_factor_adjusted_debt_value_sf) >= u128::from(stored.unhealthy_borrow_value_sf)
}

// ------------------------------------------------------------------------------------- PDAs

pub fn lending_market_authority(market: &Address) -> Address {
    address(&klend::pda::lending_market_authority(&klend::KLEND_PROGRAM_ID, &pubkey(market)).0)
}

pub fn user_metadata(owner: &Address) -> Address {
    address(&klend::pda::user_metadata(&klend::KLEND_PROGRAM_ID, &pubkey(owner)).0)
}

/// `owner`'s vanilla obligation in `market`: tag 0, id 0, no seed accounts.
pub fn vanilla_obligation(owner: &Address, market: &Address) -> Address {
    let none = Pubkey::default();
    address(&klend::pda::obligation(&klend::KLEND_PROGRAM_ID, 0, 0, &pubkey(owner), &pubkey(market), &none, &none).0)
}

/// `obligation`'s user state in `farm`.
pub fn obligation_farm(farm: &Address, obligation: &Address) -> Address {
    address(&klend::pda::farms_user_state(&pubkey(farm), &pubkey(obligation)).0)
}

// ----------------------------------------------------------------------------- instructions

/// `init_user_metadata`, paid by `owner`, with no referrer and no lookup table.
pub fn init_user_metadata(owner: &Address) -> Instruction {
    instruction(klend::instructions::init_user_metadata(
        klend::instructions::InitUserMetadataAccounts {
            owner: pubkey(owner),
            fee_payer: pubkey(owner),
            user_metadata: pubkey(&user_metadata(owner)),
            referrer_user_metadata: None,
        },
        Pubkey::default(),
    ))
}

/// `init_obligation` for `owner`'s vanilla obligation in `market`, paid by `owner`. Tag 0 takes
/// the default key for both seed accounts, which is the System program's address.
pub fn init_obligation(owner: &Address, market: &Address) -> Instruction {
    instruction(klend::instructions::init_obligation(
        klend::instructions::InitObligationAccounts {
            obligation_owner: pubkey(owner),
            fee_payer: pubkey(owner),
            obligation: pubkey(&vanilla_obligation(owner, market)),
            lending_market: pubkey(market),
            seed1_account: Pubkey::default(),
            seed2_account: Pubkey::default(),
            owner_user_metadata: pubkey(&user_metadata(owner)),
        },
        klend::types::InitObligationArgs { tag: 0, id: 0 },
    ))
}

/// `init_obligation_farms_for_reserve` in collateral mode: `obligation`'s user state in `reserve`'s
/// collateral farm. klend needs it before the obligation's first deposit into a reserve with one.
/// `owner` is passed because the obligation may not exist yet when this is built.
pub fn init_obligation_farm(svm: &LiteSVM, payer: &Address, owner: &Address, obligation: &Address, reserve: &Address) -> Instruction {
    let r = reserve_accounts(svm, reserve);
    let farm = r.farm_collateral.unwrap_or_else(|| panic!("reserve {reserve} has no collateral farm"));
    instruction(klend::instructions::init_obligation_farms_for_reserve(
        klend::instructions::InitObligationFarmsForReserveAccounts {
            payer: pubkey(payer),
            owner: pubkey(owner),
            obligation: pubkey(obligation),
            lending_market_authority: pubkey(&lending_market_authority(&r.lending_market)),
            reserve: pubkey(reserve),
            reserve_farm_state: pubkey(&farm),
            obligation_farm: pubkey(&obligation_farm(&farm, obligation)),
            lending_market: pubkey(&r.lending_market),
        },
        0, // ReserveFarmKind::Collateral
    ))
}

/// What a transaction needs before a klend v2 call on `obligation`:
/// - `refresh_reserve` for each reserve the obligation holds, and for each of `also`;
/// - then `refresh_obligation`, with the held reserves writable: deposits, then borrows.
pub fn refreshes(svm: &LiteSVM, obligation: &Address, also: &[Address]) -> Vec<Instruction> {
    let held = klend::ObligationInfo::from_account_data(pubkey(obligation), &account_data(svm, obligation))
        .unwrap_or_else(|error| panic!("{obligation} is not an obligation: {error:?}"));
    let market = self::obligation(svm, obligation).lending_market;
    let mut reserves: Vec<Pubkey> = Vec::new();
    for reserve in held.deposit_reserves.iter().chain(&held.borrow_reserves).copied().chain(also.iter().map(pubkey)) {
        if !reserves.contains(&reserve) {
            reserves.push(reserve);
        }
    }
    let infos: Vec<klend::ReserveInfo> = reserves
        .iter()
        .map(|reserve| klend::ReserveInfo::from_account_data(*reserve, &account_data(svm, &address(reserve))).expect("a reserve"))
        .collect();
    let mut instructions: Vec<Instruction> = infos.iter().map(|info| instruction(klend::helpers::refresh_reserve(info))).collect();
    instructions.push(instruction(klend::helpers::refresh_obligation(&market, &held, &infos)));
    instructions
}

/// `deposit_reserve_liquidity_and_obligation_collateral_v2`: moves `amount` of `reserve`'s
/// liquidity from `source` into `obligation`. The farm accounts are included when the reserve has
/// a collateral farm.
pub fn deposit(svm: &LiteSVM, owner: &Address, obligation: &Address, reserve: &Address, source: &Address, amount: u64) -> Instruction {
    let r = reserve_accounts(svm, reserve);
    instruction(klend::instructions::deposit_reserve_liquidity_and_obligation_collateral_v2(
        klend::instructions::DepositReserveLiquidityAndObligationCollateralV2Accounts {
            owner: pubkey(owner),
            obligation: pubkey(obligation),
            lending_market: pubkey(&r.lending_market),
            lending_market_authority: pubkey(&lending_market_authority(&r.lending_market)),
            reserve: pubkey(reserve),
            reserve_liquidity_mint: pubkey(&r.liquidity_mint),
            reserve_liquidity_supply: pubkey(&r.supply_vault),
            reserve_collateral_mint: pubkey(&r.collateral_mint),
            reserve_destination_deposit_collateral: pubkey(&r.collateral_supply),
            user_source_liquidity: pubkey(source),
            placeholder_user_destination_collateral: None,
            liquidity_token_program: pubkey(&r.token_program),
            obligation_farm_user_state: r.farm_collateral.map(|farm| pubkey(&obligation_farm(&farm, obligation))),
            reserve_farm_state: r.farm_collateral.map(|farm| pubkey(&farm)),
        },
        amount,
    ))
}

/// The same deposit under v1's discriminator with v1's 14 accounts. klend refuses it by CPI.
pub fn deposit_v1(svm: &LiteSVM, owner: &Address, obligation: &Address, reserve: &Address, source: &Address, amount: u64) -> Instruction {
    let mut v1 = deposit(svm, owner, obligation, reserve, source, amount);
    v1.accounts.truncate(DEPOSIT_DECLARED);
    v1.data[..8].copy_from_slice(&DEPOSIT_V1);
    v1
}

/// `borrow_obligation_liquidity_v2` into `destination`, with no referrer and the reserve's debt
/// farm if it has one. The obligation is in no elevation group, so there are no remaining accounts.
pub fn borrow(svm: &LiteSVM, owner: &Address, obligation: &Address, reserve: &Address, destination: &Address, amount: u64) -> Instruction {
    let r = reserve_accounts(svm, reserve);
    instruction(klend::instructions::borrow_obligation_liquidity_v2(
        klend::instructions::BorrowObligationLiquidityV2Accounts {
            owner: pubkey(owner),
            obligation: pubkey(obligation),
            lending_market: pubkey(&r.lending_market),
            lending_market_authority: pubkey(&lending_market_authority(&r.lending_market)),
            borrow_reserve: pubkey(reserve),
            borrow_reserve_liquidity_mint: pubkey(&r.liquidity_mint),
            reserve_source_liquidity: pubkey(&r.supply_vault),
            borrow_reserve_liquidity_fee_receiver: pubkey(&r.fee_vault),
            user_destination_liquidity: pubkey(destination),
            referrer_token_state: None,
            token_program: pubkey(&r.token_program),
            obligation_farm_user_state: r.farm_debt.map(|farm| pubkey(&obligation_farm(&farm, obligation))),
            reserve_farm_state: r.farm_debt.map(|farm| pubkey(&farm)),
        },
        amount,
        vec![],
    ))
}

/// `repay_obligation_liquidity_v2` from `source`, with the reserve's debt farm if it has one.
pub fn repay(svm: &LiteSVM, owner: &Address, obligation: &Address, reserve: &Address, source: &Address, amount: u64) -> Instruction {
    let r = reserve_accounts(svm, reserve);
    instruction(klend::instructions::repay_obligation_liquidity_v2(
        klend::instructions::RepayObligationLiquidityV2Accounts {
            owner: pubkey(owner),
            obligation: pubkey(obligation),
            lending_market: pubkey(&r.lending_market),
            repay_reserve: pubkey(reserve),
            reserve_liquidity_mint: pubkey(&r.liquidity_mint),
            reserve_destination_liquidity: pubkey(&r.supply_vault),
            user_source_liquidity: pubkey(source),
            token_program: pubkey(&r.token_program),
            obligation_farm_user_state: r.farm_debt.map(|farm| pubkey(&obligation_farm(&farm, obligation))),
            reserve_farm_state: r.farm_debt.map(|farm| pubkey(&farm)),
            lending_market_authority: pubkey(&lending_market_authority(&r.lending_market)),
        },
        amount,
        vec![],
    ))
}

/// The liquidator's side of a liquidation.
#[derive(Clone, Copy, Debug)]
pub struct Liquidator {
    pub signer: Address,
    /// Pays the repayment: a token account for the repaid reserve's mint.
    pub source_liquidity: Address,
    /// A token account for the withdrawn reserve's cToken mint.
    pub destination_collateral: Address,
    /// Where klend pays the seized collateral, redeemed: a token account for its liquidity mint.
    pub destination_liquidity: Address,
}

/// `liquidate_obligation_and_redeem_reserve_collateral_v2`. It repays up to `amount` of
/// `repay_reserve`'s debt and seizes `withdraw_reserve`'s collateral, with the withdrawn reserve's
/// collateral farm and the repaid reserve's debt farm when they exist. No LTV override.
pub fn liquidate(svm: &LiteSVM, liquidator: &Liquidator, obligation: &Address, repay_reserve: &Address, withdraw_reserve: &Address, amount: u64, min_received: u64) -> Instruction {
    let repay = reserve_accounts(svm, repay_reserve);
    let withdraw = reserve_accounts(svm, withdraw_reserve);
    let user_state = |farm: Address| pubkey(&obligation_farm(&farm, obligation));
    instruction(klend::instructions::liquidate_obligation_and_redeem_reserve_collateral_v2(
        klend::instructions::LiquidateObligationAndRedeemReserveCollateralV2Accounts {
            liquidator: pubkey(&liquidator.signer),
            obligation: pubkey(obligation),
            lending_market: pubkey(&repay.lending_market),
            lending_market_authority: pubkey(&lending_market_authority(&repay.lending_market)),
            repay_reserve: pubkey(repay_reserve),
            repay_reserve_liquidity_mint: pubkey(&repay.liquidity_mint),
            repay_reserve_liquidity_supply: pubkey(&repay.supply_vault),
            withdraw_reserve: pubkey(withdraw_reserve),
            withdraw_reserve_liquidity_mint: pubkey(&withdraw.liquidity_mint),
            withdraw_reserve_collateral_mint: pubkey(&withdraw.collateral_mint),
            withdraw_reserve_collateral_supply: pubkey(&withdraw.collateral_supply),
            withdraw_reserve_liquidity_supply: pubkey(&withdraw.supply_vault),
            withdraw_reserve_liquidity_fee_receiver: pubkey(&withdraw.fee_vault),
            user_source_liquidity: pubkey(&liquidator.source_liquidity),
            user_destination_collateral: pubkey(&liquidator.destination_collateral),
            user_destination_liquidity: pubkey(&liquidator.destination_liquidity),
            repay_liquidity_token_program: pubkey(&repay.token_program),
            withdraw_liquidity_token_program: pubkey(&withdraw.token_program),
            collateral_obligation_farm_user_state: withdraw.farm_collateral.map(user_state),
            collateral_reserve_farm_state: withdraw.farm_collateral.map(|farm| pubkey(&farm)),
            debt_obligation_farm_user_state: repay.farm_debt.map(user_state),
            debt_reserve_farm_state: repay.farm_debt.map(|farm| pubkey(&farm)),
        },
        amount,
        min_received,
        0,
        vec![],
    ))
}

// ---------------------------------------------------------------- the templates' farm groups

/// A deposit's `farmAccounts`: v2's accounts after the 14 the templates declare.
pub fn deposit_farm_accounts(svm: &LiteSVM, obligation: &Address, reserve: &Address) -> Vec<AccountMeta> {
    let any = Address::default();
    let mut all = deposit(svm, &any, obligation, reserve, &any, 0).accounts;
    all.split_off(DEPOSIT_DECLARED)
}

/// A repayment's `farmAccounts`: v2's accounts after the 9 declared. That is the debt farm pair,
/// the lending market authority and Farms.
pub fn repay_farm_accounts(svm: &LiteSVM, obligation: &Address, reserve: &Address) -> Vec<AccountMeta> {
    let any = Address::default();
    let mut all = repay(svm, &any, obligation, reserve, &any, 0).accounts;
    all.split_off(REPAY_DECLARED)
}

/// A liquidation's `farmAccounts`: v2's accounts after the 20 declared. That is the collateral farm
/// pair, the debt farm pair and Farms.
pub fn liquidation_farm_accounts(svm: &LiteSVM, obligation: &Address, repay_reserve: &Address, withdraw_reserve: &Address) -> Vec<AccountMeta> {
    let any = Address::default();
    let nobody = Liquidator { signer: any, source_liquidity: any, destination_collateral: any, destination_liquidity: any };
    let mut all = liquidate(svm, &nobody, obligation, repay_reserve, withdraw_reserve, 0, 0).accounts;
    all.split_off(LIQUIDATE_DECLARED)
}
```

  - The farm groups are the tails of klend-interface's own v2 instructions, so they cannot drift
    from klend's layout.
  - `Liquidator` and `ReserveAccounts` stay `Copy`.
  - `state::Reserve` is 8.6 KB and copied out of the account data on each read; that is cheap next
    to a transaction.
- [ ] **Step 4: `src/lending.rs`.** Constants, the snapshot, and the Kamino setup recipes:

```rust
//! The lending snapshot, and the setup the Kamino and marginfi scenarios share.
//!
//! `snapshot-lending/` (`scripts/snapshot/manifests/lending.json`) holds, at one slot:
//! - Kamino Lend's main market, with its SOL and USDC reserves, their vaults, collateral mints and
//!   farms;
//! - Scope's price account and Kamino Farms;
//! - marginfi's main group, with its USDC and SOL banks and their oracles;
//! - the Jupiter route `solToUsdc`.

pub const SNAPSHOT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/snapshot-lending");

pub const MARKET: Address = Address::from_str_const("7u3HeHxYDLhnCoErrtycNokbQYbWGzLs6JSDqGAv5PfF");
pub const SOL_RESERVE: Address = Address::from_str_const("d4A2prbA2whesmvHaL88BH6Ewn5N4bTSU2Ze8P6Bc4Q");
pub const USDC_RESERVE: Address = Address::from_str_const("D6q6wuQSrifJKZYpR1M8R4YawnLDtDsMmWM1NbBmgJ59");
pub const SCOPE_PRICES: Address = Address::from_str_const("3t4JZcueEzTbVP6kLxXrL3VpWx45jDer4eqysweBchNH");
/// The Scope entries the reserves read: each reserve's `price_chain` and `twap_chain`.
pub const SOL_SPOT: usize = 3;
pub const SOL_TWAP: usize = 455;
pub const USDC_SPOT: usize = 13;
pub const USDC_TWAP: usize = 456;
pub const USDC_MINT: Address = Address::from_str_const("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
pub const MARGINFI_GROUP: Address = Address::from_str_const("4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8");
pub const USDC_BANK: Address = Address::from_str_const("2s37akK2eyBbp8DZgCm7RtsaEz8eJP3Nxd4urLHQv7yB");
pub const SOL_BANK: Address = Address::from_str_const("CCKtUs6Cgwo4aaQUmBPmyoApH2gUDErxNZCAntD6LYGh");
/// Both swap templates' route: 1 SOL for USDC, built for [`wallet::wallet`].
pub const SOL_TO_USDC: &str = "solToUsdc";
/// Uploads every template, as template 1.
pub const CREATOR_SEED: &[u8; 32] = b"ballista-protocol-tests-creator1";
/// `route`'s leading accounts, which a template passes itself (`JUPITER_ROUTE_FIXED_ACCOUNTS`).
const ROUTE_FIXED_ACCOUNTS: usize = 4;

/// The snapshot, loaded and checked once per test binary.
pub fn snapshot() -> &'static Snapshot {
    static SNAPSHOT: OnceLock<Snapshot> = OnceLock::new();
    SNAPSHOT.get_or_init(|| Snapshot::load(SNAPSHOT_DIR))
}

/// A LiteSVM at the snapshot. The Scope entries the reserves read are stamped with the snapshot's
/// clock (write rule 2; prices unchanged). Each is then fresh, however long before the snapshot
/// Scope last updated it: Kamino's SOL window is 120 seconds.
pub fn svm() -> LiteSVM {
    let mut svm = snapshot().clone().into_svm();
    let clock = svm.get_sysvar::<Clock>();
    let now = u64::try_from(clock.unix_timestamp).expect("the clock is after 1970");
    for index in [SOL_SPOT, SOL_TWAP, USDC_SPOT, USDC_TWAP] {
        let price = oracle::scope_price(&svm, &SCOPE_PRICES, index);
        oracle::set_scope_price(&mut svm, &SCOPE_PRICES, index, price.value, clock.slot, now);
    }
    svm
}

/// One slot on, and no time. Call it before every template transaction.
/// - LiteSVM never advances the slot by itself, and klend counts a reserve or obligation as fresh
///   for the rest of the slot it was refreshed in. Without this, a refresh sent during setup would
///   hide a template transaction that forgot its own.
/// - The time stays put, so no interest accrues between setup and the run, and no price ages.
pub fn next_slot(svm: &mut LiteSVM) {
    snapshot::warp(svm, 1, 0);
}

/// `SetComputeUnitLimit(1,400,000)`. A run that swaps and then calls Kamino needs more than the
/// default.
pub fn compute_limit() -> Instruction {
    ComputeBudgetInstruction::set_compute_unit_limit(1_400_000)
}

/// Sends a setup transaction behind [`compute_limit`], and panics if it fails.
pub fn setup(svm: &mut LiteSVM, payer: &Keypair, signers: &[&Keypair], instructions: Vec<Instruction>) -> Outcome {
    let mut all = vec![compute_limit()];
    all.extend(instructions);
    tx::send(svm, payer, signers, &all, &[]).unwrap_or_else(|failure| panic!("setup failed: {failure:?}"))
}

/// Uploads `example` as the tests' creator's template 1.
pub fn upload(svm: &mut LiteSVM, example: &Example) -> Address {
    let creator = wallet::keypair(CREATOR_SEED);
    wallet::fund(svm, &creator.pubkey(), 10 * SOL);
    template::upload(svm, &creator, 1, &example.payload)
}

/// `route`'s own accounts, after the four a template passes itself: the `routeAccounts` group.
pub fn route_group(leg: &Leg) -> Vec<AccountMeta> {
    leg.instructions.swap.accounts[ROUTE_FIXED_ACCOUNTS..].to_vec()
}

/// What `leg` pays into its destination when it runs alone, from `svm`'s state, in the next slot:
/// the slot a template transaction runs in. `svm` is left as it was.
pub fn swap_output(svm: &LiteSVM, leg: &Leg) -> u64 {
    let mut alone = svm.clone();
    next_slot(&mut alone);
    let before = wallet::token_balance(&alone, &leg.destination_token_account);
    tx::send(&mut alone, &wallet::wallet(), &[], &[compute_limit(), leg.instructions.swap.clone()], &leg.lookup_tables)
        .unwrap_or_else(|failure| panic!("the route alone failed: {failure:?}"));
    wallet::token_balance(&alone, &leg.destination_token_account) - before
}

/// Opens `owner`'s user metadata and vanilla obligation in the main market, plus the obligation's
/// user state in each of `farmed`'s collateral farms. Returns the obligation.
pub fn open_obligation(svm: &mut LiteSVM, owner: &Keypair, farmed: &[Address]) -> Address {
    let o = owner.pubkey();
    let obligation = kamino::vanilla_obligation(&o, &MARKET);
    let mut instructions = vec![kamino::init_user_metadata(&o), kamino::init_obligation(&o, &MARKET)];
    instructions.extend(farmed.iter().map(|reserve| kamino::init_obligation_farm(svm, &o, &o, &obligation, reserve)));
    setup(svm, owner, &[], instructions);
    obligation
}

/// Deposits `amount` of `reserve`'s liquidity from `source`, behind the refreshes it needs.
pub fn deposit(svm: &mut LiteSVM, owner: &Keypair, obligation: &Address, reserve: &Address, source: &Address, amount: u64) {
    let mut instructions = kamino::refreshes(svm, obligation, &[*reserve]);
    instructions.push(kamino::deposit(svm, &owner.pubkey(), obligation, reserve, source, amount));
    setup(svm, owner, &[], instructions);
}

/// Borrows `amount` of `reserve`'s liquidity into `destination`, behind the refreshes it needs:
/// every held reserve and the borrowed one, then the obligation.
pub fn borrow(svm: &mut LiteSVM, owner: &Keypair, obligation: &Address, reserve: &Address, destination: &Address, amount: u64) {
    let mut instructions = kamino::refreshes(svm, obligation, &[*reserve]);
    instructions.push(kamino::borrow(svm, &owner.pubkey(), obligation, reserve, destination, amount));
    setup(svm, owner, &[], instructions);
}
```

  - Its imports:
    - `crate::{kamino, oracle, snapshot::{self, Leg, Snapshot}, template::{self, Example}, tx::{self, Outcome}, wallet::{self, SOL}}`;
    - `litesvm::LiteSVM`, `solana_address::Address`, `solana_clock::Clock`;
    - `solana_compute_budget_interface::ComputeBudgetInstruction`;
    - `solana_instruction::{AccountMeta, Instruction}`, `solana_keypair::Keypair`,
      `solana_signer::Signer`, `std::sync::OnceLock`.

  - Add these tests to `lending.rs`:
    1. `the_snapshot_holds_everything_the_reserves_name`:
       - for both reserves, every address `kamino::reserve_accounts` returns is in the SVM;
       - `lending_market == MARKET`, `scope_prices == Some(SCOPE_PRICES)`, `farm_debt == None`;
       - if `Snapshot::named` has landed, each constant equals the manifest's name for it, for
         example `USDC_RESERVE == snapshot().named("kaminoUsdcReserve")`. The Rust side then cannot
         drift from `lending.json`.
    2. `svm_stamps_the_scope_prices_with_the_snapshot_clock`: each of the four entries has
       `unix_timestamp == clock` and `exp == 8`.
    3. `a_kamino_user_deposits_sol_and_borrows_usdc`, the probe's setup:
       - 10 SOL deposited from a wSOL ATA, then 300 USDC borrowed;
       - the USDC ATA holds 300,000,000;
       - `deposited(SOL) > 0`;
       - `borrowed_sf(USDC) >> 60 == 300_000_000`.
    4. `the_route_pays_the_same_every_time`: `swap_output` twice gives the same amount, at least
       `leg.other_amount_threshold`. This is what the swap templates' exact assertions rest on.
  - Add these tests to `kamino.rs`:
    1. `the_reserves_vaults_are_stored_not_derived`:
       - `reserve_accounts(USDC).supply_vault` is `Bgq7trRgVMeq33yt235zM2onQ4bRDBsY5EWiTetF4qw6`;
       - `klend::pda::ReservePdas::derive(&klend::KLEND_PROGRAM_ID, &pubkey(&USDC_RESERVE)).liquidity_supply_vault`
         is not.
    2. `the_farm_groups_are_klends_v2_tails`, for an arbitrary obligation address `ob`:
       - the USDC deposit group is `[new(obligation_farm(usdc_farm, ob)), new(usdc_farm), readonly(FARMS)]`;
       - the USDC repay group is `[readonly(KLEND), readonly(KLEND), readonly(lending_market_authority(MARKET)), readonly(FARMS)]`;
       - the liquidation group (repay USDC, withdraw SOL) is
         `[new(obligation_farm(sol_farm, ob)), new(sol_farm), readonly(KLEND), readonly(KLEND), readonly(FARMS)]`.
- [ ] **Step 5:** add `pub mod kamino; pub mod lending;` to `lib.rs` and a line for each in its
  module list. Then:

```bash
cargo test --manifest-path tests/protocols/Cargo.toml --lib
```

  All pass, milestone 1's tests included. Commit "Add Kamino and Scope to the protocol-test
  harness", including `Cargo.lock`.

### Task 3: marginfi in the harness

- [ ] **Step 1: A failing offsets test.** In a new `src/marginfi.rs`, write
  `the_offsets_read_the_snapshots_banks`. It expects:
  - `bank(USDC_BANK) == Bank { mint: USDC_MINT, group: MARGINFI_GROUP, liquidity_vault: 7jaiZR5Sk8hdYN9MxTpczTcwbWpb5WEoxSANuUwveuat, oracle_setup: PYTH_PUSH_ORACLE, oracle: 6HAuqASbHEh4w4REJEUUUCginTLfj1kwCh215ZLtMkrT }`;
  - `bank(SOL_BANK) == { WSOL_MINT, MARGINFI_GROUP, 2eicbpitfJXDwqCuFAmPgDP7t2oUotnAzbGzRKLMgSLe, PYTH_PUSH_ORACLE, 7AviUf9nL62mcxNbQGKm4nKDQnPjswo6c5MX4D57HmyE }`;
  - `vault_authority(USDC_BANK) == 3uxNepDbmkDNq6JhRja5Z8QwbTrfmkKP8AKZV5chYDGG` and
    `vault_authority(SOL_BANK) == DD3AeAssFvjqTvRTrRAtpfjkBF8FpVKnFuwnMLN9haXD`;
  - each bank's vault and oracle is in the SVM.

  It fails to compile.
- [ ] **Step 2: The module.**

```rust
//! marginfi v2: the instructions setup and the contract tests send, and the account fields the
//! scenarios read. Everything here is written from the deployed program's source
//! (`mrgnlabs/marginfi-v2@33c67987a6`, 0.1.11-rc1; research §3.2 and appendix A).
//!
//! No crate for it:
//! - marginfi publishes no current instruction crate.
//! - Its type crate cannot be a dependency here: at that commit it pins
//!   `solana-instruction = "=3.4.0"`, and LiteSVM needs `~3.5`.
//!
//! The offsets below were printed by `offset_of!` against the type crate.
//! `the_offsets_read_the_snapshots_banks` checks them against the real accounts, so a layout
//! change fails there first.

use {
    ballista_sdk::{SYSTEM_PROGRAM_ID, TOKEN_PROGRAM_ID},
    litesvm::LiteSVM,
    solana_address::Address,
    solana_instruction::{AccountMeta, Instruction},
};

pub const MARGINFI: Address = Address::from_str_const("MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA");
/// `oracle_setup` of a bank priced by one Pyth `PriceUpdateV2`, `oracle_keys[0]`.
pub const PYTH_PUSH_ORACLE: u8 = 3;

/// `marginfi_account_initialize`, `lending_account_deposit`, `lending_account_withdraw`.
const INITIALIZE: [u8; 8] = [0x2b, 0x4e, 0x3d, 0xff, 0x94, 0x34, 0xf9, 0x9a];
const DEPOSIT: [u8; 8] = [0xab, 0x5e, 0xeb, 0x67, 0x52, 0x40, 0xd4, 0x8c];
const WITHDRAW: [u8; 8] = [0x24, 0x48, 0x4a, 0x13, 0xd2, 0xd2, 0xc0, 0xc0];

/// `Bank`, 1,864 bytes.
const BANK_LEN: usize = 1_864;
const BANK_MINT: usize = 8;
const BANK_GROUP: usize = 41;
const BANK_LIQUIDITY_VAULT: usize = 112;
const BANK_ORACLE_SETUP: usize = 609;
const BANK_ORACLE_KEYS: usize = 610;
/// `MarginfiAccount`, 2,312 bytes: 16 balances of 104 bytes from 72, with `active` at +0 and
/// `bank_pk` at +1.
const ACCOUNT_LEN: usize = 2_312;
const BALANCES: usize = 72;
const BALANCE_LEN: usize = 104;
const BALANCE_COUNT: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bank {
    pub mint: Address,
    pub group: Address,
    pub liquidity_vault: Address,
    pub oracle_setup: u8,
    /// `oracle_keys[0]`: the only oracle account the banks this suite uses have.
    pub oracle: Address,
}

pub fn bank(svm: &LiteSVM, address: &Address) -> Bank {
    let data = owned_data(svm, address, BANK_LEN);
    Bank {
        mint: key_at(&data, BANK_MINT),
        group: key_at(&data, BANK_GROUP),
        liquidity_vault: key_at(&data, BANK_LIQUIDITY_VAULT),
        oracle_setup: data[BANK_ORACLE_SETUP],
        oracle: key_at(&data, BANK_ORACLE_KEYS),
    }
}

/// The banks `account` holds an active balance in, in the account's own order.
pub fn active_banks(svm: &LiteSVM, account: &Address) -> Vec<Address> {
    let data = owned_data(svm, account, ACCOUNT_LEN);
    (0..BALANCE_COUNT)
        .map(|index| BALANCES + index * BALANCE_LEN)
        .filter(|&at| data[at] != 0)
        .map(|at| key_at(&data, at + 1))
        .collect()
}

/// The PDA that owns `bank`'s liquidity vault: `["liquidity_vault_auth", bank]`.
pub fn vault_authority(bank: &Address) -> Address {
    Address::find_program_address(&[b"liquidity_vault_auth", bank.as_ref()], &MARGINFI).0
}

/// `marginfi_account_initialize`: `account` is a new keypair, and signs.
pub fn initialize_account(group: &Address, account: &Address, authority: &Address, fee_payer: &Address) -> Instruction {
    Instruction {
        program_id: MARGINFI,
        accounts: vec![
            AccountMeta::new_readonly(*group, false),
            AccountMeta::new(*account, true),
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*fee_payer, true),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        ],
        data: INITIALIZE.to_vec(),
    }
}

/// `lending_account_deposit(amount, deposit_up_to_limit: None)`. It needs no remaining accounts
/// and no oracle.
pub fn deposit(svm: &LiteSVM, group: &Address, account: &Address, authority: &Address, bank: &Address, source: &Address, amount: u64) -> Instruction {
    let mut data = DEPOSIT.to_vec();
    data.extend_from_slice(&amount.to_le_bytes());
    data.push(0); // Option::None
    Instruction {
        program_id: MARGINFI,
        accounts: vec![
            AccountMeta::new_readonly(*group, false),
            AccountMeta::new(*account, false),
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*bank, false),
            AccountMeta::new(*source, false),
            AccountMeta::new(self::bank(svm, bank).liquidity_vault, false),
            AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        ],
        data,
    }
}

/// `lending_account_withdraw(0, withdraw_all: Some(true))`, followed by `remaining`.
pub fn withdraw_all(svm: &LiteSVM, group: &Address, account: &Address, authority: &Address, bank: &Address, destination: &Address, remaining: Vec<AccountMeta>) -> Instruction {
    let mut data = WITHDRAW.to_vec();
    data.extend_from_slice(&0u64.to_le_bytes());
    data.extend_from_slice(&[1, 1]); // Option::Some(true)
    let mut accounts = vec![
        AccountMeta::new_readonly(*group, false),
        AccountMeta::new(*account, false),
        AccountMeta::new_readonly(*authority, true),
        AccountMeta::new(*bank, false),
        AccountMeta::new(*destination, false),
        AccountMeta::new_readonly(vault_authority(bank), false),
        AccountMeta::new(self::bank(svm, bank).liquidity_vault, false),
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
    ];
    accounts.extend(remaining);
    Instruction { program_id: MARGINFI, accounts, data }
}

/// What marginfi's health check reads once `withdrawn` is emptied. For every other balance the
/// account holds, it takes the bank and then its oracle, by bank address from highest to lowest,
/// as `sort_balances` leaves them. This is the `healthAccounts` group; it is empty when `withdrawn`
/// was the only balance.
///
/// # Panics
///
/// If a bank is priced any other way than by one Pyth account. Staked, Kamino and other setups
/// take more accounts per bank.
pub fn health_accounts(svm: &LiteSVM, account: &Address, withdrawn: &Address) -> Vec<AccountMeta> {
    let mut banks: Vec<Address> = active_banks(svm, account).into_iter().filter(|bank| bank != withdrawn).collect();
    banks.sort_by(|a, b| b.as_ref().cmp(a.as_ref()));
    banks
        .iter()
        .flat_map(|address| {
            let bank = bank(svm, address);
            assert_eq!(bank.oracle_setup, PYTH_PUSH_ORACLE, "bank {address} is not priced by one Pyth account");
            [AccountMeta::new_readonly(*address, false), AccountMeta::new_readonly(bank.oracle, false)]
        })
        .collect()
}

fn owned_data(svm: &LiteSVM, address: &Address, len: usize) -> Vec<u8> {
    let account = svm.get_account(address).unwrap_or_else(|| panic!("{address} is not in the SVM"));
    assert!(account.owner == MARGINFI && account.data.len() == len, "{address} is not the marginfi account this reads");
    account.data
}

fn key_at(data: &[u8], offset: usize) -> Address {
    Address::new_from_array(data[offset..offset + 32].try_into().unwrap())
}
```

- [ ] **Step 3: A setup recipe in `lending.rs`,** and its test:

```rust
/// Opens the marginfi account `account` for `authority` in the main group, and deposits each
/// `(bank, source token account, amount)`. Returns the account's address.
pub fn marginfi_account(svm: &mut LiteSVM, authority: &Keypair, account: &Keypair, deposits: &[(Address, Address, u64)]) -> Address {
    let a = authority.pubkey();
    let mut instructions = vec![marginfi::initialize_account(&MARGINFI_GROUP, &account.pubkey(), &a, &a)];
    for (bank, source, amount) in deposits {
        instructions.push(marginfi::deposit(svm, &MARGINFI_GROUP, &account.pubkey(), &a, bank, source, *amount));
    }
    setup(svm, authority, &[account], instructions);
    account.pubkey()
}
```

  The test is `a_marginfi_account_takes_deposits_in_two_banks`:
  - 100 USDC and 1 SOL go in (from ATAs written by rule 1);
  - `active_banks` then holds both banks;
  - `health_accounts(account, USDC_BANK) == [readonly(SOL_BANK), readonly(SOL oracle)]`.
- [ ] **Step 4:** add `pub mod marginfi;` to `lib.rs`, run `cargo test --manifest-path tests/protocols/Cargo.toml --lib`,
  and commit "Add marginfi to the protocol-test harness".

### Task 4: What klend and marginfi require of a caller

These tests pin, against the real programs, the rules the templates broke. They pass as soon as
they are written: they characterize the programs, and the template tasks then show each template
following them. Each asserts the failing program and code. The probe already ran every one of
them.

- [ ] **Step 1: The liquidation scenario, in `lending.rs`.** Test 6 below and Task 8 share it;
  it is the probe's `liquidation_pays_liquidity_not_ctokens`, made reusable.

```rust
/// An obligation klend will liquidate:
/// - Its borrower (seed `ballista-protocol-tests-borrower`) deposited 1 SOL and borrowed 70% of its
///   value in USDC. The SOL reserve's loan-to-value is 74%.
/// - Then SOL's Scope spot and TWAP were both lowered 12% and stamped now (rule 2). That puts the
///   debt at about 80% of the collateral, past the 75% liquidation threshold. Moving the TWAP too
///   keeps every price check passing.
pub struct Unhealthy {
    pub obligation: Address,
    /// USDC base units borrowed.
    pub debt: u64,
    /// The prices it was left at: Scope values with exponent 8.
    pub sol_price: u64,
    pub usdc_price: u64,
}

pub fn unhealthy_obligation(svm: &mut LiteSVM) -> Unhealthy {
    let borrower = wallet::keypair(b"ballista-protocol-tests-borrower");
    let b = borrower.pubkey();
    wallet::fund(svm, &b, 10 * SOL);
    let collateral = wallet::token_account(svm, &b, &wallet::WSOL_MINT, SOL);
    let proceeds = wallet::token_account(svm, &b, &USDC_MINT, 0);
    let obligation = open_obligation(svm, &borrower, &[SOL_RESERVE]);
    deposit(svm, &borrower, &obligation, &SOL_RESERVE, &collateral, SOL);

    let sol = oracle::scope_price(svm, &SCOPE_PRICES, SOL_SPOT);
    let usdc = oracle::scope_price(svm, &SCOPE_PRICES, USDC_SPOT);
    assert_eq!((sol.exp, usdc.exp), (8, 8), "Scope prices with exponent 8");
    // 70% of one SOL in USDC base units: value / 10^8 dollars, times 10^6, times 0.7.
    let debt = sol.value * 7 / 1_000;
    borrow(svm, &borrower, &obligation, &USDC_RESERVE, &proceeds, debt);

    let fallen = sol.value * 88 / 100;
    let clock = svm.get_sysvar::<Clock>();
    let now = u64::try_from(clock.unix_timestamp).expect("the clock is after 1970");
    for index in [SOL_SPOT, SOL_TWAP] {
        oracle::set_scope_price(svm, &SCOPE_PRICES, index, fallen, clock.slot, now);
    }
    Unhealthy { obligation, debt, sol_price: fallen, usdc_price: usdc.value }
}

/// A liquidator (seed `ballista-protocol-tests-liquid-1`) holding `usdc` in its USDC account, with
/// empty accounts for the SOL reserve's cTokens and for wrapped SOL (rule 1).
pub fn liquidator(svm: &mut LiteSVM, usdc: u64) -> (Keypair, kamino::Liquidator) {
    let liquidator = wallet::keypair(b"ballista-protocol-tests-liquid-1");
    let l = liquidator.pubkey();
    wallet::fund(svm, &l, 10 * SOL);
    let collateral_mint = kamino::reserve_accounts(svm, &SOL_RESERVE).collateral_mint;
    let accounts = kamino::Liquidator {
        signer: l,
        source_liquidity: wallet::token_account(svm, &l, &USDC_MINT, usdc),
        destination_collateral: wallet::token_account(svm, &l, &collateral_mint, 0),
        destination_liquidity: wallet::token_account(svm, &l, &wallet::WSOL_MINT, 0),
    };
    (liquidator, accounts)
}

/// `repaid` USDC base units, in lamports, at the given Scope prices (exponent 8). SOL has 9
/// decimals and USDC 6, hence the 1,000.
pub fn break_even(repaid: u64, usdc_price: u64, sol_price: u64) -> u64 {
    u64::try_from(u128::from(repaid) * u128::from(usdc_price) * 1_000 / u128::from(sol_price)).unwrap()
}
```

- [ ] **Step 2: `tests/protocols/tests/kamino_contract.rs`.**

```rust
//! What Kamino Lend requires of a caller, against the deployed program. Each test pins a rule one
//! of the templates once broke.

/// The smallest Ballista caller: it forwards the `data` input to `target`, with its signer first
/// and its one account group after.
fn forward_template(target: Address) -> Vec<u8> {
    let mut builder = ProgramBuilder::new();
    let program = builder.account(ACCOUNT_EXECUTABLE, Some(target.to_bytes()), None, 0);
    let signer = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
    builder.account_groups(1);
    let data_input = builder.input(VALUE_BYTES, 512);
    let data = builder.load_input(data_input);
    let cpi = builder.cpi_with_group(
        program,
        &[(signer, ACCOUNT_SIGNER | ACCOUNT_WRITABLE)],
        &[Segment::Register(DATA_REG_BYTES, data)],
        0,
    );
    builder.set_cpi_max_data_len(cpi, 512);
    builder.invoke(cpi, None);
    builder.build().expect("the forwarding template builds")
}

/// `call` run through the forwarding template: its first account signs through the declared
/// slot, and the rest travel as the group with their own writable flags.
fn forward(template: Address, call: &Instruction) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new_readonly(call.program_id, false),
        AccountMeta::new(call.accounts[0].pubkey, true),
    ];
    accounts.extend(call.accounts[1..].iter().map(|meta| AccountMeta { is_signer: false, ..meta.clone() }));
    let inputs = RunInputs::new().groups(&[(call.accounts.len() - 1) as u8]).bytes(&call.data).finish();
    run_instruction(template, accounts, &inputs)
}

fn refused(failure: &Failure, code: u32) {
    assert_eq!((failure.program, failure.code), (kamino::KLEND, Some(code)), "{failure:?}");
}
```

  - Imports:
    - `ballista_protocol_tests::{kamino, lending::{self, MARKET, SOL_RESERVE, USDC_MINT, USDC_RESERVE}, template, tx::{self, Failure}, wallet::{self, SOL, WSOL_MINT}}`;
    - `ballista_sdk::{ballista_common::template::{ACCOUNT_EXECUTABLE, ACCOUNT_SIGNER, ACCOUNT_WRITABLE, DATA_REG_BYTES, VALUE_BYTES}, run_instruction, ProgramBuilder, RunInputs, Segment}`;
    - `solana_address::Address`, `solana_instruction::{AccountMeta, Instruction}`,
      `solana_signer::Signer`.
    The probe already built this exact `forward_template` and ran it.
  - Each test below builds its own SVM with `lending::svm()`, then:
    - funds its user (seed `b"ballista-protocol-tests-usdc-usr"`);
    - writes 100 USDC to the user's ATA (rule 1);
    - calls `lending::open_obligation(.., &[USDC_RESERVE])`;
    - moves on with `lending::next_slot` before the transaction under test.
  - The tests:
    1. `v1_handlers_refuse_a_ballista_caller_and_v2_handlers_do_not`:
       - upload `forward_template(kamino::KLEND)` as template 9 with `template::upload`, from
         `wallet::keypair(lending::CREATOR_SEED)` funded with 10 SOL;
       - `[compute_limit, refreshes(ob, [USDC]), forward(kamino::deposit_v1(..))]` is refused with
         6080 (`CpiDisabled`);
       - the same with `kamino::deposit(..)` lands, and the user's USDC falls by the amount.
    2. `refresh_reserve_takes_six_accounts_with_scope_last`, calling `refresh_reserve` directly on
       the SOL reserve. Build each variant from the first instruction of
       `kamino::refreshes(&svm, &ob, &[SOL_RESERVE])`, SOL's own `refresh_reserve`, by replacing its
       accounts, so no discriminator is copied by hand:
       - `[reserve, market, scope]`, the template's old list: 3005 (`AccountNotEnoughKeys`);
       - `[reserve, market, scope, KLEND, KLEND, KLEND]`, Scope in the Pyth slot: 6054
         (`InvalidPythPriceAccount`);
       - `[reserve, market, KLEND, KLEND, KLEND]`: 3005. A missing trailing optional is not "none".
       - `kamino::refreshes`'s own six-account `refresh_reserve` lands.
    3. `refresh_obligation_takes_every_reserve_refreshed_in_the_slot`, on an obligation that holds
       SOL collateral and a USDC borrow (`lending::deposit` and `lending::borrow` in setup, then
       `next_slot`):
       - `kamino::refreshes(..)` with its last instruction, `refresh_obligation`, cut to its first
         two accounts: 6006 (`InvalidAccountInput`);
       - that `refresh_obligation` whole, but alone, so no reserve was refreshed this slot: 6009
         (`ReserveStale`);
       - `kamino::refreshes(..)` in full lands.
    4. `a_deposit_into_a_farmed_reserve_needs_the_farm_accounts`: `kamino::deposit` into USDC
       with accounts 14 and 15 replaced by `AccountMeta::new_readonly(KLEND, false)`, behind its
       refreshes: 6120 (`FarmAccountsMissing`).
    5. `a_deposit_needs_the_obligation_refreshed_in_its_slot`: `kamino::deposit` alone, with no
       refreshes: 6017 (`ObligationStale`).
    6. `a_liquidation_pays_liquidity_and_keeps_no_ctokens`, using `lending::unhealthy_obligation`
       and `lending::liquidator` (Step 1). `kamino::liquidate` at the top level, behind its
       refreshes, repays `debt / 10`. Afterwards:
       - the liquidator's cToken account is unchanged at 0;
       - its wSOL account holds more than `lending::break_even(repaid, ..)`.
- [ ] **Step 3: `tests/protocols/tests/marginfi_contract.rs`.** Setup:
  - `lending::marginfi_account` for authority `b"ballista-protocol-tests-mfi-auth"` and account
    `b"ballista-protocol-tests-mfi-acct"`;
  - then `next_slot`.

  The tests:
  1. `withdrawing_a_sole_balance_needs_no_remaining_accounts`: 100 USDC deposited alone.
     `marginfi::withdraw_all(.., vec![])` lands; the ATA gets 99,999,999 or 100,000,000 back; and
     `active_banks` is empty.
  2. `a_withdrawal_that_leaves_a_balance_needs_that_balances_bank`: 100 USDC and 1 SOL deposited.
     - `withdraw_all(USDC, vec![])` is refused by marginfi with 6008 (`InvalidBankAccount`).
     - `withdraw_all(USDC, health_accounts(account, USDC_BANK))` lands, and SOL is still active.
- [ ] **Step 4:**

```bash
cargo test --manifest-path tests/protocols/Cargo.toml --test kamino_contract --test marginfi_contract
```

  All pass. Commit "Pin down what Kamino and marginfi require of a caller".

### Task 5: Name the requirement a run failed at

Milestone 1's Task 5 records `labels` in the fixture (`{ "<pc>": "<label>" }` per entry, from
`compiled.sourceMap`), reads them in `template.rs`, and adds `tx::assert_requirement_failed`.
Use what it has landed (Task 0 Step 3). Add only the steps it has not covered, under its names.

- [ ] **Step 1: A failing test** in `template.rs`, `a_label_names_its_steps_program_counters`:
  - `jupiterDepositExactOutput`'s `labels` include `swapMetItsFloor` and `depositSwapOutput`;
  - `label_at` returns `Some("swapMetItsFloor")` for each counter recorded under that label, and
    `None` for `u16::MAX`.

  It fails to compile.
- [ ] **Step 2: Only if the fixture has no `labels`:** record them. In
  `clients/js/src/protocol-examples.test.ts`, add to each entry:

```ts
            // Program counter to step label. A labelled step spans several instructions, so a
            // failure's pc names its step, never the other way round.
            labels: Object.fromEntries(
              compiled.sourceMap.filter((entry) => entry.label !== undefined).map((entry) => [entry.pc, entry.label]),
            ),
```

- [ ] **Step 3: Read them,** unless milestone 1 already did. In `template.rs`:

```rust
    /// The label of the step each program counter belongs to, for the steps that have one.
    #[serde(default)]
    pub labels: BTreeMap<u16, String>,
```

```rust
impl Example {
    /// The label of the step at `pc`, which is the context of a `RequirementFailed`.
    pub fn label_at(&self, pc: u16) -> Option<&str> {
        self.labels.get(&pc).map(String::as_str)
    }
}
```

  and, only if milestone 1's did not land, in `tx.rs`, under its name:

```rust
/// Asserts that Ballista itself refused the run, at the `require` step labelled `label`.
pub fn assert_requirement_failed(failure: &Failure, example: &Example, label: &str) {
    let (name, pc) = ballista_error(failure)
        .unwrap_or_else(|| panic!("{} failed, not Ballista: {failure:?}", failure.program));
    assert_eq!(name, "RequirementFailed", "{failure:?}");
    assert_eq!(example.label_at(pc), Some(label), "refused at pc {pc}: {failure:?}");
}
```

- [ ] **Step 4:** Run:

```bash
pnpm fixtures
pnpm --dir clients/js test
cargo test -p ballista-common every_shared_fixture_parses_and_verifies
cargo test --manifest-path tests/protocols/Cargo.toml --lib
```

  `payload_values` reads `"payload":` explicitly, so the new strings do not disturb it.
  serde_json reads the string keys `"2"`, `"3"`, … into `BTreeMap<u16, _>` directly.
  - If milestone 1 had already done all of this, skip the task: there is nothing to commit.
  - Otherwise commit "Name the step a failed protocol run stopped at".

### Task 6: `jupiterDepositExactOutput`

- [ ] **Step 1: The scenario tests,** in `tests/protocols/tests/jupiter_deposit_exact_output.rs`.

```rust
//! `jupiterDepositExactOutput` against Jupiter and Kamino: sell 1 SOL through the snapshotted
//! route, and deposit exactly what it paid into Kamino's USDC reserve.

const NAME: &str = "jupiterDepositExactOutput";
const JUPITER: Address = Address::from_str_const("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");

struct Scene {
    owner: Keypair,
    leg: Leg,
    obligation: Address,
    template: Address,
}

/// Sets up the wallet the route was built for:
/// - it holds exactly the wrapped SOL the route sells, and an empty USDC account for the route to
///   pay into (rule 1);
/// - it has a Kamino obligation with its USDC farm user state.
fn scene() -> (LiteSVM, Scene) {
    let leg = lending::snapshot().route(SOL_TO_USDC).legs[0].clone();
    let mut svm = lending::svm();
    let owner = wallet::wallet();
    let o = owner.pubkey();
    wallet::fund(&mut svm, &o, 10 * SOL);
    assert_eq!(wallet::token_account(&mut svm, &o, &WSOL_MINT, leg.in_amount), leg.source_token_account);
    assert_eq!(wallet::token_account(&mut svm, &o, &USDC_MINT, 0), leg.destination_token_account);
    let obligation = lending::open_obligation(&mut svm, &owner, &[USDC_RESERVE]);
    let template = lending::upload(&mut svm, &template::examples()[NAME]);
    (svm, Scene { owner, leg, obligation, template })
}

fn run(svm: &LiteSVM, scene: &Scene, minimum_out: u64) -> Instruction {
    let examples = template::examples();
    let usdc = kamino::reserve_accounts(svm, &USDC_RESERVE);
    Run::new(scene.template, &examples[NAME])
        .account("jupiter", JUPITER, false, false)
        .account("kamino", kamino::KLEND, false, false)
        .account("tokenProgram", TOKEN_PROGRAM_ID, false, false)
        .account("instructionsSysvar", kamino::INSTRUCTIONS_SYSVAR, false, false)
        .account("owner", scene.owner.pubkey(), true, true)
        .account("sourceAta", scene.leg.source_token_account, true, false)
        .account("destinationAta", scene.leg.destination_token_account, true, false)
        .account("obligation", scene.obligation, true, false)
        .account("lendingMarket", MARKET, false, false)
        .account("lendingMarketAuthority", kamino::lending_market_authority(&MARKET), false, false)
        .account("reserve", USDC_RESERVE, true, false)
        .account("reserveLiquidityMint", usdc.liquidity_mint, false, false)
        .account("reserveLiquiditySupply", usdc.supply_vault, true, false)
        .account("reserveCollateralMint", usdc.collateral_mint, true, false)
        .account("reserveDestinationDepositCollateral", usdc.collateral_supply, true, false)
        .input_bytes("routeArgs", &scene.leg.route.args)
        .input_u64("minimumOut", minimum_out)
        .group("routeAccounts", lending::route_group(&scene.leg))
        .group("farmAccounts", kamino::deposit_farm_accounts(svm, &scene.obligation, &USDC_RESERVE))
        .build()
}

/// The run in a new slot, behind the refreshes klend needs in that slot. The deposit refreshes its
/// own reserve too; refreshing it here as well is what the probe measured, and is harmless.
fn send_run(svm: &mut LiteSVM, scene: &Scene, run: Instruction) -> Result<Outcome, Failure> {
    lending::next_slot(svm);
    let mut instructions = vec![lending::compute_limit()];
    instructions.extend(kamino::refreshes(svm, &scene.obligation, &[USDC_RESERVE]));
    instructions.push(run);
    tx::send(svm, &scene.owner, &[], &instructions, &scene.leg.lookup_tables)
}
```

  - Imports:
    - `ballista_protocol_tests::{kamino, lending::{self, MARKET, SOL_TO_USDC, USDC_MINT, USDC_RESERVE}, snapshot::Leg, template::{self, Run}, tx::{self, Failure, Outcome}, wallet::{self, SOL, WSOL_MINT}}`;
    - `ballista_sdk::TOKEN_PROGRAM_ID`, `litesvm::LiteSVM`, `solana_address::Address`,
      `solana_instruction::Instruction`, `solana_keypair::Keypair`, `solana_signer::Signer`.
  - The other template tests (Tasks 7 to 10) follow this file's shape:
    - `scene() -> (LiteSVM, Scene)`;
    - `run(&svm, &scene, ..) -> Instruction`, bound by name;
    - `send_run(&mut svm, &scene, run)`.
    Keeping the SVM outside `Scene` lets a test run the same instruction on `svm.clone()` first.

  The tests:
  1. `deposits_exactly_what_the_swap_produced`:
     - `produced = lending::swap_output(&svm, &scene.leg)`, read before `send_run`: the replay
       runs in the same next slot;
     - run with `minimumOut = produced`, the tightest floor that passes.
     - Then assert:
       - the USDC reserve's supply vault rose by exactly `produced`;
       - the destination ATA holds 0: nothing stranded;
       - the source wSOL account holds 0;
       - the cTokens minted (the rise in `collateral_supply`) are more than 0 and equal
         `kamino::deposited(obligation, USDC_RESERVE)`.
     - `eprintln!("{NAME}: {} CU, {} bytes", outcome.compute_units, outcome.size)`.
  2. `a_floor_above_the_fill_refuses_at_swap_met_its_floor`:
     - `minimumOut = produced + 1`;
     - `tx::assert_requirement_failed(&failure, &examples[NAME], "swapMetItsFloor")`;
     - the supply vault is unchanged, and `deposited == 0`.
  3. `a_setup_refresh_hides_a_missing_refresh_until_the_slot_moves`. This is why every template
     transaction starts in a new slot:
     - send `kamino::refreshes(ob, [])` as setup;
     - on `svm.clone()`, send `[compute_limit, run(.., 1)]`, with no refreshes of its own, in the
       same slot: it lands;
     - after `lending::next_slot`, the same two instructions fail in klend with 6017.
- [ ] **Step 2: Watch them fail.**

```bash
cargo test --manifest-path tests/protocols/Cargo.toml --test jupiter_deposit_exact_output
```

  Every test panics: `the template has no accounts named "instructionsSysvar"`. The template lacks
  what v2 needs (research §4.1).
- [ ] **Step 3: Record how the template as written fails.** This is evidence for FINDINGS; commit
  nothing from it.
  - In a scratch copy of the pass test, bind the fixture's current names: the old list stops at
    `reserveDestinationDepositCollateral` and has only the `routeAccounts` group.
  - Run it.
  - Expected (research §4.1): klend fails on account 5, where the supply vault stands in for the
    liquidity mint: `InvalidAccountData`, or an Anchor account-deserialize code.
  - Record `failure.program`, `failure.code` and the klend log line.
- [ ] **Step 4: Constants.** Add to `shared.ts`, after `PYTH_RECEIVER`:

```ts
/** The instructions sysvar. Every Kamino v2 lending instruction takes it as an account. */
export const SYSVAR_INSTRUCTIONS = 'Sysvar1nstructions1111111111111111111111111' as const;
/** Kamino Farms, which Kamino Lend invokes whenever a lending instruction touches a reserve with a farm. */
export const KAMINO_FARMS = 'FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr' as const;
```

- [ ] **Step 5: The template.** Rewrite `jupiter-deposit-exact-output.ts`. The header's first two
  paragraphs stay as they are. From its third paragraph on, the file reads:

```ts
 * `route` takes the token program, the signing owner, and the owner's source and destination token
 * accounts first, and the template passes those four itself: the destination is the account it
 * measures, so what Jupiter credits is what gets deposited. The rest of the route's list varies in
 * length with the route, so it arrives as the `routeAccounts` group. Group members are forwarded
 * with the transaction's own writable flag and never sign.
 *
 * The deposit is Kamino's `_v2` handler. The v1 handler refuses every caller but Kamino itself and
 * a short whitelist (`CpiDisabled`), so a template cannot call it at all. v2 takes 17 accounts:
 * - The 14 declared below. The unused `placeholder_user_destination_collateral` slot holds the
 *   Kamino program: Kamino requires every optional slot to be present and reads its own ID as
 *   "none". Both token-program slots hold the SPL Token program.
 * - Then `farmAccounts`: the obligation's farm user state and the reserve's collateral farm, then
 *   the Farms program. When the reserve has no collateral farm, both farm slots hold the Kamino
 *   program. A group carries them because they are writable when present and read-only when they
 *   are the Kamino program, and a declared slot has one fixed writable flag. Before an
 *   obligation's first deposit into a reserve with a farm, `init_obligation_farms_for_reserve` must
 *   create its user state.
 *
 * Kamino takes a deposit only into an obligation refreshed in the same slot. It does not care
 * where in the transaction that happened, so the refreshes belong to the transaction, not the
 * template. Put `refresh_reserve` for each reserve the obligation holds, then `refresh_obligation`
 * with those reserves, before this run.
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
  JUPITER_ROUTE,
  JUPITER_V6,
  KAMINO_DEPOSIT,
  KAMINO_LEND,
  SYSVAR_INSTRUCTIONS,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  addressBytes,
} from './shared.js';

export const jupiterDepositExactOutput = defineTemplate({
  inputs: {
    /** Jupiter's `route` arguments: the Swap API's instruction data after the discriminator. */
    routeArgs: { type: 'bytes', maxLength: 512 },
    /** Below this the route is not worth depositing and the run fails instead. */
    minimumOut: { type: 'u64' },
  },
  accounts: {
    jupiter: { executable: true, address: addressBytes(JUPITER_V6) },
    kamino: { executable: true, address: addressBytes(KAMINO_LEND) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    instructionsSysvar: { address: addressBytes(SYSVAR_INSTRUCTIONS) },
    owner: { signer: true, writable: true },
    /** What the route sells from. */
    sourceAta: { writable: true },
    /** The route's destination, and the account the deposit draws from. */
    destinationAta: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
    obligation: { writable: true },
    lendingMarket: {},
    lendingMarketAuthority: {},
    reserve: { writable: true },
    reserveLiquidityMint: {},
    reserveLiquiditySupply: { writable: true },
    reserveCollateralMint: { writable: true },
    reserveDestinationDepositCollateral: { writable: true },
  },
  /**
   * `routeAccounts`: Jupiter's own list, whose length depends on the route. `farmAccounts`:
   * Kamino's v2 tail, described above.
   */
  accountGroups: ['routeAccounts', 'farmAccounts'],
  steps: [
    // `readBalanceBeforeSwap`, `swap`, `measureSwapOutput` and `swapMetItsFloor`: unchanged.
    step.invoke({
      program: account.fixed('kamino'),
      accounts: [
        { account: account.fixed('owner'), signer: true, writable: true },
        { account: account.fixed('obligation'), signer: false, writable: true },
        { account: account.fixed('lendingMarket'), signer: false, writable: false },
        { account: account.fixed('lendingMarketAuthority'), signer: false, writable: false },
        { account: account.fixed('reserve'), signer: false, writable: true },
        { account: account.fixed('reserveLiquidityMint'), signer: false, writable: false },
        { account: account.fixed('reserveLiquiditySupply'), signer: false, writable: true },
        { account: account.fixed('reserveCollateralMint'), signer: false, writable: true },
        { account: account.fixed('reserveDestinationDepositCollateral'), signer: false, writable: true },
        // The deposit draws from the account the swap paid into.
        { account: account.fixed('destinationAta'), signer: false, writable: true },
        // `placeholder_user_destination_collateral`, never used: the Kamino program means "none".
        { account: account.fixed('kamino'), signer: false, writable: false },
        // `collateral_token_program`, then `liquidity_token_program`.
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('instructionsSysvar'), signer: false, writable: false },
      ],
      accountGroup: 'farmAccounts',
      data: [
        data.literal(KAMINO_DEPOSIT),
        // Exactly what the swap produced, measured a moment ago.
        data.encode('u64', expression.variable('received')),
      ],
      label: 'depositSwapOutput',
    }),
  ],
});

export const compiled = compileTemplate(jupiterDepositExactOutput);
```

- [ ] **Step 6: The JS runner,** `run-jupiter-deposit.ts`:
  - **Header.** "handing the variable-length tail of Jupiter's account list, and of Kamino's,
    through as account groups". Add: "Send it after Kamino's refreshes, in the same transaction:
    `refresh_reserve` for each reserve the obligation holds, then `refresh_obligation`. See
    `kamino_refreshes` in `clients/rust/examples/protocol_runs.rs`."
  - **`KaminoDepositAccounts`** gains:
    - `reserveLiquidityMint: Address`;
    - `farm?: { reserveFarmState: Address; obligationFarmUserState: Address }`, documented as "The
      reserve's collateral farm and the obligation's user state in it, when the reserve has one".
  - **`buildJupiterDepositRun`** binds `instructionsSysvar: { address: address(SYSVAR_INSTRUCTIONS) }`
    and `reserveLiquidityMint`, and passes:

```ts
  const farmAccounts = input.kamino.farm
    ? [
        { address: input.kamino.farm.obligationFarmUserState, writable: true },
        { address: input.kamino.farm.reserveFarmState, writable: true },
        { address: address(KAMINO_FARMS) },
      ]
    : [{ address: address(KAMINO_LEND) }, { address: address(KAMINO_LEND) }, { address: address(KAMINO_FARMS) }];
  // … accountGroups: { routeAccounts, farmAccounts },
```

  - **The demo at the bottom** binds `reserveLiquidityMint: placeholder(18)`.
- [ ] **Step 7: The semantics tests,** in `protocol-semantics.test.ts`. Import
  `KAMINO_DEPOSIT`, `KAMINO_LEND`, `KAMINO_FARMS`, `SYSVAR_INSTRUCTIONS`.
  - **A Kamino call table.** Tasks 7, 8 and 10 each add a row:

```ts
/**
 * Kamino's v1 lending handlers refuse every caller but Kamino itself and a short whitelist
 * (`CpiDisabled`), so a template calls the `_v2` handler. v2 checks only that the reserves and
 * the obligation were refreshed in the current slot, so the refreshes go ahead of the run in the
 * transaction, and no template makes them. v2's list ends in farm accounts that are writable when
 * the reserve has the farm and the Kamino program when it does not, so a template forwards that
 * tail as `farmAccounts`, a group, which keeps each account's own writable flag.
 */
const kaminoCalls: [string, Template, { discriminator: Uint8Array; declared: number; amount: Expression }][] = [
  ['jupiterDepositExactOutput', jupiterDepositExactOutput, { discriminator: KAMINO_DEPOSIT, declared: 14, amount: { kind: 'variable', name: 'received' } }],
];
const kaminoDeposits: Template[] = [jupiterDepositExactOutput];

describe('Kamino calls are v2, forward the farm tail as a group, and leave refreshing to the transaction', () => {
  test.each(kaminoCalls)('%s', (_, template, expected) => {
    const calls = invokesOf(template, 'kamino');
    expect(calls).toHaveLength(1);
    const [call] = calls as [Invoke];
    const [discriminator, amount] = call.data;
    expect(discriminator?.kind === 'literal' ? [...discriminator.bytes] : []).toEqual([...expected.discriminator]);
    expect(amount).toEqual({ kind: 'encoded', encoding: 'u64', value: expected.amount });
    expect(call.accounts).toHaveLength(expected.declared);
    expect(call.accountGroup).toBe('farmAccounts');
  });

  test('a deposit passes the liquidity mint, the Kamino program as its unused placeholder, and the instructions sysvar', () => {
    for (const template of kaminoDeposits) {
      const [deposit] = invokesOf(template, 'kamino') as [Invoke];
      const names = deposit.accounts.map((entry) => nameOf(entry.account));
      expect(names[5]).toBe('reserveLiquidityMint');
      expect(names[10]).toBe('kamino');
      expect(deposit.accounts[10]!.writable).toBe(false);
      expect(names.slice(11)).toEqual(['tokenProgram', 'tokenProgram', 'instructionsSysvar']);
      expect(template.accounts.instructionsSysvar?.address).toEqual(addressBytes(SYSVAR_INSTRUCTIONS));
    }
  });
});
```

  - **"The Jupiter deposit runner":**
    - Add `reserveLiquidityMint: key(19)` to its `kamino` object.
    - In "forwards only what follows route's first four accounts as the group", assert
      `addresses.slice(-5)` is `[key(17), key(18), KAMINO_LEND, KAMINO_LEND, KAMINO_FARMS]`: the
      route group, then the farm tail with no farm.
    - Add a test for a reserve with a farm: `kamino.farm = { reserveFarmState: key(30), obligationFarmUserState: key(31) }`.
      The last three accounts are then `[key(31), key(30), KAMINO_FARMS]`, and the first two have
      a writable role (`isWritableRole` from `@solana/kit`).
- [ ] **Step 8: The harness's own order tests.** In `template.rs`:
  - `a_run_built_by_name_matches_one_built_by_hand`:
    - bind `instructionsSysvar` (`kamino::INSTRUCTIONS_SYSVAR`) and `reserveLiquidityMint` (`key(14)`);
    - bind `farmAccounts` to `[new(key(22)), new(key(23)), readonly(kamino::FARMS)]`;
    - expect data `[5, 3, 3, …]`;
    - expect accounts: `template, JUPITER, KAMINO, TOKEN, INSTRUCTIONS_SYSVAR, owner(s), key(5), key(6), key(7), key(8) ro, key(9) ro, key(10), key(14) ro, key(11), key(12), key(13)`,
      then the three route members, then the three farm members.
  - `a_missing_name_panics` now expects:
    `unbound in the run: accounts ["kamino", "tokenProgram", "instructionsSysvar", "owner", "sourceAta", "destinationAta", "obligation", "lendingMarket", "lendingMarketAuthority", "reserve", "reserveLiquidityMint", "reserveLiquiditySupply", "reserveCollateralMint", "reserveDestinationDepositCollateral"], inputs ["routeArgs", "minimumOut"], account groups ["routeAccounts", "farmAccounts"]`.
- [ ] **Step 9: The Rust runner,** `clients/rust/examples/protocol_runs.rs`:
  - Add the constants:

```rust
const JUPITER_V6: Pubkey = pubkey!("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");
const KAMINO_FARMS: Pubkey = pubkey!("FarmsPZpWu9i7Kky8tPN37rs2TpmMrAZrC7S7vJa91Hr");
const INSTRUCTIONS_SYSVAR: Pubkey = pubkey!("Sysvar1nstructions1111111111111111111111111");
/// The first eight bytes of `sha256("global:refresh_reserve")` and `…refresh_obligation`.
const REFRESH_RESERVE: [u8; 8] = [0x02, 0xda, 0x8a, 0xeb, 0x4f, 0xc9, 0x19, 0x66];
const REFRESH_OBLIGATION: [u8; 8] = [0x21, 0x84, 0x93, 0xe4, 0x97, 0xc0, 0x48, 0x59];
```

  - Replace the `#group` region's function (keep the region markers):

```rust
/// Kamino's reserve-side accounts for the deposit, and the reserve's collateral farm if it has one.
pub struct KaminoDeposit {
    pub obligation: Pubkey,
    pub lending_market: Pubkey,
    pub lending_market_authority: Pubkey,
    pub reserve: Pubkey,
    pub reserve_liquidity_mint: Pubkey,
    pub reserve_liquidity_supply: Pubkey,
    pub reserve_collateral_mint: Pubkey,
    pub reserve_collateral_supply: Pubkey,
    /// `(obligation farm user state, reserve farm state)` when the reserve has a collateral farm.
    pub farm: Option<(Pubkey, Pubkey)>,
}

/// Shape two: account groups, for callees whose account lists are not a fixed length.
///
/// This is `jupiter-deposit-exact-output`.
/// - Jupiter's `route` starts with the token program, the signing owner, and the owner's source
///   and destination token accounts. The template passes those four itself, so `route_accounts`
///   is the Swap API's list from the fifth account on, and one template serves every route.
///   `route_args` is the Swap API's instruction data after its eight-byte discriminator.
/// - Kamino's deposit ends in two farm accounts, writable when the reserve has a collateral farm
///   and the Kamino program ID when it does not, then the Farms program. They travel as a second
///   group, which keeps each account's own writable flag.
///
/// Send the run after [`kamino_refreshes`], in the same transaction.
pub fn run_jupiter_deposit(
    template: Pubkey,
    owner: Pubkey,
    token_accounts: (Pubkey, Pubkey),
    kamino: &KaminoDeposit,
    route_args: &[u8],
    route_accounts: Vec<AccountMeta>,
    minimum_out: u64,
) -> Instruction {
    let mut farm_accounts = match kamino.farm {
        Some((user_state, farm_state)) => vec![AccountMeta::new(user_state, false), AccountMeta::new(farm_state, false)],
        None => vec![AccountMeta::new_readonly(KAMINO_LEND, false); 2],
    };
    farm_accounts.push(AccountMeta::new_readonly(KAMINO_FARMS, false));
    // Group lengths come first, before any value, one byte per declared group.
    let inputs = RunInputs::new()
        .groups(&[route_accounts.len() as u8, farm_accounts.len() as u8])
        .bytes(route_args)
        .u64(minimum_out)
        .finish();

    let mut accounts = vec![
        AccountMeta::new_readonly(JUPITER_V6, false),
        AccountMeta::new_readonly(KAMINO_LEND, false),
        AccountMeta::new_readonly(TOKEN_PROGRAM, false),
        AccountMeta::new_readonly(INSTRUCTIONS_SYSVAR, false),
        AccountMeta::new(owner, true),
        AccountMeta::new(token_accounts.0, false),
        AccountMeta::new(token_accounts.1, false),
        AccountMeta::new(kamino.obligation, false),
        AccountMeta::new_readonly(kamino.lending_market, false),
        AccountMeta::new_readonly(kamino.lending_market_authority, false),
        AccountMeta::new(kamino.reserve, false),
        AccountMeta::new_readonly(kamino.reserve_liquidity_mint, false),
        AccountMeta::new(kamino.reserve_liquidity_supply, false),
        AccountMeta::new(kamino.reserve_collateral_mint, false),
        AccountMeta::new(kamino.reserve_collateral_supply, false),
    ];
    // The groups follow the declared accounts, in declaration order.
    accounts.extend(route_accounts);
    accounts.extend(farm_accounts);
    run_instruction(template, accounts, &inputs)
}
```

  - Add a `#refresh` region after it:

```rust
// #region refresh
/// Kamino's refreshes. A run that deposits into, repays or liquidates an obligation needs them
/// earlier in the same transaction. Kamino's v2 instructions check only that the reserves they
/// price and the obligation were refreshed in the current slot, not where.
///
/// - `held` is every reserve the obligation holds: deposits in its deposit order, then borrows in
///   its borrow order.
/// - `touched` adds any reserve the run uses that the obligation does not hold yet.
/// - Each reserve comes with the Scope price account its config names. The main market prices by
///   Scope alone, so the Pyth and Switchboard slots take the Kamino program ID, which it reads as
///   "none".
pub fn kamino_refreshes(
    lending_market: Pubkey,
    obligation: Pubkey,
    held: &[(Pubkey, Pubkey)],
    touched: &[(Pubkey, Pubkey)],
) -> Vec<Instruction> {
    let refresh_reserve = |&(reserve, scope_prices): &(Pubkey, Pubkey)| Instruction {
        program_id: KAMINO_LEND,
        accounts: vec![
            AccountMeta::new(reserve, false),
            AccountMeta::new_readonly(lending_market, false),
            AccountMeta::new_readonly(KAMINO_LEND, false), // Pyth
            AccountMeta::new_readonly(KAMINO_LEND, false), // Switchboard price
            AccountMeta::new_readonly(KAMINO_LEND, false), // Switchboard TWAP
            AccountMeta::new_readonly(scope_prices, false),
        ],
        data: REFRESH_RESERVE.to_vec(),
    };
    let mut refreshed: Vec<Pubkey> = Vec::new();
    let mut instructions = Vec::new();
    for entry in held.iter().chain(touched) {
        if !refreshed.contains(&entry.0) {
            refreshed.push(entry.0);
            instructions.push(refresh_reserve(entry));
        }
    }
    let mut accounts = vec![
        AccountMeta::new_readonly(lending_market, false),
        AccountMeta::new(obligation, false),
    ];
    // Every reserve the obligation holds, writable, in its own order.
    accounts.extend(held.iter().map(|&(reserve, _)| AccountMeta::new(reserve, false)));
    instructions.push(Instruction { program_id: KAMINO_LEND, accounts, data: REFRESH_OBLIGATION.to_vec() });
    instructions
}
// #endregion refresh
```

  - In `main`:
    - call `run_jupiter_deposit` with a `KaminoDeposit` of `key()`s and `farm: Some((key(), key()))`;
    - print its account count;
    - print `kamino_refreshes(key(), key(), &[(key(), key())], &[]).len()`.
- [ ] **Step 10: Regenerate and check.**

```bash
pnpm fixtures
pnpm --dir clients/js check && pnpm --dir clients/js test
cargo test -p ballista-common every_shared_fixture_parses_and_verifies
cargo run -p ballista-sdk --example protocol_runs
cargo test --manifest-path tests/protocols/Cargo.toml
```

  The three scenario tests pass. Note the CU and bytes printed by the first. If the transaction
  panics over 1,232 bytes, lower the manifest's `maxAccounts` (Task 1 Step 3). Do not add a lookup
  table.
- [ ] **Step 11: Commit** "Deposit a Jupiter swap's output into Kamino through its v2 instruction,
  as the real program requires". In the body, record the as-written failure from Step 3.

### Task 7: `kaminoRepaySwapOutput`

- [ ] **Step 1: The scenario test,** `tests/protocols/tests/kamino_repay_swap_output.rs`,
  `repays_exactly_what_the_swap_produced`.
  - **Scene.** The borrower is `wallet::wallet()`, funded with 100 SOL:
    - 10 SOL of wSOL (rule 1) deposited through `lending::open_obligation(.., &[SOL_RESERVE])`
      and `lending::deposit`;
    - the route's destination, the wallet's USDC ATA, written empty (rule 1) and asserted equal to
      `leg.destination_token_account`;
    - then `debt = 3 * leg.out_amount` USDC borrowed into that ATA, so one repayment cannot clear
      it;
    - then the wSOL ATA rewritten to exactly `leg.in_amount` (rule 1).
  - **The run.** Bind:
    - `jupiter`, `kamino`, `tokenProgram`, `instructionsSysvar`;
    - `borrower` (writable, signer);
    - `collateralAta` = the source, and `borrowedAssetAta` = the destination;
    - `obligation`, `lendingMarket`;
    - `repayReserve` = USDC_RESERVE, `reserveLiquidityMint`, `reserveLiquiditySupply`;
    - `routeArgs`, and `minimumRepayment = produced`;
    - the `routeAccounts` group, and `farmAccounts = kamino::repay_farm_accounts(&svm, &ob, &USDC_RESERVE)`.
  - **`send_run`:** `next_slot`, then `[compute_limit, kamino::refreshes(ob, []) (refresh SOL, refresh USDC, refresh_obligation), run]`
    with the leg's tables.
  - **Assertions.** `produced = lending::swap_output`, and `produced < debt`. Take `debt_before`
    as `kamino::borrowed_sf(USDC)` read on `svm.clone()` after `next_slot` and a setup send of the
    same refreshes: the debt as the run will see it. Then:
    - `debt_before - borrowed_sf(after) == u128::from(produced) << 60`: the debt fell by exactly
      the swap's output;
    - the USDC supply vault rose by `produced`;
    - the destination ATA is unchanged at `debt`: the output went to the debt, not the wallet;
    - print CU and bytes.
- [ ] **Step 2:** run it. It fails: `the template has no accounts named "instructionsSysvar"`.
- [ ] **Step 3: Record how the template as written fails** (scratch; not committed).
  - Bind the current names: with `reservePriceFeed = SCOPE_PRICES`, and only `routeAccounts`.
  - Expected (research §4.2): klend 3005 at the template's own three-account `refresh_reserve`,
    after the swap.
- [ ] **Step 4: The template.** In `kamino-repay-swap-output.ts`:
  - **Accounts.**
    - Remove `reservePriceFeed`.
    - Add `instructionsSysvar: { address: addressBytes(SYSVAR_INSTRUCTIONS) }` after
      `tokenProgram`, and `reserveLiquidityMint: {}` after `repayReserve`.
    - Set `accountGroups: ['routeAccounts', 'farmAccounts']`.
  - **Steps.**
    - Delete the `refreshReserve` step and its comment. `KAMINO_REFRESH_RESERVE` goes from the
      imports.
    - The repay invoke becomes:

```ts
    step.invoke({
      program: account.fixed('kamino'),
      accounts: [
        { account: account.fixed('borrower'), signer: true, writable: true },
        { account: account.fixed('obligation'), signer: false, writable: true },
        { account: account.fixed('lendingMarket'), signer: false, writable: false },
        { account: account.fixed('repayReserve'), signer: false, writable: true },
        { account: account.fixed('reserveLiquidityMint'), signer: false, writable: false },
        { account: account.fixed('reserveLiquiditySupply'), signer: false, writable: true },
        // The repayment draws from the account the swap paid into.
        { account: account.fixed('borrowedAssetAta'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('instructionsSysvar'), signer: false, writable: false },
      ],
      accountGroup: 'farmAccounts',
      data: [
        data.literal(KAMINO_REPAY),
        // Exactly what the swap produced, measured a moment ago.
        data.encode('u64', expression.variable('swapped')),
      ],
      label: 'repayWhatTheSwapProduced',
    }),
```

  - **Header.** Replace "Kamino's `refresh_reserve` runs just before the repayment…" with:

```ts
 * The repayment is Kamino's `_v2` handler: the v1 handler refuses every caller but Kamino itself
 * and a short whitelist. v2 takes the 9 accounts declared below, then `farmAccounts`, its tail:
 * - the obligation's farm user state and the reserve's debt farm, or the Kamino program for each
 *   when the reserve has no debt farm (the main market's SOL and USDC reserves have none);
 * - the lending market authority;
 * - the Farms program.
 * A group carries them so each keeps its own writable flag.
 *
 * Kamino takes a repayment only against a reserve and an obligation refreshed in the same slot,
 * and does not care where in the transaction that happened. Put `refresh_reserve` for each
 * reserve the obligation holds, then `refresh_obligation` with them, before this run. The swap in
 * between does not touch Kamino.
```

- [ ] **Step 5: Semantics.** `KAMINO_REPAY` and `kaminoRepaySwapOutput` are already imported.
  - Add the row `['kaminoRepaySwapOutput', kaminoRepaySwapOutput, { discriminator: KAMINO_REPAY, declared: 9, amount: { kind: 'variable', name: 'swapped' } }]`.
  - Delete the old "the Kamino repay" describe; the row now covers it.
- [ ] **Step 6:** Run `pnpm fixtures`, the JS checks and tests, and the Rust suite. It passes;
  note CU and bytes. Commit "Repay Kamino through its v2 instruction, with the refreshes in the
  transaction".

### Task 8: `kaminoLiquidateWithProof`

- [ ] **Step 1: The scenario tests,** `tests/protocols/tests/kamino_liquidate_with_proof.rs`.
  - **Scene:**
    - `lending::unhealthy_obligation(&mut svm)`;
    - `lending::liquidator(&mut svm, unhealthy.debt)`;
    - the template, uploaded.
  - **The run.** Bind the 19 declared accounts:
    - `kamino`, `tokenProgram`, `instructionsSysvar`;
    - `liquidator` (writable, signer);
    - `obligation`, `lendingMarket`, `lendingMarketAuthority`;
    - `repayReserve` = USDC_RESERVE, with its mint and supply vault;
    - `withdrawReserve` = SOL_RESERVE, with its mint, collateral mint, collateral supply, supply
      vault and fee vault (`withdrawReserveFeeReceiver`);
    - `userSourceLiquidity`, `userDestinationCollateral`, `userDestinationLiquidity`, from the
      `kamino::Liquidator`.
  - **Inputs:** `liquidityAmount`, `minAcceptableReceived = 0` (so klend's own floor never
    triggers first), `minimumBounty`.
  - **Group:** `farmAccounts = kamino::liquidation_farm_accounts(&svm, &ob, &USDC_RESERVE, &SOL_RESERVE)`.
  - **`send_run`:** `next_slot`, then `[compute_limit, kamino::refreshes(ob, []), run]`, signed
    by the liquidator.
  - The tests:
    1. `the_liquidator_nets_at_least_the_bounty`.
       - On `svm.clone()`, after `next_slot` and a setup send of the refreshes,
         `kamino::is_liquidatable` holds: the scenario is not vacuous.
       - Run with:
         - `liquidityAmount = debt / 10`, the market's close factor;
         - `minimumBounty = break_even(debt / 10, usdc_price, sol_price)`: the liquidator must at
           least get back what it repaid, valued at the oracle price.
       - Then assert:
         - `repaid = debt − USDC left` is more than 0 and at most `debt / 10`;
         - the wSOL received is at least `minimumBounty`, and more than `break_even(repaid, ..)`;
         - the cToken account holds 0;
         - print CU and bytes.
    2. `a_bounty_above_the_payout_refuses_at_liquidation_paid_the_bounty`.
       - Run test 1's instruction on `svm.clone()` to learn `payout`.
       - Then run with `minimumBounty = payout + 1` on `svm`.
       - `tx::assert_requirement_failed(.., "liquidationPaidTheBounty")`. The liquidator's USDC and wSOL are
         unchanged.
- [ ] **Step 2:** run them. They fail: `the template has no accounts named "instructionsSysvar"`.
- [ ] **Step 3: Record how the template as written fails** (scratch; not committed). Bind the
  current names, with `reservePriceFeed = SCOPE_PRICES`. Expected (research §4.3): klend 3005 at
  the template's own `refresh_reserve`. Also note the second fault, which the research found
  statically: even with every account right, the bounty was measured on the cToken account, which
  klend leaves unchanged. Task 4's liquidation contract test shows it.
- [ ] **Step 4: The template.** Rewrite `kamino-liquidate-with-proof.ts`:

```ts
/**
 * Liquidate a Kamino obligation and prove the liquidator came out ahead.
 *
 * `liquidate_obligation_and_redeem_reserve_collateral_v2(liquidity_amount,
 * min_acceptable_received_liquidity_amount, max_allowed_ltv_override_percent)` repays part of an
 * unhealthy obligation's debt and seizes collateral in return. In the same instruction it redeems
 * the seized cTokens and pays the underlying to `userDestinationLiquidity`, less Kamino's
 * protocol fee. If the reserve cannot redeem all of it, the rest stays in
 * `userDestinationCollateral` as cTokens.
 *
 * So the template measures `userDestinationLiquidity`, and requires it to have grown by at least
 * `minimumBounty`, in the collateral's own units. Anything less, including a payout left in
 * cTokens, and the run reverts. The reverted transaction still pays its fee.
 * - `minimumBounty` is the runner's bar, for example the repaid amount valued at the oracle price
 *   plus the margin worth liquidating for.
 * - Kamino's own `min_acceptable_received_liquidity_amount` is a floor on its computed liquidity
 *   leg. It is not a measurement of what arrived.
 *
 * The liquidation is Kamino's `_v2` handler; the v1 handler refuses every caller but Kamino itself
 * and a short whitelist. v2 takes the 20 accounts declared below, then `farmAccounts`:
 * - the borrower's user state in the withdrawn reserve's collateral farm, and that farm;
 * - the borrower's user state in the repaid reserve's debt farm, and that farm;
 * - the Farms program.
 * The Kamino program stands in for each farm account the reserve does not have. A group carries
 * them so each keeps its own writable flag.
 *
 * Kamino liquidates only against both reserves and the obligation refreshed in the same slot, and
 * refuses a healthy obligation (`ObligationHealthy`). Put `refresh_reserve` for each reserve the
 * obligation holds, then `refresh_obligation` with them, before this run.
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
  KAMINO_LEND,
  KAMINO_LIQUIDATE,
  SYSVAR_INSTRUCTIONS,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  addressBytes,
} from './shared.js';

export const kaminoLiquidateWithProof = defineTemplate({
  inputs: {
    /** How much debt to repay on behalf of the borrower. */
    liquidityAmount: { type: 'u64' },
    /** Kamino's own floor on the liquidity leg. */
    minAcceptableReceived: { type: 'u64' },
    /** What the liquidator must receive, in the seized collateral's underlying token. */
    minimumBounty: { type: 'u64' },
  },
  accounts: {
    kamino: { executable: true, address: addressBytes(KAMINO_LEND) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    instructionsSysvar: { address: addressBytes(SYSVAR_INSTRUCTIONS) },
    liquidator: { signer: true, writable: true },
    obligation: { writable: true },
    lendingMarket: {},
    lendingMarketAuthority: {},
    repayReserve: { writable: true },
    repayReserveLiquidityMint: {},
    repayReserveLiquiditySupply: { writable: true },
    withdrawReserve: { writable: true },
    withdrawReserveLiquidityMint: {},
    withdrawReserveCollateralMint: { writable: true },
    withdrawReserveCollateralSupply: { writable: true },
    withdrawReserveLiquiditySupply: { writable: true },
    /** Where Kamino's protocol fee on the seized collateral goes: the withdrawn reserve's fee vault. */
    withdrawReserveFeeReceiver: { writable: true },
    /** Pays the repayment. */
    userSourceLiquidity: { writable: true },
    /** Receives the seized cTokens, which Kamino redeems in the same instruction. */
    userDestinationCollateral: { writable: true },
    /** Receives the redeemed collateral: the account the bounty is measured on. */
    userDestinationLiquidity: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
  },
  /** Kamino's v2 tail, described above. */
  accountGroups: ['farmAccounts'],
  steps: [
    step.snapshot(
      'payoutBefore',
      expression.accountData(account.fixed('userDestinationLiquidity'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
      'readPayoutBefore',
    ),

    step.invoke({
      program: account.fixed('kamino'),
      accounts: [
        { account: account.fixed('liquidator'), signer: true, writable: true },
        { account: account.fixed('obligation'), signer: false, writable: true },
        { account: account.fixed('lendingMarket'), signer: false, writable: false },
        { account: account.fixed('lendingMarketAuthority'), signer: false, writable: false },
        { account: account.fixed('repayReserve'), signer: false, writable: true },
        { account: account.fixed('repayReserveLiquidityMint'), signer: false, writable: false },
        { account: account.fixed('repayReserveLiquiditySupply'), signer: false, writable: true },
        { account: account.fixed('withdrawReserve'), signer: false, writable: true },
        { account: account.fixed('withdrawReserveLiquidityMint'), signer: false, writable: false },
        { account: account.fixed('withdrawReserveCollateralMint'), signer: false, writable: true },
        { account: account.fixed('withdrawReserveCollateralSupply'), signer: false, writable: true },
        { account: account.fixed('withdrawReserveLiquiditySupply'), signer: false, writable: true },
        { account: account.fixed('withdrawReserveFeeReceiver'), signer: false, writable: true },
        { account: account.fixed('userSourceLiquidity'), signer: false, writable: true },
        { account: account.fixed('userDestinationCollateral'), signer: false, writable: true },
        { account: account.fixed('userDestinationLiquidity'), signer: false, writable: true },
        // `collateral_token_program`, `repay_liquidity_token_program`, `withdraw_liquidity_token_program`.
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('instructionsSysvar'), signer: false, writable: false },
      ],
      accountGroup: 'farmAccounts',
      data: [
        data.literal(KAMINO_LIQUIDATE),
        data.encode('u64', expression.input('liquidityAmount')),
        data.encode('u64', expression.input('minAcceptableReceived')),
        // No LTV override: liquidate on the protocol's own terms.
        data.encode('u64', expression.u64(0)),
      ],
      label: 'liquidate',
    }),

    step.require(
      expression.greaterThanOrEqual(
        expression.subtract(
          expression.accountData(account.fixed('userDestinationLiquidity'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
          expression.snapshot('payoutBefore'),
        ),
        expression.input('minimumBounty'),
      ),
      'liquidationPaidTheBounty',
    ),
  ],
});

export const compiled = compileTemplate(kaminoLiquidateWithProof);
```

- [ ] **Step 5: Semantics.** Import `kaminoLiquidateWithProof` and `KAMINO_LIQUIDATE`.
  - Add the row `['kaminoLiquidateWithProof', kaminoLiquidateWithProof, { discriminator: KAMINO_LIQUIDATE, declared: 20, amount: { kind: 'input', name: 'liquidityAmount' } }]`.
  - Add:

```ts
describe('the Kamino liquidation', () => {
  test('measures the bounty where Kamino pays the seized collateral', () => {
    const bindings = bindingsOf(kaminoLiquidateWithProof);
    const check = requireLabeled(kaminoLiquidateWithProof, 'liquidationPaidTheBounty');
    expect(dependsOn(check.condition, bindings, reads('userDestinationLiquidity', TOKEN_ACCOUNT_AMOUNT_OFFSET))).toBe(true);
    expect(dependsOn(check.condition, bindings, reads('userDestinationCollateral', TOKEN_ACCOUNT_AMOUNT_OFFSET))).toBe(false);
    const [liquidate] = invokesOf(kaminoLiquidateWithProof, 'kamino') as [Invoke];
    expect(nameOf(liquidate.accounts[15]!.account)).toBe('userDestinationLiquidity');
  });
});
```

- [ ] **Step 6:** run `pnpm fixtures`, the JS checks and tests, and the Rust suite; it passes.
  Commit "Liquidate on Kamino through its v2 instruction and measure the bounty where it is paid".

### Task 9: `marginfiWithdrawAllWithFloor`

- [ ] **Step 1: The scenario tests,** `tests/protocols/tests/marginfi_withdraw_all_with_floor.rs`.
  - **`scene(with_sol: bool)`:**
    - authority `b"ballista-protocol-tests-mfi-auth"`, account `b"ballista-protocol-tests-mfi-acct"`;
    - the authority's USDC ATA written with `DEPOSIT = 100_000_000` (rule 1), then deposited
      through `lending::marginfi_account`;
    - with `with_sol`, 1 SOL of wSOL deposited too;
    - `treasury` is the USDC ATA of `b"ballista-protocol-tests-treasury"`, holding 0.
  - **The run.** Bind:
    - `marginfi`, `tokenProgram`, `marginfiGroup`;
    - `marginfiAccount` (writable), `authority` (signer);
    - `bank` = USDC_BANK (writable), `bankLiquidityVault` (writable);
    - `bankLiquidityVaultAuthority = marginfi::vault_authority(&USDC_BANK)`, **not writable**;
    - `destinationAta` = the authority's USDC ATA, and `treasuryAta`;
    - `minimumWithdrawn`;
    - `healthAccounts = marginfi::health_accounts(&svm, &account, &USDC_BANK)`.
  - **`send_run`:** `next_slot`, then `[compute_limit, run]`, signed by the authority. There are no
    refreshes: marginfi has none.
  - The tests:
    1. `withdraws_a_sole_position_and_sweeps_it`, with `minimumWithdrawn = 99_000_000`:
       - the treasury holds `withdrawn`, between 99,999,999 and 100,000,000 (marginfi's share
         rounding gave 99,999,999 in the probe);
       - the destination holds 0;
       - `active_banks` is empty;
       - print CU and bytes.
    2. `withdraws_one_position_of_two_with_the_health_group`: `scene(true)`, with the same
       assertions, except that `active_banks == [SOL_BANK]`.
    3. `a_floor_above_the_position_refuses_at_withdrawal_met_its_floor`:
       - `minimumWithdrawn = DEPOSIT + 1`;
       - `tx::assert_requirement_failed(.., "withdrawalMetItsFloor")`;
       - the USDC balance is still active, and the treasury holds 0.
- [ ] **Step 2:** run them. They fail: `the template has no account groups named "healthAccounts"`.
  The current template does handle the sole-balance case (the research refuted the fault there).
  Test 1 fails only because it binds the new group.
- [ ] **Step 3: Record how the template as written fails** (scratch; not committed). Run
  `scene(true)` with the current names: no group, and a writable vault authority. Expected:
  marginfi 6008 (`InvalidBankAccount`). Task 4 shows the same at the top level.
- [ ] **Step 4: The template.** In `marginfi-withdraw-all-with-floor.ts`:
  - **Accounts:**
    - `bankLiquidityVaultAuthority: {}` (no longer writable);
    - `accountGroups: ['healthAccounts']`.
  - **The withdraw invoke:**
    - the vault authority's `writable: false`;
    - add `accountGroup: 'healthAccounts'`.
  - **Header,** after the paragraph about `withdraw_all`:

```ts
 * marginfi then checks the account's health against every balance it still holds, and reads
 * those balances' banks and oracles from the accounts after withdraw's eight. `healthAccounts`
 * carries them: for each remaining balance, its bank and then its oracle, by bank address from
 * highest to lowest. It is empty when the withdrawn balance was the account's only one; without
 * it, any other balance fails the withdrawal (`InvalidBankAccount`). A Token-2022 bank also needs
 * its mint, first in the group. The vault authority is a PDA marginfi signs for, so it is passed
 * read-only.
```

- [ ] **Step 5: Semantics.** Import `marginfiWithdrawAllWithFloor` from the examples and
  `MARGINFI_WITHDRAW` from `shared.ts`:

```ts
const marginfiWithdrawals: [string, Template][] = [['marginfiWithdrawAllWithFloor', marginfiWithdrawAllWithFloor]];

describe('marginfi withdrawals', () => {
  test.each(marginfiWithdrawals)("%s forwards the health check's banks and oracles after withdraw's eight accounts", (_, template) => {
    const [withdraw] = invokesOf(template, 'marginfi') as [Invoke];
    const [discriminator] = withdraw.data;
    expect(discriminator?.kind === 'literal' ? [...discriminator.bytes] : []).toEqual([...MARGINFI_WITHDRAW]);
    expect(withdraw.accounts).toHaveLength(8);
    expect(withdraw.accountGroup).toBe('healthAccounts');
    // The vault authority is a PDA marginfi signs for; nothing writes it.
    expect(withdraw.accounts[5]!.writable).toBe(false);
  });
});
```

- [ ] **Step 6:** run `pnpm fixtures`, the JS checks and tests, and the Rust suite; it passes.
  Commit "Pass marginfi's health-check banks through a group when withdrawing".

### Task 10: `marginfiToKaminoRebalance`, and retiring Drift

- [ ] **Step 1: The scenario test,** `tests/protocols/tests/marginfi_to_kamino_rebalance.rs`,
  `deposits_into_kamino_exactly_what_marginfi_released`.
  - **Scene.** The owner is `b"ballista-protocol-tests-rebalanc"`, funded, and the marginfi
    account is `b"ballista-protocol-tests-rebal-mf"`:
    - the owner's USDC ATA written with 100 USDC, then deposited through
      `lending::marginfi_account` (the ATA is left at 0);
    - `lending::open_obligation(.., &[USDC_RESERVE])`.
  - **The run.** Bind:
    - `marginfi`, `kamino`, `tokenProgram`, `instructionsSysvar`;
    - `owner` (writable, signer), `walletAta`;
    - `marginfiGroup`, `marginfiAccount`, `marginfiBank` = USDC_BANK, `marginfiVault`,
      `marginfiVaultAuthority`;
    - `obligation`, `lendingMarket`, `lendingMarketAuthority`;
    - `reserve` = USDC_RESERVE, with its mint, supply, collateral mint and collateral supply;
    - `minimumMoved = 1`;
    - `healthAccounts = marginfi::health_accounts(..)`, empty here;
    - `farmAccounts = kamino::deposit_farm_accounts(..)`.
  - **`send_run`:** `next_slot`, then `[compute_limit, kamino::refreshes(ob, [USDC_RESERVE]), run]`,
    signed by the owner.
  - **Assertions.** With `released` as the drop in marginfi's USDC vault:
    - Kamino's USDC supply vault rose by exactly `released`;
    - `released` is between 99,999,999 and 100,000,000;
    - the wallet ATA holds 0;
    - `kamino::deposited` equals the cTokens minted;
    - marginfi's `active_banks` is empty;
    - print CU and bytes.
- [ ] **Step 2:** run it. It fails: `no example "marginfiToKaminoRebalance"`.
- [ ] **Step 3: The template,** `clients/js/examples/protocols/marginfi-to-kamino-rebalance.ts`:

```ts
/**
 * Move a position from marginfi to Kamino in one transaction, depositing exactly what came out.
 *
 * Kamino's `deposit_reserve_liquidity_and_obligation_collateral_v2(liquidity_amount: u64)` needs a
 * number that the marginfi withdrawal produces moments earlier. A transaction has to guess it.
 * Guess high and the deposit fails on insufficient funds, taking the withdrawal down with it;
 * guess low and the remainder sits in the wallet, earning nothing, until someone notices.
 *
 * The template empties the marginfi balance, measures what landed in the wallet, and deposits
 * exactly that.
 * - `healthAccounts` is what marginfi's health check reads once the withdrawn balance is gone: for
 *   every balance the account still holds, its bank and then its oracle, by bank address from
 *   highest to lowest. It is empty when the withdrawn balance was the only one. A Token-2022 bank
 *   also needs its mint, first.
 * - `farmAccounts` is the end of Kamino's v2 deposit: the obligation's farm user state and the
 *   reserve's collateral farm, or the Kamino program for each when the reserve has no collateral
 *   farm; then the Farms program. A group carries them so each keeps its own writable flag.
 * - Kamino takes the deposit only into an obligation refreshed in the same slot. Put
 *   `refresh_reserve` for each reserve the obligation holds, then `refresh_obligation` with those
 *   reserves, before this run.
 *
 * Without a template this takes two transactions, with the funds sitting in the wallet between
 * them, or a custom program.
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
  KAMINO_DEPOSIT,
  KAMINO_LEND,
  MARGINFI_V2,
  MARGINFI_WITHDRAW,
  SYSVAR_INSTRUCTIONS,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  addressBytes,
} from './shared.js';

export const marginfiToKaminoRebalance = defineTemplate({
  inputs: {
    /** Do not bother rebalancing less than this. */
    minimumMoved: { type: 'u64' },
  },
  accounts: {
    marginfi: { executable: true, address: addressBytes(MARGINFI_V2) },
    kamino: { executable: true, address: addressBytes(KAMINO_LEND) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    instructionsSysvar: { address: addressBytes(SYSVAR_INSTRUCTIONS) },
    owner: { signer: true, writable: true },
    /** The wallet account the assets pass through. */
    walletAta: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
    marginfiGroup: {},
    marginfiAccount: { writable: true },
    marginfiBank: { writable: true },
    marginfiVault: { writable: true },
    marginfiVaultAuthority: {},
    obligation: { writable: true },
    lendingMarket: {},
    lendingMarketAuthority: {},
    reserve: { writable: true },
    reserveLiquidityMint: {},
    reserveLiquiditySupply: { writable: true },
    reserveCollateralMint: { writable: true },
    reserveDestinationDepositCollateral: { writable: true },
  },
  accountGroups: ['healthAccounts', 'farmAccounts'],
  steps: [
    step.snapshot(
      'walletBefore',
      expression.accountData(account.fixed('walletAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
      'readWalletBeforeWithdraw',
    ),

    step.invoke({
      program: account.fixed('marginfi'),
      accounts: [
        { account: account.fixed('marginfiGroup'), signer: false, writable: false },
        { account: account.fixed('marginfiAccount'), signer: false, writable: true },
        { account: account.fixed('owner'), signer: true, writable: false },
        { account: account.fixed('marginfiBank'), signer: false, writable: true },
        { account: account.fixed('walletAta'), signer: false, writable: true },
        { account: account.fixed('marginfiVaultAuthority'), signer: false, writable: false },
        { account: account.fixed('marginfiVault'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
      ],
      accountGroup: 'healthAccounts',
      data: [
        data.literal(MARGINFI_WITHDRAW),
        // `amount` is ignored when `withdraw_all` is Some(true), but Borsh still reads it.
        data.encode('u64', expression.u64(0)),
        // Option::Some(true).
        data.literal(Uint8Array.of(1, 1)),
      ],
      label: 'withdrawFromMarginfi',
    }),

    step.let(
      'moved',
      expression.subtract(
        expression.accountData(account.fixed('walletAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
        expression.snapshot('walletBefore'),
      ),
      'measureWithdrawal',
    ),

    step.require(
      expression.greaterThanOrEqual(expression.variable('moved'), expression.input('minimumMoved')),
      'worthRebalancing',
    ),

    step.invoke({
      program: account.fixed('kamino'),
      accounts: [
        { account: account.fixed('owner'), signer: true, writable: true },
        { account: account.fixed('obligation'), signer: false, writable: true },
        { account: account.fixed('lendingMarket'), signer: false, writable: false },
        { account: account.fixed('lendingMarketAuthority'), signer: false, writable: false },
        { account: account.fixed('reserve'), signer: false, writable: true },
        { account: account.fixed('reserveLiquidityMint'), signer: false, writable: false },
        { account: account.fixed('reserveLiquiditySupply'), signer: false, writable: true },
        { account: account.fixed('reserveCollateralMint'), signer: false, writable: true },
        { account: account.fixed('reserveDestinationDepositCollateral'), signer: false, writable: true },
        { account: account.fixed('walletAta'), signer: false, writable: true },
        // `placeholder_user_destination_collateral`, never used: the Kamino program means "none".
        { account: account.fixed('kamino'), signer: false, writable: false },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('instructionsSysvar'), signer: false, writable: false },
      ],
      accountGroup: 'farmAccounts',
      data: [
        data.literal(KAMINO_DEPOSIT),
        // Exactly what marginfi released, not an estimate of it.
        data.encode('u64', expression.variable('moved')),
      ],
      label: 'depositIntoKamino',
    }),
  ],
});

export const compiled = compileTemplate(marginfiToKaminoRebalance);
```

- [ ] **Step 4: Retire Drift.**
  - `git rm clients/js/examples/protocols/drift-rebalance-exact.ts clients/js/examples/protocols/drift-settle-when-profitable.ts`.
  - **`index.ts`:** drop both Drift exports. Add
    `export { marginfiToKaminoRebalance } from './marginfi-to-kamino-rebalance.js';` in
    alphabetical order, before `marginfiWithdrawAllWithFloor`.
  - **`shared.ts`:** delete `DRIFT_V2`, `DRIFT_DEPOSIT`, `DRIFT_WITHDRAW` and `u16Bytes`. Only the
    Drift templates used them; `u16Bytes` existed for Drift's `market_index`. Keep the generic Borsh
    constants. First check with `git grep -n "DRIFT_\|u16Bytes" -- clients common tests scripts`;
    it must come back empty.
  - **Counts, 12 → 11:**
    - `protocol-examples.test.ts` (`expect(entries.length).toBe(11)`);
    - `common/src/template/verify.rs` (`assert_eq!(examples, 11, …)`);
    - `tests/protocols/src/template.rs` (`every_example_lines_up_with_its_payload`);
    - `protocol_runs.rs`'s doc ("all eleven examples").
  - **`protocol-semantics.test.ts`:**
    - delete the "the Drift settle" describe, and the now-unused imports
      (`driftSettleWhenProfitable`, `DRIFT_WITHDRAW`, `BORSH_TRUE`);
    - add the rebalance to `kaminoCalls` (`{ discriminator: KAMINO_DEPOSIT, declared: 14, amount: { kind: 'variable', name: 'moved' } }`),
      to `kaminoDeposits`, and to `marginfiWithdrawals`;
    - add, in the Kamino describe:

```ts
  test('every example that pins Kamino is listed here', () => {
    const kamino = [...addressBytes(KAMINO_LEND)].join();
    const pinning = Object.entries(protocols)
      .filter(([, template]) =>
        Object.values(template.accounts).some(
          (constraint) => constraint.address !== undefined && [...constraint.address].join() === kamino,
        ),
      )
      .map(([name]) => name)
      .sort();
    expect(pinning).toEqual(kaminoCalls.map(([name]) => name).sort());
  });
```

    - add the same for marginfi (`MARGINFI_V2` against `marginfiWithdrawals`);
    - import `marginfiToKaminoRebalance` from the examples and `MARGINFI_V2` from `shared.ts`.
- [ ] **Step 5: Regenerate and check.**

```bash
pnpm fixtures
pnpm --dir clients/js check && pnpm --dir clients/js test
cargo test -p ballista-common every_shared_fixture_parses_and_verifies
cargo run -p ballista-sdk --example protocol_runs
cargo test --manifest-path tests/protocols/Cargo.toml
pnpm check:docs
```

  - `pnpm check:docs` still builds. VitePress renders a missing `<<<` snippet as "Code snippet path
    not found" instead of failing. `docs/examples/protocols/drift-*.md` show that until the docs
    session replaces them; Task 11 tells it.
  - `git grep -n -i drift -- clients common tests scripts` finds nothing. The strings that remain in
    `docs/` are the docs session's.
- [ ] **Step 6: Commit** "Replace the Drift templates with a marginfi-to-Kamino rebalance". In the
  body, say that Drift v2's program is a withdraw-only drain since 2026-04-01, with the
  program-data upgrade at slot 429,731,225 (research `drift.md` §0), so no Drift template can run
  against it.

### Task 11: Findings, and notes for the docs session

- [ ] **Step 1: Collect the numbers.**

```bash
cargo test --manifest-path tests/protocols/Cargo.toml -- --nocapture 2>&1 | grep -E " CU, [0-9]+ bytes"
```

- [ ] **Step 2: Write `tests/protocols/FINDINGS.md`.**
  - Append to milestone 1's file; if it does not exist yet, create it with a one-line title.
  - This section is the docs session's source, so give file paths, account names and behaviour, not
    prose for the site. Its outline:

```markdown
## Milestone 3: Kamino, marginfi, and retiring Drift

Snapshot `snapshot-lending/`: slot …, clock …. klend deployed at 440,486,775
(`Kamino-Finance/klend@a08760976f`, release/v1.25.0), marginfi at 444,313,123
(`mrgnlabs/marginfi-v2@33c67987a6`), Farms at …, route `solToUsdc` via ….

| Template | As written | Fix (commit) | Real tests | CU | Bytes |
| --- | --- | --- | --- | --- | --- |
| `jupiterDepositExactOutput` | Failed in klend: … (Task 6 Step 3) | … | `tests/jupiter_deposit_exact_output.rs` | … | … |
| `kaminoRepaySwapOutput` | Failed in klend: … (Task 7 Step 3; expected 3005 at its own `refresh_reserve`) | … | … | … | … |
| `kaminoLiquidateWithProof` | Failed in klend: … (Task 8 Step 3; expected 3005). It also measured the cToken account, which klend leaves unchanged | … | … | … | … |
| `marginfiWithdrawAllWithFloor` | Worked for a sole balance; a second balance failed in marginfi with … (Task 9 Step 3; expected 6008) | … | … | … | … |
| `marginfiToKaminoRebalance` | New; replaces `driftRebalanceExact` | … | … | … | … |
| `driftRebalanceExact`, `driftSettleWhenProfitable` | Cannot run: Drift v2 is now a withdraw-only drain program | Deleted (…) | none | | |
```

  Then these sections:
  - **What klend and marginfi require of a caller.** One row per rule, with its code and its test
    in `tests/kamino_contract.rs` or `tests/marginfi_contract.rs`:
    - v1 handlers refuse any CPI caller (6080);
    - every optional slot must be present, with the program ID for "none" (3005);
    - `refresh_reserve` takes six accounts, with Scope last on these reserves (6054);
    - `refresh_obligation` takes every held reserve, writable, refreshed in the slot (6006, 6009);
    - a farmed reserve needs its farm accounts (6120), and its user state created first;
    - everything is fresh in the same slot (6017);
    - liquidation pays liquidity, not cTokens;
    - marginfi's health check needs every remaining balance's bank and oracle (6008).
  - **What a runner must now do:**
    - prepend the refreshes (`protocol_runs.rs` `#refresh`; klend-interface
      `refresh_all_for_obligation`);
    - fill `farmAccounts` per reserve;
    - create the obligation's farm user state before the first deposit into a farmed reserve;
    - fill `healthAccounts`: bank and oracle pairs by bank address, highest first; Token-2022 mint
      first;
    - choose `minimumBounty` in the collateral's units.
  - **Limits:**
    - SPL Token only;
    - the liquidation bounty is not netted against the repayment, which is in another mint;
    - marginfi banks with multi-account oracles (staked, Kamino, Drift, JupLend) take more accounts
      per bank than `health_accounts` builds;
    - marginfi's oracles are never stamped. Its initial health check values an asset with a stale
      oracle at zero, and no scenario holds a liability.
  - **Notes for the docs session (never edited here):**
    - The pages that include deleted files render "Code snippet path not found":
      - `docs/examples/protocols/drift-rebalance.md` includes `drift-rebalance-exact.ts`. Replace
        it with a marginfi → Kamino page on `marginfi-to-kamino-rebalance.ts`.
      - `docs/examples/protocols/drift-settle.md` includes `drift-settle-when-profitable.ts`.
        Delete it.
    - Where Drift is still mentioned (line numbers as of `4c6b938`):
      - `docs/.vitepress/config.mts`: the two Drift sidebar entries;
      - `docs/examples/index.md`: line 29, which lists Drift, and the rows at lines 41 and 42;
      - `docs/examples/protocols/index.md`: "Twelve", the rows at lines 23 and 24, line 36 on the
        Drift and marginfi templates, and line 102's "twelve templates need only three kinds of
        run".
    - Pages whose template changed, each with what changed:
      - `jupiter-deposit.md`: 17 accounts, `farmAccounts`, runner refreshes.
      - `kamino-repay.md`: no refresh step, `farmAccounts`.
      - `kamino-liquidate.md`:
        - the new measured account and what the bounty means;
        - the 25 accounts;
        - its "Run · Rust" tab shows `#plain`, but the run now has a group.
      - `marginfi-withdraw.md`:
        - `healthAccounts`;
        - the vault authority is now read-only;
        - its tab also shows `#plain`.
      - The "Not yet run against …" lines are now false for all five: name the test files.
    - `protocol_runs.rs`:
      - `#group` changed signature (`KaminoDeposit`, the farm group);
      - `#refresh` is new, and the Kamino pages will want it;
      - `#plain` and `#rows` are unchanged.
    - `run-jupiter-deposit.ts` gained `reserveLiquidityMint` and `farm`.
    - If Task 1 Step 5 found no CI job, milestone 1's Task 9 should key its LFS cache on
      `tests/protocols/snapshot*/manifest.json`.

- [ ] **Step 3:** commit "Record what the lending templates needed to run against the real
  programs".
- [ ] **Step 4:** the full suite once more, from a clean build:

```bash
cargo build-sbf --manifest-path programs/ballista/Cargo.toml
cargo test --manifest-path tests/protocols/Cargo.toml
pnpm --dir clients/js check && pnpm --dir clients/js test
cargo test -p ballista-common
git status --short
```

  `git status --short` must be clean. Then hand over with the superpowers:finishing-a-development-branch
  skill.
