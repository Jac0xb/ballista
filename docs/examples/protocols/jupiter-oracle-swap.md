# Swap checked against an oracle

<p class="protocol-line">Jupiter · Pyth</p>

**Status:** Tested locally in [LiteSVM](https://github.com/LiteSVM/litesvm) against Jupiter, Meteora
and Pyth programs and accounts copied from mainnet; not yet run on devnet or mainnet.

**Cost:** Ballista's own work took 9,770 of the tested transaction's 82,854
[compute units](/reference/glossary#compute-units); the protocols took the rest. Ballista charges no
fee; see [what it costs](/guide/why-ballista#cost).

## What it does

Sells SOL for USDC through Jupiter, and reverts unless the swap paid at least what Pyth's SOL/USD
price says the SOL was worth, less 1%.

Jupiter's `slippageBps` limits how far the swap can fall short of the quote Jupiter produced. It
doesn't help when the quote itself is the problem, such as a bad quote or a manipulated pool inside
the route. An independent price catches those.

The template requires, in order:

- the Pyth price account's verification level to be `Full` (`priceIsFullyVerified`), because the
  level decides where every other field sits (see [reading offsets](/examples/protocols/#reading-offsets-from-an-account));
- the account to hold SOL/USD's feed, `FEED_ID` (`priceIsTheExpectedFeed`). Pyth's receiver owns
  every feed's price account, so otherwise USDC/USD's price would pass for SOL/USD's;
- the price to be at most 60 seconds old (`oracleIsFresh`);
- the source to hold wrapped SOL and the destination USDC, the pair the feed prices, read from mint
  accounts pinned to those two (`sourceHoldsTheSourceMint`, `destinationHoldsTheDestinationMint`);
- both token accounts to belong to the trader (`sellsTheTradersOwnTokens`, `proceedsGoToTheTrader`),
  since a route's step can pay any account of the output mint;
- the price to be above zero (`oraclePriceIsPositive`);
- the route's `platformFeeBps` to be at most `MAX_PLATFORM_FEE_BPS`, a constant that is 0
  (`platformFeeWithinCap`), before Jupiter is called, since whoever builds the run picks the fee
  account and rate;
- after the swap, exactly `inAmount` to have left the source (`soldTheRouteInput`), since Jupiter
  doesn't require its steps to move the source account it is given;
- the destination to have received at least that amount's value at the Pyth price, less
  `TOLERANCE_BPS`, 100 basis points or 1% (`fillBeatTheOracle`).

The feed, the pair and the tolerance are constants, so whoever builds the run can't change them.
For another pair or tolerance, change them and upload your own template.

It does not guard against:

- **Venues' own fee accounts.** The cap covers the route's platform fee, not fees a venue takes
  inside the route. The fill check bounds those to `TOLERANCE_BPS`, so set it to what you would
  accept losing to the builder, not only to the market.
- **A price the publishers disagree on.** The template doesn't read Pyth's confidence interval.
  [Act only on a fresh price](/examples/protocols/pyth-gate) shows the check.
- **The choice of price within the last minute.** Whoever posts the price update can pick any
  Pyth price from the last 60 seconds, such as the lowest.
- **Spending the trader's other token accounts.** The trader signs `route`, and Jupiter passes that
  authority to every step.

## Template

::: code-group

<<< @/../clients/js/examples/protocols/jupiter-oracle-checked-swap.ts#template [TypeScript · Template]

<<< @/../clients/rust/examples/protocol_templates.rs#jupiter-oracle-swap [Rust · Template]

<<< @/../clients/js/examples/protocols/run/jupiter-oracle-swap.ts [TypeScript · Run]

<<< @/../clients/rust/examples/protocol_templates_run.rs#jupiter-oracle-swap [Rust · Run]

:::

The Rust template takes its program addresses, `token_account()`, `balance_of()` and
`jupiter_route_data()` from the [shared helpers](/examples/protocols/#rust-helpers).

SOL/USD prices the SOL sold in the USDC bought, counting a USDC as a dollar. Pyth publishes a price
as `price × 10^exponent` per whole token. The template reads the exponent and both mints' decimals
on chain, and values what was sold, in the destination token's smallest units, as
`sold × price × 10^(destinationDecimals + exponent − sourceDecimals)`.

## Run it

Jupiter's `route` starts its account list with the token program, the signer, and the signer's
source and destination token accounts. The template passes those four itself; the rest of the
route's accounts arrive as the `routeAccounts` [account group](/guide/accounts-and-cpis#account-groups).

The Run tabs pass the eight declared accounts, `jupiter`, `tokenProgram`, `priceUpdate`, `trader`,
`sourceAta`, `destinationAta`, `sourceMint` (wrapped SOL's mint) and `destinationMint` (USDC's),
then the inputs `routePlan`, `inAmount`, `quotedOutAmount`, `slippageBps` and `platformFeeBps`,
then the group. `splitJupiterRoute` (TypeScript) and `RouteQuote::split` (Rust) split the Swap
API's `route` data into `routePlan` and the four numbers after it.

`priceUpdate` is a fully verified `PriceUpdateV2` carrying `FEED_ID`, SOL/USD's feed id
`ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d`: the account Pyth's push oracle
keeps for it, `7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE`, or an update you post with Pyth's
receiver earlier in the transaction.

[Getting a Jupiter route](/examples/protocols/#jupiter-routes) says how to request the route from
Jupiter's Swap API and what to keep from its response.

## What has been tested

- **In LiteSVM.** `tests/protocols/tests/oracle_checked_swap.rs` sells 1 SOL for
  USDC through Jupiter and a Meteora pool, valued at Pyth's SOL/USD price, in place of `route` in
  the transaction Jupiter's API built. It fills exactly as that transaction does alone, for 110
  more bytes. Its floor matches the formula above to the last unit. Jupiter's `route` on its own
  takes exactly `in_amount` through a split route and with a platform fee, as `soldTheRouteInput`
  needs.
- **Failures.** Each of these fails, and the whole transaction reverts: an oracle 5% above the
  market (`fillBeatTheOracle`, after the swap); spare token accounts of the trader's at `sourceAta`
  and `destinationAta` while the route moves others (`soldTheRouteInput`); another wallet's source
  or an attacker's destination (`sellsTheTradersOwnTokens`, `proceedsGoToTheTrader`, before Jupiter
  is called); USDC/USD's price account passed as SOL/USD's (`priceIsTheExpectedFeed`); USDC's mint
  as `sourceMint` (its pinned address); a route that charges a platform fee (`platformFeeWithinCap`,
  before Jupiter is called).
- An opt-in test checks the Pyth offsets against devnet accounts.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
