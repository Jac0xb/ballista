# Orca: what the templates did against the real Whirlpool program

Milestone 2 of the real-protocol tests ran `orcaCompoundFees` and `orcaHarvestManyPositions` as
signed transactions against mainnet's Whirlpool program in LiteSVM. Neither worked as written, and
both are fixed. A later security pass found a second problem shared by both (M7). Every fix except
M5's, a rewritten header, landed with a test that failed before it; the findings table says what
shows each one. Plan:
[2026-09-27-protocol-orca](../../../docs/superpowers/plans/2026-09-27-protocol-orca.md).

- **Snapshot:** `tests/protocols/snapshot-orca/`, slot 451,137,027 (2026-09-27 22:54:57 UTC). Every
  CU and byte figure below was measured against this snapshot; a later one could shift them.
  - Pools: SOL/USDC `Czfq…` (tick spacing 4) for the compounder, and SOL/USDC `HJPj…` (tick
    spacing 64) for the harvest. Both were near tick βˆ’21,090, about 121 USDC per SOL.
  - Programs, all from mainnet: Whirlpool, Token, Token-2022, Associated Token, Memo.
- **Tests:**
  - `orca_snapshot.rs`: the pools and their tick arrays are in the snapshot.
  - `orca_setup.rs`: the Orca behaviors the fixes rest on, called without Ballista.
  - `orca_cpis.rs`: every Whirlpool call against Orca's own client.
  - `orca_compound_fees.rs`: 12 tests, four of them with a real `dustFloor`.
  - `orca_harvest_many_positions.rs`: 7 tests, one with a real `dustFloor`.
  - Each run's Whirlpool calls are asserted in order (`orca::whirlpool_calls`), not counted, so a
    call by the wrong row fails. A collect of nothing moves nothing, so no account would show it.
- **Setup is real:** positions, deposits, withdrawals, swaps and updates go through Orca's
  instructions, built with `orca_whirlpools_client` 8.0.0. Expected numbers come from Orca's own
  math, `orca_whirlpools_core` 2.1.1. Only wallets' balances are written directly.

## Findings

| | Finding | Template | Shown by | Fixed |
| --- | --- | --- | --- | --- |
| M1 | `collect_fees` passed the whirlpool writable; Orca declares it read-only | both | a static check of the compiled templates (`orca_cpis.rs`): no run can see a flag | yes |
| M2 | `fee_owed_*` was read without `update_fees_and_rewards`, so it was stale, usually 0 | both | real runs | yes |
| M3 | The guard read token A's fee only | both | real runs | yes |
| M4 | `increase_liquidity` with a liquidity fixed at signing fails once the price moves or the fees are one-sided | compound | real runs | yes |
| M5 | The harvest's `when` was said to prevent reverts; it only saves compute | harvest | a setup test of Orca alone (`orca_setup.rs`) and real refusals | header rewritten |
| M6 | Whirlpool's error codes collide with Ballista's | both | real refusals, and the runner's unit tests | tests and runner read the logs |
| M7 | `tokenOwnerAccountA`/`B` were not pinned to the signer; `collect_fees` checks only their mint | both | real runs (`fees_must_go_to_the_owner_not_a_strangers_accounts`) | yes |

## `orcaCompoundFees`

**As written, it did nothing.**
- It read `fee_owed_a` and `fee_owed_b` without updating the position. So it always saw 0 and
  skipped both calls (M2).
- Once something else had updated the position, its `increase_liquidity` failed with
  `TokenMaxExceeded` (6017), unless the caller's `liquidityAmount` fit the fees at the landing
  price.
  - With fees in one token it failed for any liquidity, and the collect reverted with it (M4).
  - Its guard read token A alone (M3).
- Its `collect_fees` passed the whirlpool writable (M1). This changed no transaction's locks,
  because the other calls write the pool.

**Fixed.**
- It calls `update_fees_and_rewards` first, skipped for a position without liquidity. Whirlpools
  refuses the update there with `LiquidityZero` (6012), and such a position has nothing to record.
- It collects when either fee is above `dustFloor`.
- It reinvests with `increase_liquidity_by_token_amounts_v2`, as Orca's SDK does.
  - The two fees are the caps, and the program works out the most liquidity they buy at the
    price when the block runs. One fee is used whole and part of the other stays in the wallet.
  - The deposit runs only when both fees are above the floor and the position still has
    liquidity. In range, liquidity takes both tokens: with either cap at zero the program works
    out none and fails with 6012.
  - `minSqrtPrice` and `maxSqrtPrice` bound the pool price the deposit accepts. Outside them it
    fails with `PriceSlippageOutOfBounds` (6069), and the whole run reverts.
- `collect_fees` takes the whirlpool read-only (M1).
- `tokenOwnerAccountA`/`B` must belong to `positionAuthority` (M7).

**Decided while planning, beyond M1–M6: an emptied position is collected, not refilled.**
- A position its owner emptied with `decrease_liquidity` still has fees owed. The old template
  would have poured them back in.
- The fixed one collects them and leaves the position empty
  (`an_emptied_position_is_collected_not_refilled`).
- If refilling is wanted, drop the `hasLiquidity` term from the deposit's `when`, and that test.

**Interface changes, for the docs.**
- Inputs, in order: `dustFloor: u64`, `minSqrtPrice: u128`, `maxSqrtPrice: u128`.
  - `liquidityAmount` is gone.
  - The two bounds are Q64.64 sqrt prices; Orca's `get_sqrt_price_slippage_bounds` computes them
    from a price and a tolerance.
- The accounts gain `memoProgram` (pinned to `MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr`),
  `tokenMintA` and `tokenMintB`, which makes 15. `fixtures/protocol-examples.json` has the order:
  - whirlpoolProgram, tokenProgram, memoProgram, positionAuthority (signer);
  - whirlpool (writable), position (writable), positionTokenAccount, tokenMintA, tokenMintB;
  - tokenOwnerAccountA, tokenOwnerAccountB, tokenVaultA, tokenVaultB, tickArrayLower,
    tickArrayUpper (all writable).
- Step labels: `feesGoToTheOwner`, `readLiquidity`, `updateFees`, `readFeesOwedA`, `readFeesOwedB`,
  `collectFees`, `compoundFees`.
- The payload grew from 440 bytes to 714 (M1–M4) and then 842 (M7 added 128 bytes).
- `shared.ts` loses `ORCA_INCREASE_LIQUIDITY`. It gains `MEMO_PROGRAM`,
  `ORCA_UPDATE_FEES_AND_REWARDS`, `ORCA_INCREASE_LIQUIDITY_BY_TOKEN_AMOUNTS_V2`,
  `ORCA_BY_TOKEN_AMOUNTS`, `TOKEN_ACCOUNT_LENGTH` and `TOKEN_ACCOUNT_OWNER_OFFSET`.

**Remaining limits.**
- **SPL Token pools only.** `collect_fees` and the pinned token program are SPL Token's, so both
  mints must be SPL Token mints. SOL and USDC are.
- **Dust can fail the deposit.** A fee too small to buy one unit of liquidity over the position's
  range makes the deposit fail with 6012, and the collect reverts with it. A full range buys the
  least per unit of fee. `dust_that_buys_no_liquidity_fails_the_run_unless_the_floor_skips_it`
  shows it on a full-range SOL/USDC position, set up with Orca's own instructions (its end tick
  arrays included):
  - owed (1 lamport, 34 micro-USDC) at `dustFloor` 0, Whirlpools refuses the deposit with 6012;
  - at a `dustFloor` of 1, the deposit is skipped, and the run lands with the update and the
    collect;
  - by Orca's own quote at this price, 1 or 2 lamports buy no full-range liquidity, and 3 buy 1.

  So a floor of a few base units covers SOL/USDC. A pool whose token A is worth less per base unit
  needs more; a few thousand is a safe default. The template's header and its `dustFloor` doc say
  this now, and that 0 is not a safe choice (below).
- **One `dustFloor` for two tokens.** It is compared with each fee in that token's own base
  units, and lamports and micro-USDC differ by 1,000 times.
  - `a_floor_between_the_fees_collects_both_and_reinvests_neither` pins what that means. At a
    floor equal to the smaller fee, that fee is dust, the larger is worth collecting, and
    `collect_fees` takes both. The deposit, which needs both, is skipped.
  - `fees_at_or_below_the_floor_stay_owed`: with both fees at or below it, nothing is collected,
    and the update leaves them recorded as owed.
  - Two floors would be more precise. The name is kept because milestone 1's harness test and
    both runners use it.

## `orcaHarvestManyPositions`

**As written, it collected nothing.**
- The same stale read as the compounder: no row ever showed a fee (M2).
- Its guard read token A alone (M3).
- Its `collect_fees` passed the whirlpool writable (M1).

**Fixed.**
- Each row calls `update_fees_and_rewards`, skipped for a position without liquidity. Then the
  row collects when either fee is above the floor.
  - `only_the_rows_that_earned_collect`: the calls come out as update and collect for the two
    earning rows, update alone for the out-of-range row, and nothing for the empty one.
  - `a_row_whose_fees_are_at_the_floor_is_left_for_later`: the floor is compared with each row's
    own fees. A row whose one fee equals the floor updates, which records the fee, and leaves it
    owed.
- The row is `position`, `positionTokenAccount`, `tickArrayLower` and `tickArrayUpper`. The update
  reads both tick arrays.
- `collect_fees` takes the whirlpool read-only (M1). The schema keeps `whirlpool` writable because
  each row's update writes it, so M1 changes no locks. A run cannot observe it, so its test reads
  the compiled templates (`orca_cpis.rs`).
- `tokenOwnerAccountA`/`B` must belong to `positionAuthority`, checked once for the whole batch
  rather than once per row, since both accounts are fixed (M7).
- The payload grew from 298 bytes to 478 (M1–M3) and then 606 (M7 added 128 bytes).
- Step labels: `feesGoToTheOwner`, `updateIfLiquid`, `collectIfWorthIt`, `everyPosition`.

**M5, for the docs: what the guard is for.**
- The guard saves compute, about 13,300 CU per skipped collect β€” `orca_compound_fees.rs`'s
  one-sided runs minus its no-fees run, 25,814 or 25,821 minus 12,494 β€” and leaves dust alone.
- It never prevented a revert: `collect_fees` with nothing owed succeeds and moves nothing
  (`orca_setup.rs`).
- What reverts the whole harvest is a row Whirlpools refuses:
  - a position the signer does not hold: `MissingOrInvalidDelegate`, 6019;
  - a position from another pool: Anchor's `ConstraintHasOne`, 2001.
- Neither depends on trades, so filter those rows out before building the run. The template's
  header now says this.

**Interface changes, for the docs.**
- `run-orca-harvest.ts`:
  - `HarvestRow` gains `tickArrayLower` and `tickArrayUpper`;
  - new: `getOrcaTickArrayAddress(whirlpool, tickIndex, tickSpacing)`, which derives
    `["tick_array", whirlpool, start]`;
  - new: `failedProgram(logs)`;
  - changed: `describeFailure(code, logs)` now takes the failed transaction's logs (M6), and calls
    out truncated logs by name instead of reporting that no program failed.
- `clients/rust/examples/protocol_runs.rs`:
  - The `#rows` region, which the harvest page includes, has a new `HarvestRow` struct, and
    `run_orca_harvest` takes `rows: &[HarvestRow]`.
  - `cargo run -p ballista-sdk --example protocol_runs` prints 29 accounts for 5 rows (was 19).

**Limits, measured at this snapshot (M7 included; see there for its own cost).**
- **Ballista's own limits hold with room.**
  - A stride of 4 row accounts (limit 8).
  - 8 fixed accounts plus 4 Γ— 12 rows = 56 runtime accounts (limit 120).
  - 2 calls Γ— 12 rows = 24 CPIs (limit 64).
- **Compute:** about 23,600 CU per earning row.
  - `eight_rows_fit_the_default_compute_limit`: eight land under the default 200,000 with no
    compute-budget instruction, 190,931 CU; a ninth exhausts it, failing mid-CPI with no custom
    code (`InstructionError::ProgramFailedToComplete`, not a Whirlpools or Ballista error number).
- **Size:** 68 bytes per earning row when rows share tick arrays. Each extra distinct tick array
  costs 32 bytes more.
  - `ten_rows_fit_one_legacy_transaction`: ten fit a legacy transaction with a compute-budget
    instruction, 1,195 bytes; eleven do not, and need a lookup table.

## M6: which program refused

- Whirlpool's codes are Anchor's, numbered from 6000 like Ballista's. Read as Ballista's:
  - 6012 (`LiquidityZero`) is `TypeMismatch`;
  - 6017 (`TokenMaxExceeded`) is `InvalidPdaDerivation`;
  - 6019 (`MissingOrInvalidDelegate`) is `ReturnDataMismatch`.
- The tests assert the failing program from the logs (`Failure.program`), never by the code
  alone. `orca::assert_whirlpool_error` names Whirlpools' own errors so a test reads
  `LiquidityZero`, not `6012`.
- `run-orca-harvest.ts::describeFailure` takes the logs, and names Whirlpools for its own codes.
- The plan left one item to the program's owner, the doc comment on `RunError` in
  `programs/ballista/src/processor/execute.rs`, which said invoked programs' codes are "never
  confused with Ballista's". Runtime phase 1 (1efbd24) has since rewritten it to say a code alone
  does not name its program. Nothing is left to do there.

## M7: fees must go to the owner

Both templates named `tokenOwnerAccountA`/`tokenOwnerAccountB` as the fee destinations without
pinning who holds them. Whirlpools' `collect_fees` checks only their mint against its vaults, never
their owner (confirmed by reading the accounts Orca's own client builds for it, in
`orca_cpis.rs`'s `orca_accounts`), so a run assembled by an untrusted builder β€” a frontend, a bot,
anything the true owner merely signs β€” could point them at the builder's own accounts and collect
the position's real fees there. The owner's signature over `positionAuthority` authorizes the
collect; it said nothing about where the proceeds went.

- **compound** was exposed on its collect-only path: a `dustFloor` between the two fees collects
  both without reinvesting. When it does reinvest, the deposit draws from the same accounts with
  the owner's authority and fails closed against a stranger's, so only the collect-only path paid
  out.
- **harvest** was exposed on any earning row, since every row shares the same fixed
  `tokenOwnerAccountA`/`B`.

**Fixed.** Both templates require `tokenOwnerAccountA`/`B`'s owner field (SPL Token account offset
32) to equal `positionAuthority`'s key, labelled `feesGoToTheOwner`. Harvest checks it once before
the batch, not once per row. Both accounts now also pin `owner: TOKEN_PROGRAM_ADDRESS_BYTES` and a
165-byte `minDataLength`, so the read is of a real token account, not an `unsafeUnpinned` guess.

`fees_must_go_to_the_owner_not_a_strangers_accounts` (both test files) is the regression test: an
attacker's own token accounts of the right mints stand in for the fee destinations, on a run that
otherwise lands (a floor between the fees for compound, one earning row for harvest), and it
asserts `RequirementFailed` at `feesGoToTheOwner`. Before this fix, neither template's schema
constrained these accounts at all beyond `writable: true`, and nothing else in either template
read them β€” so the same run would have landed and paid the stranger. That was not re-confirmed by
reverting the fix and rerunning, since doing so means running the vulnerable templates again to
prove a point already settled by inspection: Orca's own account list for `collect_fees`
(`orca_cpis.rs`'s `orca_accounts`) constrains neither account's owner either.

**Cost, measured at this snapshot.** Two account reads and two comparisons, paid once per
transaction regardless of row count, and the run's own account list and inputs are unchanged, so
its wire size does not move:
- compound: +612 CU on every path (11,882 β†’ 12,494 CU with no fees; every other case in the
  Measurements table moved the same 612). 706 bytes, unchanged.
- harvest: +648 CU (59,610 β†’ 60,258 CU on the four-row test; 237,622 β†’ 238,270 on the ten-row
  test). 747 and 1,195 bytes, unchanged.
- Both templates' uploaded payload grew 128 bytes (compound 714 β†’ 842, harvest 478 β†’ 606); that
  is a one-time upload cost, not a per-run one.

## CI

- No protocol-test CI job exists on this branch, so there is no LFS cache key to widen.
- Whoever adds the job should:
  - key the LFS cache on `hashFiles('tests/protocols/snapshot*/manifest.json')`, so both
    snapshots, and later ones, invalidate it;
  - run `git lfs pull`, `cargo build-sbf --manifest-path programs/ballista/Cargo.toml` and
    `cargo test --manifest-path tests/protocols/Cargo.toml`.
- `.gitattributes` now keeps `tests/protocols/snapshot*/programs/*.so` in LFS.

## Measurements

From `cargo test --manifest-path tests/protocols/Cargo.toml -- --nocapture --test-threads=1`, at
this snapshot, with M7's check included. Compute units include Ballista's own work; sizes are the
signed transaction on the wire.

| Run | Compute units | Bytes | Whirlpool calls |
| --- | --- | --- | --- |
| compound, fees in both tokens | 43,994 | 706 | 3 |
| compound, fees in one token | 25,814–25,821 | 706 | 2 |
| compound, no fees | 12,494 | 706 | 1 |
| compound, no liquidity | 3,158 | 706 | 0 |
| compound, emptied position | 16,267 | 706 | 1 |
| harvest, the four rows | 60,258 | 747 | 5 |
| harvest, eight earning rows, no budget instruction | 190,931 | 1,019 | 16 |
| harvest, ten earning rows | 238,270 | 1,195 (budget instruction included) | 20 |
| Orca: `update_fees_and_rewards` | 7,687–8,184 | | |
| Orca: `collect_fees`, fees owed | 11,346–11,390 | | |
| Orca: `collect_fees`, nothing owed | 11,375 | | |
| Orca: `increase_liquidity_by_token_amounts_v2` | 15,281 | | |

The four Orca rows above call Orca's instructions directly, without a Ballista template, so M7
does not touch them.

- The planning run (slot 451,125,511) measured the two-sided compound at 42,991 CU and 674 bytes.
- The 32 bytes are one tick array. At this snapshot the price sits 30 ticks above its array's first
  tick, so the ±60-tick position's bounds fall in two arrays rather than one. Everything else is
  within a few hundred CU of the planning run.
- In the two-sided run the fees were (13,919,999 lamports, 2,087,999 micro-USDC).
  - The liquidity added equals the smaller of Orca's two quotes, exactly.
  - 327,114 micro-USDC stayed in the wallet.

## Evidence

One line per finding: the test written before its fix, failing as run at this snapshot. M5 has no
row, since its fix is the header; M7's is in its own section above, since nothing captured its
pre-fix run.

| Finding | Test | Failed as |
| --- | --- | --- |
| M1 | `orca_cpis.rs::every_whirlpool_call_passes_the_accounts_orcas_client_does` | "`collect_fees`: whirlpool is writable here and read-only in Orca's client" (both templates) |
| M2 (compound) | `orca_compound_fees.rs::two_sided_fees_are_collected_and_compounded` | "liquidity added left: 0, right: 1599965546" |
| M2 (harvest) | `orca_harvest_many_positions.rs::only_the_rows_that_earned_collect` | "left: (0, 0), right: (47454547, 4971430)" |
| M3 | `orca_harvest_many_positions.rs::only_the_rows_that_earned_collect`, after M2's fix | "left: (47454547, 2485715), right: (47454547, 4971430)": the B-only row's fee stayed behind |
| M4 | `orca_compound_fees.rs::fees_in_token_a_alone_are_collected_not_compounded` | "failed with code 6017 (0x1781)" (`TokenMaxExceeded`) |
| M6 | `run-orca-harvest.ts`'s `describeFailure` unit test | returned `"ReturnDataMismatch at inputs.dustFloor"` instead of naming Whirlpools |

The floor and call-order tests, added after the fixes above, have no failure from before one. A
mutation check stands in: with both templates' floor comparisons weakened to `>=`, all four floor
tests fail, and so does `only_the_rows_that_earned_collect`, on its call order alone β€” the idle
rows' empty collects left every balance and account as it was:

```text
  left: ["UpdateFeesAndRewards", "CollectFees", "UpdateFeesAndRewards", "CollectFees", "UpdateFeesAndRewards", "CollectFees", "CollectFees"]
 right: ["UpdateFeesAndRewards", "CollectFees", "UpdateFeesAndRewards", "CollectFees", "UpdateFeesAndRewards"]
```

## Claims in the docs that are now false

Quoted from this branch; the docs session's copy may have moved on. Both pages show the template's
source directly, so each new header appears there without a docs edit.

- `docs/examples/protocols/orca-compound.md:6-8` β€” "Orca's `increase_liquidity` takes limits ...
  Set them too high and it takes the extra from your wallet." The template no longer calls
  `increase_liquidity`; the deposit is the most liquidity the fees buy, and nothing is topped up
  from the wallet.
- `docs/examples/protocols/orca-compound.md:10-12` β€” "The template reads `fee_owed_a` and
  `fee_owed_b` ... before collecting" is still true, but only after the update; without it both
  read 0. "You choose `liquidityAmount` ... when you build the run" is not: the run takes
  `minSqrtPrice` and `maxSqrtPrice` instead.
- `docs/examples/protocols/orca-compound.md:22-24` β€” "Both calls have a `when` condition ... The
  condition looks only at token A's fees." There are three calls now: the update runs while the
  position has liquidity, the collect runs when either fee is above the floor, and the deposit
  runs when both fees are above the floor and the position has liquidity.
- `docs/examples/protocols/orca-compound.md:34-35` and
  `docs/examples/protocols/orca-harvest.md:32-33` β€” "Not yet run against Orca ... no test calls
  Orca." Tests now run both templates against the real program.
- `docs/examples/protocols/orca-harvest.md:10-12` β€” "Sending one `collect_fees` per position
  fails the whole transaction at the first position Orca refuses," given as the reason for the
  `when`. The `when` does not stop a refusal (M5); what reverts the whole harvest is a row
  Whirlpools refuses, regardless of order.
- `docs/examples/protocols/orca-harvest.md:14-15` β€” "the template reads `fee_owed_a` ... and
  calls `collect_fees` only if it is above `dustFloor`." It now updates first, and either fee
  counts.
- `docs/examples/protocols/orca-harvest.md:25` β€” "each position is one row of two accounts, the
  position and its position token account." A row is four accounts now.

Neither page yet says that `tokenOwnerAccountA`/`B` must belong to the signer (M7) or that
`dustFloor` is unsafe at 0 (the compound template's "Remaining limits" above): both pages are
silent on this rather than actively wrong, so they are not listed as false, but a docs pass should
add both.

This revision of the file is the next commit.
