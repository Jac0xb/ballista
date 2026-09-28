# Swap checked against an oracle

<p class="protocol-line">Jupiter · Pyth</p>

**Status:** Compiles and passes the verifier; not yet run against Jupiter.

## What it does

Swaps through Jupiter and reverts unless the swap paid at least what a Pyth price says the sold
tokens were worth, less a tolerance.

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

## Template

::: code-group

<<< ../../../clients/js/examples/protocols/jupiter-oracle-checked-swap.ts [TypeScript · Template]

<<< ../../../clients/rust/examples/protocol_templates.rs#jupiter-oracle-swap [Rust · Template]

<<< ../../../clients/js/examples/protocols/run/jupiter-oracle-swap.ts [TypeScript · Run]

<<< ../../../clients/rust/examples/protocol_templates_run.rs#jupiter-oracle-swap [Rust · Run]

:::

The feed must price the token you sell in the token you buy, for example SOL/USD when selling SOL
for USDC. Pyth publishes a price as `price × 10^exponent` per whole token, so you compute
`scaleDivisor` once for the pair: `10^(sourceDecimals − destinationDecimals − exponent)`. If the
feed's exponent ever differs from the one you computed for, step 3 fails the run instead of
mispricing the swap.

## Run it

Jupiter's `route` starts its account list with the token program, the signer, and the signer's
source and destination token accounts. The template passes those four itself, so the balances it
measures are the ones Jupiter moves. The rest of the route's accounts arrive as the
`routeAccounts` [account group](/guide/account-groups).

The Run tabs pass the six declared accounts in order, then the inputs `routeArgs`,
`priceExponent`, `scaleDivisor` and `toleranceBps`, then the group. `routeArgs` is Jupiter's
instruction data without its eight-byte discriminator; the template adds the `route` discriminator
itself.

::: tip Requesting the route
Ask Jupiter's Swap API for `useSharedAccounts: false`. The template always sends Jupiter's `route`
instruction. The API's default, `shared_accounts_route`, is a different instruction whose accounts
are in a different order.
:::

## What has been tested

- The template compiles and passes Ballista's verifier.
- A test checks that it calls `route` with its accounts in `route`'s order, that its minimum
  depends on the Pyth price and on what left the source account, and that it pins the feed's
  exponent (`clients/js/src/protocol-semantics.test.ts`).
- An opt-in test checks the Pyth offsets against devnet accounts.
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares (`clients/rust/tests/protocol_templates.rs`).
- No test calls Jupiter.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
