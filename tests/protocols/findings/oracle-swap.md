# `jupiterOracleCheckedSwap` against the real programs

Tests: [`tests/oracle_checked_swap.rs`](../tests/oracle_checked_swap.rs).

The snapshot was taken at slot 451,100,151 (2026-09-27 20:09:42 UTC).

- **Route `solToUsdc`:** 1 SOL for USDC through Meteora DLMM (`HTvjzsfX…`). Jupiter quoted
  123,106,283 USDC units, with a floor of 122,490,752 at 50 bps of slippage.
- **Pyth SOL/USD** (`7UVimff…`): price 12,308,803,053 at exponent −8, which is $123.088. The
  confidence is $0.012, and the price was published 19 s before the snapshot's clock.

The run replaces `route` in Jupiter's own transaction. The transaction starts with Jupiter's compute
budget and setup (create the wrapped SOL account, wrap 1 SOL, create the USDC account). The run
comes next, then Jupiter's cleanup, which closes the wrapped SOL account.

## Runs

| Run | Result |
| --- | --- |
| `toleranceBps` 100 | Lands. The fill is 123,106,283, and the on-chain floor is 121,857,149 |
| The oracle at the highest price the fill clears, 12,434,978,199 | Lands. The floor equals the fill exactly |
| One unit higher, 12,434,978,200 | Fails at `fillBeatTheOracle` (pc 60). The floor is one unit over the fill |
| The oracle 5% above the market (write rule 2) | Fails at `fillBeatTheOracle` (pc 60) |
| USDC/USD's price account passed as the oracle, with `feedId` SOL/USD | Fails at `priceIsTheExpectedFeed` (pc 13) after 2,615 CU, before anything else reads the account |
| The same account, with `feedId` USDC/USD | Lands (a control). No other check tells the two feeds apart |

In the 5%-above run, Jupiter's `route` returned and the requirement failed after it. The runtime
then unwound the swap and the setup's wrap and account creation, so the trader paid only the fee.

## Compute units and size

|  | The run | Jupiter's own transaction |
| --- | --- | --- |
| Transaction | 82,394 CU | 73,084 CU |
| Jupiter's `route` | 40,985 CU | 40,985 CU |
| Ballista's own work (its run less `route`) | 9,310 CU | none |
| Wire size | 835 bytes | 698 bytes |

- **The fill.** The run fills exactly as Jupiter's own transaction does: 123,106,283 units, with the
  same `route` compute units. The template changes what is checked, not what is traded.
- **The feed pin's cost.** It costs 318 CU (the run took 49,977 CU before the pin) and 32 bytes, the
  `feedId` input (803 bytes before).
- **The template's size.** It is 1,232 bytes, up from 1,164. Either is too large for one
  `create_template` transaction, so the harness uploads it in four: `begin_template`, two chunk
  writes, and `finalize_template`.
- **Call depth.** Ballista runs at stack height 1, Jupiter at 2, Meteora at 3, and Meteora's
  Token transfers and event call at 4. Wrapping adds one level. This route stays within the limit
  of 5 that the tests run under: LiteSVM uses mainnet's feature set, in which SIMD-0268's raise to
  9 is not active. A route whose own program calls already reach height 5 under a top-level Jupiter
  would not fit.

## Findings

### P1, fixed: the price account's feed was never checked

**The problem.** The template pinned the price account's owner (the Pyth receiver), its length, and
its verification level, but not which feed it held. The receiver owns every feed's
`PriceUpdateV2` alike, so the template accepted any fully verified price.

**What it allowed.** The test builds mainnet's USDC/USD sponsored feed as a stand-in:
- at its real address, `Dpw1EAVr…`;
- with its real feed id, `eaa020c6…`;
- priced at $0.9999.

It is copied from SOL/USD's account (write rule 2). The unfixed template, run from the payload
committed before the fix, **landed** with this account as the SOL oracle. It valued 1 SOL at $0.99,
so any fill above a dollar passed the fill check.

**The fix,** in both Pyth templates:
- **The input.** A `feedId` input of type `pubkey`.
- **The requirement.** `priceIsTheExpectedFeed` requires
  `accountData(priceUpdate, 41, 'pubkey') == input('feedId')`. Offset 41 is
  `PriceUpdateV2.price_message.feed_id` in the `Full` layout, now `PYTH.feedId` in `shared.ts`.
- **Where it sits.** Right after `priceIsFullyVerified`, because the offset is valid only once the
  verification level is pinned.

Pyth's SDK makes the same check: `get_price_no_older_than` fails with `MismatchedFeedId`.

**What it still allows:**
- **The feed id comes from the run's inputs.** The signer names the feed, and the template holds
  the account to it.
- **The address is not pinned.** A `PriceUpdateV2` for the same feed, posted by anyone through
  `post_update_atomic` under their own write authority, still passes. So does Pyth's SDK.
  Wormhole verifies such an update's price, and `oracleIsFresh` bounds its age. Pinning
  `priceUpdate`'s address to the sponsored feed would be stricter, but would tie the template to
  one feed.

### The floor is exact on chain

The template reads SOL's 9 decimals, USDC's 6 and the feed's exponent of −8, and scales by
10^(6 − 8 − 9) = 10^−11. At the edge price above, the floor equals the fill unit for unit. One
price unit more raises it by one USDC base unit, and the run fails.

So the runtime's `multiplyDivide` and `powerOfTen` opcodes, both rounding steps and the i32
exponent read all agree with `oracle_floor` in the test, which is the template's formula in plain
integer arithmetic.

### The fill check guards a completed swap

The oracle check runs after `route` returns. A fill short of the oracle's valuation reverts a swap
that already happened, together with everything before it in the transaction. Only the fee is
charged, and it includes Jupiter's priority fee, set by the compute-budget instructions. The tests
assert both the revert and the fee.
