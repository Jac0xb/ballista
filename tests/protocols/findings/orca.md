# Orca: what the templates did against the real Whirlpool program

Milestone 2 of the real-protocol tests ran `orcaCompoundFees` and `orcaHarvestManyPositions` as
signed transactions against mainnet's Whirlpool program in LiteSVM. Neither worked as written, and
both are fixed. Every fix except M5's, a rewritten header, landed with a test that failed before
it; the findings table says what shows each one. Plan:
[2026-09-27-protocol-orca](../../../docs/superpowers/plans/2026-09-27-protocol-orca.md).

- **Snapshot:** `tests/protocols/snapshot-orca/`, slot 451,137,027 (2026-09-27 22:54:57 UTC).
  - Pools: SOL/USDC `Czfq…` (tick spacing 4) for the compounder, and SOL/USDC `HJPj…` (tick
    spacing 64) for the harvest. Both were near tick −21,090, about 121 USDC per SOL.
  - Programs, all from mainnet: Whirlpool, Token, Token-2022, Associated Token, Memo.
- **Tests:**
  - `orca_snapshot.rs`: the pools and their tick arrays are in the snapshot.
  - `orca_setup.rs`: the Orca behaviors the fixes rest on, called without Ballista.
  - `orca_cpis.rs`: every Whirlpool call against Orca's own client.
  - `orca_compound_fees.rs`: 11 tests, three of them with a real `dustFloor`.
  - `orca_harvest_many_positions.rs`: 5 tests, one with a real `dustFloor`.
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
- Step labels: `readLiquidity`, `updateFees`, `readFeesOwedA`, `readFeesOwedB`, `collectFees`,
  `compoundFees`.
- The payload grew from 440 to 714 bytes.
- `shared.ts` loses `ORCA_INCREASE_LIQUIDITY`. It gains `MEMO_PROGRAM`,
  `ORCA_UPDATE_FEES_AND_REWARDS`, `ORCA_INCREASE_LIQUIDITY_BY_TOKEN_AMOUNTS_V2` and
  `ORCA_BY_TOKEN_AMOUNTS`.

**Claims on `docs/examples/protocols/orca-compound.md` that are now false.** These are quoted from
this branch; the docs session's copy may have moved on.
- "Orca's `increase_liquidity` takes limits ... Set them too high and it takes the extra from your
  wallet."
  - The template no longer calls `increase_liquidity`.
  - The deposit is the most liquidity the fees buy, and nothing is topped up from the wallet.
- "The template reads `fee_owed_a` and `fee_owed_b` ... before collecting." Still true, but only
  after the update. Without it, both read 0.
- "You choose `liquidityAmount`, the liquidity to add, when you build the run." The run takes
  `minSqrtPrice` and `maxSqrtPrice` instead.
- "Both calls have a `when` condition: if token A's fees are not above `dustFloor`, both calls are
  skipped ... The condition looks only at token A's fees."
  - There are three calls now.
  - The update runs while the position has liquidity.
  - The collect runs when either fee is above the floor.
  - The deposit runs when both fees are above the floor and the position has liquidity.
- "Not yet run against Orca ... no test calls Orca." Tests now run it against the real program.
- The page includes the template's source, so the new header is shown without an edit.

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
  needs more; a few thousand is a safe default.
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
- The payload grew from 298 to 478 bytes.
- Step labels: `updateIfLiquid`, `collectIfWorthIt`, `everyPosition`.

**M5, for the docs: what the guard is for.**
- The guard saves compute, about 11,000 CU per skipped collect, and leaves dust alone.
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
  - changed: `describeFailure(code, logs)` now takes the failed transaction's logs (M6).
- `clients/rust/examples/protocol_runs.rs`:
  - The `#rows` region, which the harvest page includes, has a new `HarvestRow` struct, and
    `run_orca_harvest` takes `rows: &[HarvestRow]`.
  - `cargo run -p ballista-sdk --example protocol_runs` prints 29 accounts for 5 rows (was 19).

**Claims on `docs/examples/protocols/orca-harvest.md` that are now false.**
- "Sending one `collect_fees` per position fails the whole transaction at the first position Orca
  refuses." This is given as the reason for the `when`, but the `when` does not stop a refusal
  (M5).
- "the template reads `fee_owed_a` from the position account during the run and calls
  `collect_fees` only if it is above `dustFloor`." It now updates first, and either fee counts.
- "each position is one row of two accounts, the position and its position token account." A row
  is four accounts now.
- "Not yet run against Orca ... no test calls Orca."

**Limits, measured.**
- **Ballista's own limits hold with room.**
  - A stride of 4 row accounts (limit 8).
  - 8 fixed accounts plus 4 × 12 rows = 56 runtime accounts (limit 120).
  - 2 calls × 12 rows = 24 CPIs (limit 64).
- **Compute:** about 23,600 CU per earning row.
  - Eight earning rows land under the default 200,000 with no budget instruction: 190,283 CU.
  - Nine exhaust it.
- **Size:** 68 bytes per earning row when rows share tick arrays. Each extra distinct tick array
  costs 32 bytes more.
  - Ten earning rows fit a legacy transaction with a compute-budget instruction: 1,195 bytes.
  - Eleven are 1,263 bytes, and need a lookup table. So does twelve, the template's limit.
- **The old two-account row never reached its limit either.** Eleven rows were 1,169 bytes and
  twelve were 1,235 bytes, so the declared maximum of 12 never fit a legacy transaction.

## M6: which program refused

- Whirlpool's codes are Anchor's, numbered from 6000 like Ballista's. Read as Ballista's:
  - 6012 (`LiquidityZero`) is `TypeMismatch`;
  - 6017 (`TokenMaxExceeded`) is `InvalidPdaDerivation`;
  - 6019 (`MissingOrInvalidDelegate`) is `ReturnDataMismatch`.
- The tests assert the failing program from the logs (`Failure.program`), never by the code
  alone.
- `run-orca-harvest.ts::describeFailure` takes the logs, and names Whirlpools for its own codes.
- The plan left one item to the program's owner, the doc comment on `RunError` in
  `programs/ballista/src/processor/execute.rs`, which said invoked programs' codes are "never
  confused with Ballista's". Runtime phase 1 (1efbd24) has since rewritten it to say a code alone
  does not name its program. Nothing is left to do there.

## CI

- No protocol-test CI job exists on this branch, so there is no LFS cache key to widen.
- Whoever adds the job should:
  - key the LFS cache on `hashFiles('tests/protocols/snapshot*/manifest.json')`, so both
    snapshots, and later ones, invalidate it;
  - run `git lfs pull`, `cargo build-sbf --manifest-path programs/ballista/Cargo.toml` and
    `cargo test --manifest-path tests/protocols/Cargo.toml`.
- `.gitattributes` now keeps `tests/protocols/snapshot*/programs/*.so` in LFS.

## Measurements

From `cargo test --manifest-path tests/protocols/Cargo.toml -- --nocapture --test-threads=1`. The
rows the tests do not print were measured with the same setups in a scratch crate. Compute units
include Ballista's own work; sizes are the signed transaction on the wire.

| Run | Compute units | Bytes | Whirlpool calls |
| --- | --- | --- | --- |
| compound, fees in both tokens | 43,382 | 706 | 3 |
| compound, fees in one token | 25,202–25,209 | 706 | 2 |
| compound, no fees | 11,882 | 706 | 1 |
| compound, no liquidity | 2,546 | 706 | 0 |
| compound, emptied position | 15,655 | 706 | 1 |
| harvest, the four rows | 59,610 | 747 | 5 |
| harvest, ten earning rows | 237,622 | 1,195 (budget instruction included) | 20 |
| harvest, per earning row | 23,592–23,595 | 68 | 2 |
| Orca: `update_fees_and_rewards` | 7,687–8,184 | | |
| Orca: `collect_fees`, fees owed | 11,346–11,390 | | |
| Orca: `collect_fees`, nothing owed | 11,375 | | |
| Orca: `increase_liquidity_by_token_amounts_v2` | 15,281 | | |

- The planning run (slot 451,125,511) measured the two-sided compound at 42,991 CU and 674 bytes.
- The 32 bytes are one tick array. At this snapshot the price sits 30 ticks above its array's first
  tick, so the ±60-tick position's bounds fall in two arrays rather than one. Everything else is
  within a few hundred CU of the planning run.
- In the two-sided run the fees were (13,919,999 lamports, 2,087,999 micro-USDC).
  - The liquidity added equals the smaller of Orca's two quotes, exactly.
  - 327,114 micro-USDC stayed in the wallet.

## Evidence

The tests written before each fix, failing as run at this snapshot. M5 has none: its fix is the
header.

M1, `orca_cpis.rs`, before the flag fix:

```text
orcaCompoundFees collect_fees: whirlpool is writable here and read-only in Orca's client
orcaHarvestManyPositions collect_fees: whirlpool is writable here and read-only in Orca's client
```

M2, `orca_compound_fees.rs` against the template as written. 7 of 8 fail;
`a_position_without_liquidity_lands` passes:

```text
two_sided_fees_are_collected_and_compounded: liquidity added left: 0, right: 1599965546
fees_in_token_a_alone_are_collected_not_compounded: the whole fee reached the wallet; left: (0, 0), right: (3479999, 0)
fees_in_token_b_alone_are_collected_not_compounded: the whole fee reached the wallet; left: (0, 0), right: (0, 347999)
no_fees_lands_without_collecting: the update only; left: 0, right: 1
a_price_move_inside_the_bounds_still_lands: assertion failed: state(&setup).liquidity > before.liquidity
a_price_outside_the_bounds_fails_in_whirlpools: `unwrap_err()` on an `Ok` value: landed: 1821 CU, 591 bytes
an_emptied_position_is_collected_not_refilled: left: PositionState { liquidity: 1599965546, fee_owed_a: 0, fee_owed_b: 0 }, right: PositionState { liquidity: 0, .. }
```

The update, added without its liquidity guard:

```text
a_position_without_liquidity_lands: whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc failed with code 6012 (0x177c)
```

M3 and M4, the update guarded, collect and deposit still guarded on token A:

```text
fees_in_token_a_alone_are_collected_not_compounded: whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc failed with code 6017 (0x1781)
fees_in_token_b_alone_are_collected_not_compounded: the whole fee reached the wallet; left: (0, 0), right: (0, 347999)
```

After M3 (collect on either fee), token A alone still failed with 6017, until M4's
`increase_liquidity_by_token_amounts_v2`.

M2, `orca_harvest_many_positions.rs` against the template as written. 4 of 4 fail:

```text
only_the_rows_that_earned_collect: left: (0, 0), right: (47454547, 4971430)
a_stranger_cannot_collect_and_whirlpools_says_so: `unwrap_err()` on an `Ok` value: landed: 1927 CU, 543 bytes
a_row_from_another_pool_reverts_the_whole_harvest: `unwrap_err()` on an `Ok` value: landed: 2432 CU, 675 bytes
ten_rows_fit_one_legacy_transaction: Whirlpool calls left: 0, right: 20
```

Each row updated, unguarded: the row without liquidity reverted the whole harvest.

```text
Program log: Instruction: UpdateFeesAndRewards
Program log: AnchorError occurred. Error Code: LiquidityZero. Error Number: 6012. Error Message: Liquidity amount must be greater than zero.
Program whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc failed: custom program error: 0x177c
```

M3, the update guarded, the collect still on token A. The B-only row's fee stayed behind:

```text
only_the_rows_that_earned_collect: left: (47454547, 2485715), right: (47454547, 4971430)
```

M6, `describeFailure` before it read the logs:

```text
blames Whirlpools for its own code, though Ballista uses the same number:
  Expected: "code 6019 came from Whirlpools, not Ballista"
  Received: "ReturnDataMismatch at inputs.dustFloor"
```

A stranger's harvest, as the real program refuses it:

```text
Program log: Instruction: CollectFees
Program log: AnchorError occurred. Error Code: MissingOrInvalidDelegate. Error Number: 6019. Error Message: Position token account has a missing or invalid delegate.
Program whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc failed: custom program error: 0x1783
Program BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD failed: custom program error: 0x1783
```

The floor tests and the ordered calls came after the fixes, so they have no failure from before
one. A mutation check stands in. With both templates' floor comparisons weakened to `>=`, all four
floor tests fail. So does `only_the_rows_that_earned_collect`, on its calls alone: the idle rows'
empty collects left every balance and account as it was.

```text
  left: ["UpdateFeesAndRewards", "CollectFees", "UpdateFeesAndRewards", "CollectFees", "UpdateFeesAndRewards", "CollectFees", "CollectFees"]
 right: ["UpdateFeesAndRewards", "CollectFees", "UpdateFeesAndRewards", "CollectFees", "UpdateFeesAndRewards"]
```

## Commits

On `claude/protocol-orca`, after 461d230:

| Commit | |
| --- | --- |
| a85da56 | Plan the Orca templates' tests against the real Whirlpool program |
| 16203fd | Add Orca's crates to the protocol tests and keep every snapshot's programs in LFS |
| c073fdb | Snapshot both SOL/USDC Whirlpools for the Orca tests |
| c6f0452 | Add the Orca module to the protocol-test harness |
| d8b8e8b | Pin the Whirlpool behaviors the Orca templates depend on |
| 603eb4c | Pass the whirlpool read-only to collect_fees, as Orca declares it (M1) |
| 87ca59b | Update an Orca position's fees before compounding them, and reinvest by token amounts (M2, M3, M4) |
| 28755bf | Update each Orca position before harvesting it, and collect either fee (M2, M3, M5) |
| e28e05b | Name the program that refused an Orca harvest from its logs (M6) |
| 2e7ba70 | Document the Orca snapshot |
| 11cadf8 | Record what the Orca templates did against the real program |
| eaa4546 | Pin each Orca run's Whirlpool calls in order, and test a real dustFloor |

This revision of the file is the next commit.
