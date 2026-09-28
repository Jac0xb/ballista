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
| One unit higher, 12,434,978,200 | Fails at `fillBeatTheOracle` (pc 74). The floor is one unit over the fill |
| The oracle 5% above the market (write rule 2) | Fails at `fillBeatTheOracle` (pc 74) |
| The same oracle, with the trader's own second token accounts where the template measures | Fails at `soldTheRouteInput` (pc 55) after 49,984 CU, once the route has run. Another wallet's decoys landed before `soldTheRouteInput` existed |
| Another wallet's wrapped SOL at `sourceAta` (write rule 1) | Fails at `sellsTheTradersOwnTokens` (pc 34) after 4,210 CU, before the route |
| An attacker's USDC account at `destinationAta` and as the step's output (write rule 1), the oracle at the market | Fails at `proceedsGoToTheTrader` (pc 38) after 4,482 CU, before the route. It landed before the fix |
| The attacker's account as the step's output only, the trader's at `destinationAta` | Fails at `fillBeatTheOracle` (pc 74): nothing arrived where the template measures |
| USDC/USD's price account passed as the oracle, with `feedId` SOL/USD | Fails at `priceIsTheExpectedFeed` (pc 17) after 2,959 CU, before anything else reads the account |
| The same account, with `feedId` USDC/USD | Lands (a control). No other check tells the two feeds apart |

In the 5%-above run, Jupiter's `route` returned and the requirement failed after it. The runtime
then unwound the swap and the setup's wrap and account creation, so the trader paid only the fee.

The program counters and the compute units at each failure are one-off readings of the failures'
logs, not printed by the tests.

## Compute units and size

|  | The run | Jupiter's own transaction |
| --- | --- | --- |
| Transaction | 83,581 CU | 73,084 CU |
| Jupiter's `route` | 40,985 CU | 40,985 CU |
| Ballista's own work (its run less `route`) | 10,497 CU | none |
| Wire size | 848 bytes | 698 bytes |

- **The fill.** The run fills exactly as Jupiter's own transaction does: 123,106,283 units, with the
  same `route` compute units. The template changes what is checked, not what is traded.
- **The owner checks' cost.** They cost 578 CU: the transaction took 83,003 CU before them, and
  Ballista's run 50,904. They add 128 bytes to the template and none to the transaction, which
  already passes the trader's key.
- **The route-input check's cost.** It costs 609 CU and 13 bytes: the run took 82,394 CU and 835
  bytes before it. The four numbers after the plan travel as 8-byte inputs, 32 bytes where `route`
  packs them into 19.
- **The feed pin's cost.** It cost 318 CU (the run took 49,977 CU before the pin) and 32 bytes, the
  `feedId` input (803 bytes before).
- **The template's size.** It is 1,504 bytes: 1,376 before the owner checks, 1,232 before the
  route-input check, and 1,164 before the feed pin. Each is too large for one `create_template`
  transaction, so the harness uploads it in four: `begin_template`, two chunk writes, and
  `finalize_template`.
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

**What it allowed.** The test made two decoys under write rule 1: another wallet's wrapped SOL
account, holding `in_amount` (1 SOL), and its empty USDC account. It put them at `sourceAta` and
`destinationAta`, left the route's step accounts as the Swap API built them, and moved the oracle 5%
above the market. Against the template before this fix the transaction **landed** (82,410 CU,
899 bytes):
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

**The test now uses the trader's own decoys.** Since the owner checks below, another wallet's
decoys fail at `sellsTheTradersOwnTokens` before the route runs.
`decoys_where_the_template_measures_fail_at_sold_the_route_input` now makes the decoys the trader's
own second wrapped SOL and USDC accounts, created through the System and Token programs' own
instructions rather than written. They pass the owner checks, and `soldTheRouteInput` still stops
them after the route (pc 55, 49,984 CU; pc 47 and 49,406 CU before the owner checks).

**`==` held.** On the real route exactly `in_amount`, 1,000,000,000, left the trader's wrapped SOL,
and every other run lands or fails as before. Split routes and platform fees take exactly
`in_amount` too: see "Splits and platform fees take exactly `in_amount`" below.

### P1, fixed: a hostile route could pay the fill to another wallet

**The problem.** The template checked both token accounts' program owner (SPL Token), size and
mint, but never whose they were. Jupiter checks `route`'s destination position by its mint alone,
and a step pays whichever account it names. So a route could pay the fill into another wallet's
account of the destination mint, and put that account at `destinationAta`, where the fill check
would find it. A reviewer first showed this on chain.

**What it allowed.** `an_attackers_destination_fails_at_proceeds_go_to_the_trader` writes an
attacker's empty USDC account under write rule 1. It puts the account at `destinationAta`, and in
the trader's place as the Meteora step's `user_token_out`, position 15 of `route`'s accounts. The
oracle is at the market. Against the template before this fix (1,376 bytes), the transaction
**landed** (83,003 CU, 880 bytes):
- the attacker's USDC account went from 0 to 123,106,283, the whole fill;
- the trader's wrapped SOL account was emptied and closed, and its USDC account held 0;
- the trader's lamports fell by 1,002,144,280: the 1 SOL sold, the 2,039,280 rent of the USDC
  account the setup created, and the 105,000 fee.

Every check passed. Exactly `inAmount` left the trader's source, and the fill, measured in the
attacker's account, cleared the floor of 121,857,149.

**The fix.** Two requirements right after the mint checks, before anything is measured:
- `sellsTheTradersOwnTokens` requires `accountData(sourceAta, 32, 'pubkey')` to equal the trader's
  key;
- `proceedsGoToTheTrader` requires the same of `destinationAta`.

Offset 32 is the SPL Token account's `owner` (`TOKEN_ACCOUNT_OWNER_OFFSET` in `shared.ts`).
`jitoProfitGuardedTip` makes the same check in `searcherOwnsTheWsolAccount`. The source's check
stops no attack found here. It makes the template state that both ends of the swap are the
trader's.

**Now:**
- **The attack** fails at `proceedsGoToTheTrader` before Jupiter is called. The attacker's account
  still holds 0, and the trader pays only the fee.
- **The attacker's account as the step's output only**, with the trader's own at
  `destinationAta`: the fill check sees nothing arrive and fails at `fillBeatTheOracle`.
- **Another wallet's wrapped SOL at `sourceAta`** fails at `sellsTheTradersOwnTokens`. Before the
  fix it failed at `soldTheRouteInput` (pc 47), after the route had run.

### Open: the signer's authority over its other token accounts

Both ends of the swap are now the trader's, and exactly `inAmount` must leave the source. What
remains open is the rest of what the trader's signature authorizes:
- **Why.** The trader signs `route` as `user_transfer_authority`, and Jupiter passes that authority
  to every step. A step can debit any token account the trader owns, not only `sourceAta`, and pay
  any account.
- **An example.** A two-step route sells `inAmount` of SOL from `sourceAta` for USDT, and pays the
  USDT to an attacker. Its second step debits the trader's own USDT account instead, and swaps that
  into `destinationAta`. The template sees `inAmount` sold from the trader's source and a fair fill
  in the trader's destination, and passes. But the trader paid for the fill with its own USDT, and
  the SOL's proceeds went to the attacker. This is inferred from the findings above; no test runs
  it.
- **What would close it.** A delegate approved only on the source: approve it on `sourceAta` for
  exactly `inAmount`, pass it to `route` in the trader's place, and revoke it afterwards. The route
  could then spend nothing else the trader owns.
- **Why it stays open.** Template CPIs carry no signer seeds today, so the delegate would be a
  keypair that signs the transaction, or would need a new capability. And a multi-hop route passes
  its intermediate amounts through token accounts that its authority must control. This is a design
  question, left open here.

### Open: the tolerance is a budget a hostile route can spend

`route`'s `platform_fee_account` (position 6) is chosen by whoever builds the run, as are
`platformFeeBps`, `slippageBps` and `quotedOutAmount`. Nothing checks who owns it. Jupiter's own
6001 refusal below is no defense, since the builder also writes the quote it checks against (with
`quotedOutAmount` set to 1, the same fee landed at 50 bps). The fill check bounds what it can take,
but only to `toleranceBps`.

A one-off probe at the snapshot's slot, not a committed test, ran the fixed template at the market
with `toleranceBps` 100 and an attacker's USDC account as the platform fee account:
- **A fee of 100 bps, `slippageBps` 50:** Jupiter's own slippage check refused it (6001).
- **100 bps with `slippageBps` 200:** the run **landed**. The attacker took 1,231,062 units, and
  the trader's 121,875,221 cleared the floor of 121,857,149.
- **150 bps with `slippageBps` 200:** it failed at `fillBeatTheOracle`.

So a trader should set `toleranceBps` to what it will accept losing to the builder, not only to
the market. Venues' own fee accounts, such as Meteora's `host_fee_in`, sit in the same place: in
the route's accounts, without an owner check. Nothing here tested them; the fill check would bound
them the same way.

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

### Splits and platform fees take exactly `in_amount`

`soldTheRouteInput` requires exactly `in_amount` to leave the source. A split route or a platform
fee could, in principle, leave some of it behind or take more. Every route in the snapshot is one
step at 100%, so `jupiter_takes_exactly_in_amount_through_a_split_or_a_platform_fee` sends
Jupiter's `route` on its own, with a quoted output of 1 so that its slippage check never refuses:
- **Splits.** The route's step split in two over the same pool, each half with its own copy of the
  step's accounts, selling an odd 999,999,999 lamports. At [50, 50] and at [33, 67] exactly that
  much left the trader's wrapped SOL, for 123,106,282 USDC units, so the last step takes the
  remainder. A [50, 100] plan is refused with Jupiter 6010.
- **A 100 bps platform fee** also takes exactly `in_amount`. The fee account's mint decides where
  the fee comes from:
  - **wrapped SOL:** out of `in_amount`, 10,000,000 lamports, and the pool swaps the rest;
  - **USDC:** out of the fill, 1,231,062 of the 123,106,283 units.
- **Untested:** venues that leave some of their input unspent. `==` refuses such a route, so it
  fails closed.

A reviewer's probe first made these runs; the test repeats them.

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

`docs/` was out of bounds for this change. These claims in it are no longer true.

`docs/examples/protocols/jupiter-oracle-swap.md`:
- **6–9.** It says an independent price catches "a route built by someone other than the signer".
  The price alone did not: it cleared a fill paid to an attacker. The owner checks and
  `soldTheRouteInput` now do that part. A hostile route can still take up to `toleranceBps` through
  its own fee account, and can spend the signer's other token accounts.
- **11–21.** The step list lacks the feed pin (`priceIsTheExpectedFeed`), the two mint checks, the
  two owner checks (`sellsTheTradersOwnTokens`, `proceedsGoToTheTrader`) and `soldTheRouteInput`.
- **16, 23–27.** The template does not require the feed's exponent to equal `priceExponent`, and the
  caller does not compute `scaleDivisor`. Neither input exists: the template reads the exponent and
  both mints' decimals on chain.
- **29–31.** Because the template passes `route`'s first four accounts itself, "the balances it
  measures are the ones Jupiter moves". Jupiter moves the accounts its steps name. The template now
  requires both token accounts to be the signer's, and exactly `inAmount` to leave the source.
- **32–33.** `routeArgs` is Jupiter's instruction data after the discriminator. The route now
  arrives as `routePlan` and four numbers.
- **49–51.** Its run has the same form as the Jupiter deposit's, which the page's Rust tab shows.
  Their inputs now differ.
- **53–55.** "Not yet run against Jupiter", and "No test calls Jupiter".

`docs/examples/protocols/index.md`:
- **8–9, 31–33, 45–46 and 117.** Each says no template has been run against the real protocols, or
  has measured costs. This one has, against Jupiter v6, Meteora DLMM and Pyth at the slot above.
- **68–78.** The offsets table gives only the SPL Token account's `amount` (64). The templates now
  also read its `mint` (0) and `owner` (32).
- **90–92.** "Require an owner and a minimum data length for every account a template reads" means
  the program that owns the account. A template that measures a token account should also require
  that account's `owner` field to be the signer.
