# Swap checked against an oracle

This template swaps through Jupiter and reverts unless the swap paid at least what a Pyth price
says the sold tokens were worth, less a tolerance.

Jupiter's `slippageBps` limits how far the swap can fall short of the quote Jupiter produced. It
doesn't help when the quote itself is the problem: a bad quote, a manipulated pool inside the
route, or a route built by someone other than the signer. An independent price catches those
cases.

The template:

1. requires the Pyth price account's verification level to be `Full`, because the level decides
   where every other field sits (see [reading offsets](/examples/protocols/#reading-offsets-from-an-account));
2. requires the price to be at most 60 seconds old;
3. requires the feed's exponent to equal `priceExponent`;
4. requires the price to be above zero;
5. records the source and destination token balances, then runs the Jupiter route;
6. values what actually left the source account at the Pyth price, less `toleranceBps` (basis
   points, or hundredths of a percent);
7. requires the destination to have received at least that value.

The feed must price the token you sell in the token you buy, for example SOL/USD when selling SOL
for USDC. Pyth publishes a price as `price × 10^exponent` per whole token, so you compute
`scaleDivisor` once for the pair: `10^(sourceDecimals − destinationDecimals − exponent)`. If the
feed's exponent ever differs from the one you computed for, step 3 fails the run instead of
mispricing the swap.

Jupiter's `route` starts its account list with the token program, the signer, and the signer's
source and destination token accounts. The template passes those four itself, so the balances it
measures are the ones Jupiter moves, and the rest of the route's accounts arrive as an
[account group](/guide/account-groups). `routeArgs` is Jupiter's instruction data without its
eight-byte discriminator; the template adds the `route` discriminator itself.

::: tip Requesting the route
Ask Jupiter's Swap API for `useSharedAccounts: false`. The template always sends Jupiter's `route`
instruction. The API's default, `shared_accounts_route`, is a different instruction whose accounts
are in a different order.
:::

::: code-group

<<< ../../../clients/js/examples/protocols/jupiter-oracle-checked-swap.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#group [Run · Rust]

:::

The Rust tab builds the run for [deposit exactly what a swap produced](/examples/protocols/jupiter-deposit).
This template's run has the same form, with the route's remaining accounts passed as an account
group, but uses its own accounts and inputs in the order the template declares them.

Not yet run against Jupiter: the template compiles and passes Ballista's verifier, and a test
checks that it calls `route` with its accounts in `route`'s order and that its minimum depends on
the Pyth price and on what left the source account. No test calls Jupiter.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
