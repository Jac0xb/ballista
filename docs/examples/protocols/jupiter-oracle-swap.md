# Swap checked against an oracle

<p class="protocol-line">Jupiter · Pyth</p>

**Status:** Run as real transactions against Jupiter, a Meteora pool and Pyth's SOL/USD price,
copied from mainnet at one slot into LiteSVM, a local Solana runtime. Not yet run on mainnet itself.

## What it does

Swaps through Jupiter and reverts unless the swap paid at least what a Pyth price says the sold
tokens were worth, less a tolerance.

Jupiter's `slippageBps` limits how far the swap can fall short of the quote Jupiter produced. It
doesn't help when the quote itself is the problem, such as a bad quote or a manipulated pool inside
the route. An independent price catches those.

The template requires, in order:

- the Pyth price account's verification level to be `Full` (`priceIsFullyVerified`), because the
  level decides where every other field sits (see [reading offsets](/examples/protocols/#reading-offsets-from-an-account));
- the account to hold the feed you name in `feedId` (`priceIsTheExpectedFeed`). Pyth's receiver
  owns every feed's price account, so otherwise USDC/USD's price would pass for SOL/USD's;
- the price to be at most 60 seconds old (`oracleIsFresh`);
- each token account to hold the mint passed for it, whose decimals the template reads
  (`sourceHoldsTheSourceMint`, `destinationHoldsTheDestinationMint`);
- both token accounts to belong to the trader (`sellsTheTradersOwnTokens`, `proceedsGoToTheTrader`),
  since a route's step can pay any account of the output mint;
- the price to be above zero (`oraclePriceIsPositive`);
- the route's `platformFeeBps` to be at most `MAX_PLATFORM_FEE_BPS`, a constant that is 0
  (`platformFeeWithinCap`), before Jupiter is called, since whoever builds the run picks the fee
  account and rate;
- after the swap, exactly `inAmount` to have left the source (`soldTheRouteInput`), since Jupiter
  doesn't require its steps to move the source account it is given;
- the destination to have received at least that amount's value at the Pyth price, less
  `toleranceBps` basis points, or hundredths of a percent (`fillBeatTheOracle`).

It does not guard against:

- **Venues' own fee accounts.** The cap covers the route's platform fee, not fees a venue takes
  inside the route. The fill check bounds those to `toleranceBps`, so set it to what you would
  accept losing to the builder, not only to the market.
- **Spending the trader's other token accounts.** The trader signs `route`, and Jupiter passes that
  authority to every step.

## Template

::: code-group

<<< ../../../clients/js/examples/protocols/jupiter-oracle-checked-swap.ts [TypeScript · Template]

<<< ../../../clients/rust/examples/protocol_templates.rs#jupiter-oracle-swap [Rust · Template]

<<< ../../../clients/js/examples/protocols/run/jupiter-oracle-swap.ts [TypeScript · Run]

<<< ../../../clients/rust/examples/protocol_templates_run.rs#jupiter-oracle-swap [Rust · Run]

:::

The feed must price the token you sell in the token you buy, for example SOL/USD when selling SOL
for USDC. Pyth publishes a price as `price × 10^exponent` per whole token. The template reads the
exponent and both mints' decimals on chain, and values what was sold, in the destination token's
smallest units, as `sold × price × 10^(destinationDecimals + exponent − sourceDecimals)`.

## Run it

Jupiter's `route` starts its account list with the token program, the signer, and the signer's
source and destination token accounts. The template passes those four itself; the rest of the
route's accounts arrive as the `routeAccounts` [account group](/guide/account-groups).

The Run tabs pass the eight declared accounts, `jupiter`, `tokenProgram`, `priceUpdate`, `trader`,
`sourceAta`, `destinationAta`, `sourceMint` and `destinationMint`, then the inputs `feedId`,
`routePlan`, `inAmount`, `quotedOutAmount`, `slippageBps`, `platformFeeBps` and `toleranceBps`,
then the group. `feedId` is the Pyth feed's 32-byte id: SOL/USD's is `ef0d8b6f…c280b56d`.
`splitJupiterRoute` (TypeScript) and `RouteQuote::split` (Rust) split the Swap API's `route` data
into `routePlan` and the four numbers after it.

::: tip Requesting the route
Ask Jupiter's Swap API for `useSharedAccounts: false`. The template always sends Jupiter's `route`
instruction. The API's default, `shared_accounts_route`, is a different instruction whose accounts
are in a different order.
:::

## What has been tested

- **Against the real programs.** `tests/protocols/tests/oracle_checked_swap.rs` sells 1 SOL for
  USDC through Jupiter and a Meteora pool, valued at Pyth's SOL/USD price, in place of `route` in
  the transaction Jupiter's API built. With `toleranceBps` 100 it fills exactly as that transaction
  does alone, for about 9,900 more compute units (Solana's measure of execution cost) and 150 more
  bytes. Its floor matches the formula above to the last unit. Jupiter's `route` on its own takes
  exactly `in_amount` through a split route and with a platform fee, as `soldTheRouteInput` needs.
- **Failures.** Each of these fails, and the whole transaction reverts: an oracle 5% above the
  market (`fillBeatTheOracle`, after the swap); spare token accounts of the trader's at `sourceAta`
  and `destinationAta` while the route moves others (`soldTheRouteInput`); another wallet's source
  or an attacker's destination (`sellsTheTradersOwnTokens`, `proceedsGoToTheTrader`, before Jupiter
  is called); USDC/USD's price account passed as SOL/USD's (`priceIsTheExpectedFeed`); a route that
  charges a platform fee (`platformFeeWithinCap`, before Jupiter is called).
- A test reads the template and checks that it calls `route` in `route`'s account order, values
  what left the source at the Pyth price with both mints' decimals, and orders the feed pin, owner
  checks and `soldTheRouteInput` as above (`clients/js/src/protocol-semantics.test.ts`).
- An opt-in test checks the Pyth offsets against devnet accounts.
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares (`clients/rust/tests/protocol_templates.rs`).

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
