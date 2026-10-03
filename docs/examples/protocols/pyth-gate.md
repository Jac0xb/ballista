# Act only on a fresh price

<p class="protocol-line">Pyth · Jupiter</p>

**Status:** Run as real transactions against Jupiter, a Meteora pool and Pyth's SOL/USD price,
copied from mainnet at one slot into LiteSVM, a local Solana runtime. Not yet run on mainnet itself.

## What it does

Reads a Pyth price during the transaction and runs a Jupiter swap only if the price passes your
checks.

Inside a Solana program you would call Pyth's `get_price_no_older_than`. A transaction can't: it
can read the price while it is being built, but it executes later, against whatever the price is
then. In between, a publisher can stall and the market can move.

The template runs the swap only if all of these hold:

- the price account's verification level is `Full`, which fixes where the other fields sit (see
  [reading offsets](/examples/protocols/#reading-offsets-from-an-account));
- the feed id is `feedId`;
- the exponent is `exponent`;
- the price was published at most `maximumAge` seconds ago;
- the confidence interval is at most `maximumConfidence` (a wide interval means the publishers
  disagree);
- the price is between `floorPrice` and `ceilingPrice`, inclusive;
- the route's `platformFeeBps` is at most `MAX_PLATFORM_FEE_BPS`, a constant that is 0
  (`platformFeeWithinCap`).

A feed id is the 32 bytes that name a Pyth feed, such as SOL/USD. Pyth's receiver program owns
every feed's price account, so only the feed id says which feed an account holds.

Pyth publishes a price as `price × 10^exponent`. `floorPrice`, `ceilingPrice` and
`maximumConfidence` are raw integers at `exponent`: at SOL/USD's exponent of −8, $100 is
10,000,000,000. If the feed's exponent changed, each would be off by a power of ten, so the
exponent check fails the run instead.

## Template

::: code-group

<<< ../../../clients/js/examples/protocols/pyth-fresh-price-gate.ts [TypeScript · Template]

<<< ../../../clients/rust/examples/protocol_templates.rs#pyth-gate [Rust · Template]

<<< ../../../clients/js/examples/protocols/run/pyth-gate.ts [TypeScript · Run]

<<< ../../../clients/rust/examples/protocol_templates_run.rs#pyth-gate [Rust · Run]

:::

## Run it

The swap is Jupiter's `route` instruction, which starts its account list with the token program and
the signer. The template passes those two itself; the rest of the route's accounts, including its
token accounts, arrive as the `actionAccounts` [account group](/guide/account-groups).

The Run tabs pass the four declared accounts, `priceUpdate`, `actionProgram`, `tokenProgram` and
`actor`, then the inputs `feedId`, `exponent`, `maximumAge`, `maximumConfidence`, `floorPrice`,
`ceilingPrice`, `routePlan`, `inAmount`, `quotedOutAmount`, `slippageBps` and `platformFeeBps`,
then the group. `exponent` is an `i64` input, though Pyth stores it as an `i32`.

::: tip Requesting the route
Ask Jupiter's Swap API for `useSharedAccounts: false`. The template always sends Jupiter's `route`
instruction. The API's default, `shared_accounts_route`, is a different instruction whose accounts
are in a different order.
:::

## What has been tested

- **Against the real programs.** `tests/protocols/tests/pyth_fresh_price_gate.rs` sells 1 SOL for
  USDC through Jupiter and a Meteora pool, gated on Pyth's SOL/USD price, with the run in place of
  `route` in the transaction Jupiter's API built. With a fresh price in band, it fills exactly as
  that transaction does alone. A price exactly `maximumAge` old passes, and so does a band of
  exactly the price.
- **Failures.** A price one second past `maximumAge` fails at `priceIsFresh`, and one raw unit past
  either end of the band at `priceAboveFloor` or `priceBelowCeiling`. An in-band price with another
  feed's id fails at `priceIsTheExpectedFeed`, and the SOL/USD account with its exponent moved from
  −8 to −7 at `priceExponentIsExpected`. No test fails the verification-level or confidence
  check.
- A test reads the template and checks that it calls `route` with its accounts in `route`'s order,
  and checks the feed and then the exponent right after the verification level
  (`clients/js/src/protocol-semantics.test.ts`).
- An opt-in test checks the Pyth offsets against devnet accounts.
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares (`clients/rust/tests/protocol_templates.rs`).

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
