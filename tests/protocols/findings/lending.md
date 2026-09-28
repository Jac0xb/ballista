# Milestone 3: Kamino, marginfi, and retiring Drift

What the lending templates needed to run against the real programs, and what the docs session has
to change because of it. Plan: `docs/superpowers/plans/2026-09-27-protocol-lending.md`.

Snapshot `snapshot-lending/` (from `scripts/snapshot/manifests/lending.json`): slot 451,137,053,
clock 2026-09-27 22:55:04 UTC (unix 1,790,549,704).

- klend deployed at slot 440,486,775 (`Kamino-Finance/klend@a08760976f`, release/v1.25.0).
- marginfi deployed at 444,313,123 (`mrgnlabs/marginfi-v2@33c67987a6`).
- Kamino Farms deployed at 444,035,168.
- Route `solToUsdc`: 1 SOL for 121,391,105 USDC units via Meteora DLMM (`HTvjzsfX…`), 29
  accounts, one lookup table.

## The templates

CU and bytes are what one run printed (`cargo test -- --nocapture`); no test asserts them. Each is
the whole transaction: compute budget, Kamino's refreshes when it needs them, and the run.

| Template | Failed as written | Fix (commits) | Test in `tests/` | CU | Bytes |
| --- | --- | --- | --- | --- | --- |
| `jupiterDepositExactOutput` | klend `InvalidAccountData`: 10 accounts under v2's discriminator | v2's 14 accounts and `farmAccounts`; refreshes before the run (`67535b3`) | `jupiter_deposit_exact_output.rs` | 150,842 | 1,072 |
| `kaminoRepaySwapOutput` | Its own `refresh_reserve` CPI: 3005, three accounts of six | No refresh step; v2's 9 and `farmAccounts` (`5d7ac45`); owner check (`471877c`) | `kamino_repay_swap_output.rs` | 141,280 | 993 |
| `kaminoLiquidateWithProof` | The same 3005; it also measured the cToken account, which klend leaves at 0 | No refresh steps; v2's 20 and `farmAccounts`; bounty on `userDestinationLiquidity` (`a4b6428`); owner checks (`471877c`) | `kamino_liquidate_with_proof.rs` | 185,402 | 1,045 |
| `marginfiWithdrawAllWithFloor` | marginfi 6008 when another balance remains | `healthAccounts` after withdraw's 8; vault authority read-only (`7664007`); owner checks (`471877c`) | `marginfi_withdraw_all_with_floor.rs` | 60,162; 78,056 with a second balance | 550; 616 |
| `marginfiToKaminoRebalance` | New, replacing `driftRebalanceExact` (`d32dfe4`) | | `marginfi_to_kamino_rebalance.rs` | 162,738 | 1,009 |
| `driftRebalanceExact`, `driftSettleWhenProfitable` | Drift v2 is a withdraw-only drain | Deleted (`d32dfe4`) | none | | |

What the scenarios moved:

- Deposit: 121,391,105 USDC units produced, 121,391,104 deposited, 1 left by klend's rounding;
  100,970,304 cTokens minted.
- Repay: a debt of three times the route's quote fell by exactly the 121,391,105 produced.
- Liquidation: an obligation that borrowed 70% of 1 SOL, after SOL's Scope price fell 12%. 8,497,265
  of 84,972,650 USDC units repaid (the market's close factor, 10%), 81,344,262 lamports received
  against a break-even of 79,539,088. No cTokens left.
- Rebalance: marginfi released 99,999,999 of 100,000,000 USDC units; Kamino took all of it and
  minted 83,177,679 cTokens.

Drift is retired: its v2 instructions were disabled on 2026-04-01 (v2.162.0, slot 410,366,404).
Since the upgrade at slot 429,731,225 (2026-06-29), `dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH`
is a withdraw-only drain with three instructions and no `deposit`, `withdraw` or `settle_pnl`, and
its vaults are empty.

## Paying only the signer

Assume the run's builder is hostile, say a frontend the signer trusts only to build transactions.
It names every account, and klend and marginfi check the mints of the token accounts they pay, not
their owners. Each test below names an attacker's account (right mint) in a slot, on a run that
otherwise lands. Before the fix (`471877c`) every one landed, and its panic printed:

| Template | Attacker's account in | Before the fix | Refused now at |
| --- | --- | --- | --- |
| `kaminoLiquidateWithProof` | `userDestinationLiquidity`, approved | Attacker 0 → 81,344,262 lamports; the liquidator repaid 8,497,265 USDC units for nothing | `bountyGoesToTheLiquidator` |
| `kaminoLiquidateWithProof` | `userDestinationCollateral`, approved | Landed, attacker's cTokens back at 0: klend redeemed them all. What it cannot redeem stays there (`lending_operations.rs:2171`) | `seizedCollateralGoesToTheLiquidator` |
| `marginfiWithdrawAllWithFloor` | `treasuryAta` | Attacker got 99,999,999 of 100,000,000 | `sweepGoesToTheAuthority` |
| `marginfiWithdrawAllWithFloor` | `destinationAta` and `treasuryAta`, approved | Attacker got 99,999,999 | `withdrawalGoesToTheAuthority` |
| `kaminoRepaySwapOutput` | `borrowedAssetAta` and the route's output, approved | 1 SOL of the borrower's sold, debt 60,695,552 → 0, attacker kept 60,695,553 | `swapPaysTheBorrower` |

- "Approved": the attacker's own `Approve` lets the signer's authority move tokens out of the
  account. Without it, the liquidation's payout slot fails closed at klend's fee transfer (SPL
  Token 4, `OwnerMismatch`).
- The fix: each slot's token owner (offset 32) must be the signer, checked before anything moves.
  The marginfi treasury must now be another account of the authority's own.
- `jupiterDepositExactOutput` and `marginfiToKaminoRebalance` fail closed: klend's v2 deposit
  refuses a source the signing owner does not own, approval or not (`token::authority = owner`,
  `handler_deposit_reserve_liquidity_and_obligation_collateral.rs:214`; Anchor 2015).
- `kaminoRepaySwapOutput` did not: klend's repay checks only the source's mint
  (`handler_repay_obligation_liquidity.rs:143`) and repays at most the debt
  (`state/reserve.rs:960`), so the rest of the swap stayed with the attacker.

klend paths are under `programs/klend/src/` at `a08760976f`.

## What klend and marginfi require of a caller

Each rule has a test in `tests/kamino_contract.rs` or `tests/marginfi_contract.rs` unless named.

| Rule | Code when broken | Test |
| --- | --- | --- |
| klend's v1 lending handlers refuse a CPI caller that is not whitelisted, Ballista included; v2 handlers accept it | klend 6080 `CpiDisabled` | `v1_handlers_refuse_a_ballista_caller_and_v2_handlers_do_not` |
| Every optional account slot must be present, with klend's program ID meaning "none"; a missing trailing optional is not "none" | Anchor 3005 `AccountNotEnoughKeys` | `refresh_reserve_takes_six_accounts_with_scope_last` |
| `refresh_reserve` takes reserve, market, Pyth, Switchboard price, Switchboard TWAP, Scope. These reserves price by Scope alone, so the three before it are klend's ID | klend 6054 `InvalidPythPriceAccount` (Scope in the Pyth slot) | `refresh_reserve_takes_six_accounts_with_scope_last` |
| `refresh_obligation` takes every reserve the obligation holds, writable, deposits then borrows, each refreshed earlier in the slot | klend 6006 `InvalidAccountInput` (missing), 6009 `ReserveStale` (not refreshed) | `refresh_obligation_takes_every_reserve_refreshed_in_the_slot` |
| A deposit into a reserve with a collateral farm needs the farm accounts, and the obligation's farm user state created first (`init_obligation_farms_for_reserve`, mode 0) | klend 6120 `FarmAccountsMissing` | `a_deposit_into_a_farmed_reserve_needs_the_farm_accounts` |
| v2 deposit, repay and liquidation need the obligation refreshed in the same slot, anywhere in the transaction. A first deposit needs nothing more: the deposit refreshes its own reserve | klend 6017 `ObligationStale` | `a_deposit_needs_the_obligation_refreshed_in_its_slot`; `jupiter_deposit_exact_output.rs` `a_setup_refresh_hides_a_missing_refresh_until_the_slot_moves` |
| A liquidation pays the seized collateral out as liquidity, to `user_destination_liquidity`, and leaves the cToken account unchanged | (not an error) | `a_liquidation_pays_liquidity_and_keeps_no_ctokens` |
| klend mints `floor(amount / rate)` cTokens and takes `ceil(cTokens × rate)`, so less than one cToken's worth stays in the source | (not an error) | `jupiter_deposit_exact_output.rs` (1 of 121,391,105 left) |
| marginfi's `withdraw_all` of an account's only balance needs no remaining accounts | (lands) | `withdrawing_a_sole_balance_needs_no_remaining_accounts` |
| A marginfi withdrawal that leaves another balance needs that balance's bank and oracle after its eight accounts. Only one remaining balance is tested; the order for several, highest bank address first, is read from `sort_balances` (`marginfi_account.rs:1725`) | marginfi 6008 `InvalidBankAccount` | `a_withdrawal_that_leaves_a_balance_needs_that_balances_bank` |

## What a runner must do

- **Refresh first, in the same transaction.** Before a run that deposits into, repays or
  liquidates an obligation: `refresh_reserve` for each reserve the obligation holds, then
  `refresh_obligation` with them, writable, deposits then borrows, then a referred obligation's
  referrer token states (`handler_refresh_obligation.rs:18`). A first deposit's reserve needs no
  refresh; sending one is harmless. No template refreshes. Builders: `kamino_refreshes` in
  `clients/rust/examples/protocol_runs.rs` (region `#refresh`), klend-interface's
  `refresh_all_for_obligation`, and `kamino::refreshes` in the harness.
- **Fill `farmAccounts` per reserve.** Writable when the reserve has the farm; klend's program ID,
  read-only, when it does not.
  - Deposit, 3: the obligation's user state in the reserve's collateral farm, that farm, Farms.
  - Repay, 4: the debt farm user state and the debt farm, the lending market authority, Farms. The
    main market's SOL and USDC reserves have no debt farm, so this is
    `[klend, klend, market authority, Farms]`.
  - Liquidation, 5: the borrower's collateral farm user state and farm (of the withdrawn reserve),
    the debt farm user state and farm (of the repaid reserve), Farms.
- **Create the farm user state** with `init_obligation_farms_for_reserve` (mode 0) before an
  obligation's first deposit into a reserve with a collateral farm. The main market's SOL and USDC
  reserves both have one.
- **Fill `healthAccounts`** for marginfi withdrawals: for each balance the account still holds
  after the withdrawal, its bank then its oracle, by bank address from highest to lowest. Empty
  when the withdrawn balance was the only one.
- **Name the signer's own token accounts** wherever a template pays out; see above.
- **Choose `minimumBounty`** in the seized collateral's own units (lamports for SOL collateral),
  for example the repaid amount valued at the oracle price plus the margin worth liquidating for.
- **Keep the transaction under 1,232 bytes.** The deposit run with its refreshes took 1,072 bytes
  with the route's one lookup table. A route with more accounts may need a lower `maxAccounts` or a
  lookup table of the runner's own.

## Limits

- SPL Token only. Every template pins the SPL Token program and its token accounts' owner, so
  Token-2022 reserves and banks cannot run.
- Only the token accounts the templates pay are bound to the signer. The route's own accounts and
  every input are still the builder's to choose.
- The liquidation bounty is not netted against the repayment, which is in another mint.
- marginfi banks priced by multi-account oracles (staked, Kamino, Drift, JupLend) take more
  accounts per bank than `marginfi::health_accounts` builds; it panics on them.
- marginfi's oracles are never stamped. Its initial health check values an asset with a stale
  oracle at zero rather than failing, and no scenario holds a liability.
- Tests write only wallet balances, Scope prices (the four entries the reserves read, stamped with
  the snapshot's clock, and SOL's lowered 12% for the liquidation) and the clock (one slot per
  template transaction, no time). A refreshed snapshot whose reserves read other Scope entries, or
  whose market has no partial close factor, fails in `lending::svm` or `kamino::close_factor_pct`
  with the fix named.

## Notes for the docs session (never edited here)

Line numbers are as of this branch (`claude/protocol-lending`).

### The build is broken until these two pages go

`pnpm check:docs` fails: VitePress 1.6.4 stats a `<<<` snippet's path before checking it exists,
so a deleted source throws `ENOENT`.

- `docs/examples/protocols/drift-rebalance.md` embeds the deleted
  `clients/js/examples/protocols/drift-rebalance-exact.ts` (line 18). Replace it with a page for
  `clients/js/examples/protocols/marginfi-to-kamino-rebalance.ts` (below).
- `docs/examples/protocols/drift-settle.md` embeds the deleted
  `clients/js/examples/protocols/drift-settle-when-profitable.ts` (line 17). Delete it.

### Where Drift is still mentioned

- `docs/.vitepress/config.mts` lines 143 and 144: the sidebar entries "Drift · rebalance" and
  "Drift · settle and withdraw". The first becomes the marginfi → Kamino page; the second goes.
- `docs/examples/index.md` line 29 ("Jupiter, Kamino, marginfi, Drift, …"), and the rows at lines
  41 (Rebalance, marginfi → Drift) and 42 (Settle and withdraw).
- `docs/examples/protocols/index.md`:
  - line 3: "Twelve example templates … Drift": now eleven, and no Drift;
  - the rows at lines 23 and 24;
  - line 36: "The Kamino, Drift and marginfi templates don't read those protocols' accounts": drop
    Drift. It is still true of the others: they read only SPL token accounts, their balances and,
    in three templates, the owner of each account paid out;
  - line 102: "The twelve templates need only three kinds of run": eleven.

### The new marginfi → Kamino page

- Source: `clients/js/examples/protocols/marginfi-to-kamino-rebalance.ts`, export
  `marginfiToKaminoRebalance`.
- What it does: `lending_account_withdraw(0, Some(true))` empties a marginfi balance into
  `walletAta`; the template measures what arrived (`moved`), requires at least `minimumMoved`
  (`worthRebalancing`), and deposits exactly `moved` into a Kamino obligation through the v2
  deposit (`depositIntoKamino`).
- Accounts: `marginfi`, `kamino`, `tokenProgram`, `instructionsSysvar`, `owner` (signer),
  `walletAta`, `marginfiGroup`, `marginfiAccount`, `marginfiBank`, `marginfiVault`,
  `marginfiVaultAuthority` (read-only), `obligation`, `lendingMarket`, `lendingMarketAuthority`,
  `reserve`, `reserveLiquidityMint`, `reserveLiquiditySupply`, `reserveCollateralMint`,
  `reserveDestinationDepositCollateral`. Input: `minimumMoved`. Groups: `healthAccounts` (after
  marginfi's 8 accounts), then `farmAccounts` (after Kamino's 14).
- The transaction puts Kamino's refreshes before the run.
- Kamino's cToken rounding can leave less than one cToken's worth in `walletAta`.
- Kamino refuses a `walletAta` the owner does not own, so a run cannot divert the withdrawal.
- SPL Token banks and reserves only.
- Run against marginfi and Kamino in `tests/protocols/tests/marginfi_to_kamino_rebalance.rs`; one
  run printed 162,738 CU and 1,009 bytes.
- Rust tab: `protocol_runs.rs#group` is the closest shape (two groups here).

### Pages whose template changed

- `docs/examples/protocols/jupiter-deposit.md`:
  - Kamino's deposit is v2 with 17 accounts: the 14 declared (now including
    `reserveLiquidityMint`, the Kamino program as the unused placeholder, both token programs and
    `instructionsSysvar`) and `farmAccounts`, a second group of 3.
  - The runner puts Kamino's refreshes before the run (`#refresh`). The template does not refresh.
  - Before an obligation's first deposit into a farmed reserve, its farm user state must exist.
  - Kamino keeps back less than one cToken's worth (a base unit or so). The page's "guess too low
    and the rest stays in your token account" is still the problem the template solves; "exactly"
    now has that footnote.
  - `run-jupiter-deposit.ts` gained `reserveLiquidityMint` and an optional `farm`, and passes
    `farmAccounts` (lines 42-44 describe the script).
- `docs/examples/protocols/kamino-repay.md`:
  - Lines 12-13 are now false: the template does not refresh the reserve. The transaction carries
    `refresh_reserve` for each reserve the obligation holds and `refresh_obligation` before the run.
  - `reservePriceFeed` is gone; `instructionsSysvar` and `reserveLiquidityMint` are new; v2's
    tail (debt farm pair, lending market authority, Farms) arrives as `farmAccounts`.
  - New: the borrower must own `borrowedAssetAta` (`swapPaysTheBorrower`); `borrower` is read-only.
- `docs/examples/protocols/kamino-liquidate.md`:
  - Lines 12-15 are now false. The template does not refresh, and it measures
    `userDestinationLiquidity`, where Kamino pays the redeemed collateral (SOL for SOL
    collateral), not the collateral token account; the bounty is in that token's units. The old
    cToken measurement always saw 0.
  - The liquidation has 25 accounts: 20 declared and 5 in `farmAccounts`.
  - Its Rust tab shows `#plain`, and lines 31-33 say "This template has no account group, so
    leave out the `.groups(...)` call": no longer true.
  - New: the liquidator must own `userDestinationLiquidity` and `userDestinationCollateral`
    (`bountyGoesToTheLiquidator`, `seizedCollateralGoesToTheLiquidator`); `liquidator` is
    read-only.
  - Lines 25-28 on gating by health: klend-interface's `Obligation` puts
    `borrow_factor_adjusted_debt_value_sf` at offset 2208 and `unhealthy_borrow_value_sf` at 2256,
    discriminator included (u128, 60 fractional bits; liquidatable when the first is at least the
    second). No template reads them, so a gate on them is untested.
- `docs/examples/protocols/marginfi-withdraw.md`:
  - New `healthAccounts` group: for each remaining balance its bank and oracle, highest bank
    address first; empty for a sole balance; without it any second balance fails
    (`InvalidBankAccount`).
  - The vault authority is now read-only.
  - Lines 3-4 ("sends the proceeds on to a treasury account") and 12-14: the treasury must now be
    the authority's own account, and so must the destination (`withdrawalGoesToTheAuthority`,
    `sweepGoesToTheAuthority`).
  - Its Rust tab shows `#plain`, and lines 29-31 say the template has no account group: no longer
    true.
- The "Not yet run against …" lines are now false: `jupiter-deposit.md:46`, `kamino-repay.md:41`,
  `kamino-liquidate.md:35`, `marginfi-withdraw.md:33` and `drift-rebalance.md:33` (whose page
  becomes the marginfi → Kamino one), all in `docs/examples/protocols/`. The tests are
  `tests/protocols/tests/{jupiter_deposit_exact_output,kamino_repay_swap_output,kamino_liquidate_with_proof,marginfi_withdraw_all_with_floor,marginfi_to_kamino_rebalance}.rs`,
  and `kamino_contract.rs` and `marginfi_contract.rs` pin the rules above.
- `docs/examples/protocols/index.md` is now false for these five in three places:
  - lines 8-9, "None has been run against the real protocols";
  - "What has been tested" (from line 26), "No test runs any of these templates against the real
    protocols" (line 31);
  - "What they cost", line 117, "No test runs these protocols' programs, so these templates have
    no measured costs". It can cite the table at the top of this file, as one run's figures.

### `clients/rust/examples/protocol_runs.rs`

- `#group` changed: `run_jupiter_deposit` takes a `KaminoDeposit` (with the liquidity mint and an
  optional farm) instead of `[Pubkey; 7]`, passes the instructions sysvar, and sends two groups.
  Four pages embed it: `jupiter-deposit.md`, `jupiter-oracle-swap.md`, `kamino-repay.md`,
  `token-sweep.md`. The last two only borrow its shape.
- `#refresh` is new: `kamino_refreshes`, which takes a referred obligation's referrer token
  states. The Kamino pages will want it.
- `#plain` and `#rows` are unchanged.
- The module doc now says "all eleven examples".

### CI

- There is no protocol-test job on this branch, so the LFS cache key was not changed. When
  milestone 1's CI job lands, key its LFS cache on
  `hashFiles('tests/protocols/snapshot*/manifest.json')`, which covers `snapshot-lending/` too.
- `.gitattributes` sends `tests/protocols/snapshot*/programs/*.so` to Git LFS.
