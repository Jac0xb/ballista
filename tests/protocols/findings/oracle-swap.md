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
| One unit higher, 12,434,978,200 | Fails at `fillBeatTheOracle` (pc 66). The floor is one unit over the fill |
| The oracle 5% above the market (write rule 2) | Fails at `fillBeatTheOracle` (pc 66) |
| The same oracle, with decoy token accounts where the template measures (write rule 1) | Fails at `soldTheRouteInput` (pc 47) after 49,406 CU, once the route has run. It landed before the fix |
| USDC/USD's price account passed as the oracle, with `feedId` SOL/USD | Fails at `priceIsTheExpectedFeed` (pc 17) after 2,935 CU, before anything else reads the account |
| The same account, with `feedId` USDC/USD | Lands (a control). No other check tells the two feeds apart |

In the 5%-above run, Jupiter's `route` returned and the requirement failed after it. The runtime
then unwound the swap and the setup's wrap and account creation, so the trader paid only the fee.

## Compute units and size

|  | The run | Jupiter's own transaction |
| --- | --- | --- |
| Transaction | 83,003 CU | 73,084 CU |
| Jupiter's `route` | 40,985 CU | 40,985 CU |
| Ballista's own work (its run less `route`) | 9,919 CU | none |
| Wire size | 848 bytes | 698 bytes |

- **The fill.** The run fills exactly as Jupiter's own transaction does: 123,106,283 units, with the
  same `route` compute units. The template changes what is checked, not what is traded.
- **The route-input check's cost.** It costs 609 CU and 13 bytes: the run took 82,394 CU and 835
  bytes before it. The four numbers after the plan travel as 8-byte inputs, 32 bytes where `route`
  packs them into 19.
- **The feed pin's cost.** It cost 318 CU (the run took 49,977 CU before the pin) and 32 bytes, the
  `feedId` input (803 bytes before).
- **The template's size.** It is 1,376 bytes: 1,164 before the feed pin, and 1,232 before the
  route-input check. Each is too large for one `create_template` transaction, so the harness
  uploads it in four: `begin_template`, two chunk writes, and `finalize_template`.
- **Call depth.** Ballista runs at stack height 1, Jupiter at 2, Meteora at 3, and Meteora's
  Token transfers and event call at 4. Wrapping adds one level. This route stays within the limit
  of 5 that the tests run under: LiteSVM uses mainnet's feature set, in which SIMD-0268's raise to
  9 is not active. A route whose own program calls already reach height 5 under a top-level Jupiter
  would not fit.

## Findings

### P1, fixed: decoy token accounts made the fill check pass at any price

**The problem.** The template measured `sold` as the drop in `sourceAta`, and required
`destinationAta` to rise by at least `fairOut`, which is proportional to `sold`. It passed `route`'s
arguments through as opaque bytes, so it never read `in_amount`. And Jupiter does not tie `route`'s
source and destination positions to the accounts its steps move. The Jito work pinned what Jupiter
checks there (finding 4 of `findings/jito-tip.md` on `claude/pt-jito`, 0df3202):
- the source position must hold at least `in_amount`, of any mint and any owner (6024 otherwise);
- the destination position must hold `destination_mint` (6019 otherwise);
- accounts that pass are never moved. The steps' own accounts are.

So accounts the route never touches could sit where the template measures.

**What it allowed.** `decoys_where_the_template_measures_fail_at_sold_the_route_input` makes two
decoys under write rule 1: another wallet's wrapped SOL account, holding `in_amount` (1 SOL), and
its empty USDC account. It puts them at `sourceAta` and `destinationAta`, leaves the route's step
accounts as the Swap API built them, and moves the oracle 5% above the market. Against the unfixed
template the transaction **landed** (82,410 CU, 899 bytes):
- the decoys still held 1,000,000,000 and 0;
- the trader's wrapped SOL account was emptied and closed, and its USDC account took the fill,
  123,106,283;
- the fill check compared 0 received with a floor of 0. Measured on the trader's own accounts, the
  floor at that price is 127,950,007, and the same fill fails `fillBeatTheOracle`.

**The fix:**
- **The inputs.** The route arrives in parts, as `tokenSweepIntoSwap` takes it: `routePlan`, then
  `inAmount`, `quotedOutAmount`, `slippageBps` and `platformFeeBps`. The four are u64 inputs,
  written into `route`'s data as u64, u64, u16 and u8. A slippage or fee too large for its field
  fails with `ArithmeticOverflow`; it is never truncated. `splitJupiterRoute` in `shared.ts` splits
  the Swap API's data into these parts.
- **The requirement.** `soldTheRouteInput` requires `sold == inAmount`, right after
  `measureAmountSold` and before the floor is computed.

**`==` held.** On the real route exactly `in_amount`, 1,000,000,000, left the trader's wrapped SOL,
and every other run lands or fails as before. Every route in the snapshot is one step at 100%, so a
split route's rounding is untested. If a split ever sold less than `in_amount`, `==` would refuse
the route.

### Open: the route can still spend the signer's other token accounts

The fix ties the measured source to the route's input. It does not stop the route from moving other
accounts:
- **Why.** The trader signs `route` as `user_transfer_authority`, and Jupiter passes that authority
  to every step. A step can debit any token account the trader owns, not only `sourceAta`.
- **An example.** A two-step route sells `inAmount` of SOL from `sourceAta` for USDT, and pays the
  USDT into someone else's account. Its second step debits the trader's own USDT account instead,
  and swaps that into `destinationAta`. The template sees `inAmount` sold and a fair fill, and
  passes. But the trader paid for the fill with its own USDT, and the SOL's proceeds went to someone
  else. This is inferred from the finding above; no test runs it.
- **What would see it.** An authority that can spend only what the template allows. For example,
  approve a delegate on `sourceAta` for exactly `inAmount`, pass it to `route` in the trader's
  place, and revoke it afterwards. The route could then spend nothing else the trader owns.
- **Why it stays open.** Template CPIs carry no signer seeds today, so the delegate would be a
  keypair that signs the transaction, or would need a new capability. And a multi-hop route passes
  its intermediate amounts through token accounts that its authority must control. This is a design
  question, left open here.

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

## Stale docs

`docs/` was out of bounds for this change. `docs/examples/protocols/jupiter-oracle-swap.md` still
says these things, none of them true now:
- The template requires the feed's exponent to equal `priceExponent`, and the caller computes
  `scaleDivisor`. Neither input exists: the template reads the exponent and both mints' decimals
  on chain.
- Because the template passes `route`'s first four accounts itself, "the balances it measures are
  the ones Jupiter moves". Jupiter moves the accounts its steps name.
- `routeArgs` is Jupiter's instruction data after the discriminator. The route now arrives as
  `routePlan` and four numbers.
- Its run has the same form as the Jupiter deposit's, which the page's Rust tab shows. Their
  inputs now differ.
- No test calls Jupiter. `docs/examples/protocols/index.md` says the same of every template.

The page's step list also lacks the feed pin, the two mint checks and `soldTheRouteInput`.
