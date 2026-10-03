# Act only on a fresh price

<p class="protocol-line">Pyth · Jupiter</p>

**Status:** Tested locally in [LiteSVM](https://github.com/LiteSVM/litesvm) against Jupiter, Meteora
and Pyth programs and accounts copied from mainnet; not yet run on devnet or mainnet.

**Cost:** Ballista's own work took 6,978 of the tested transaction's 80,062
[compute units](/reference/glossary#compute-units); the protocols took the rest. Ballista charges no
fee; see [what it costs](/guide/why-ballista#cost).

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

It does not guard against:

- **The swap itself.** The route's token accounts arrive in the group, so nothing checks what the
  swap paid or which account it paid. Only Jupiter's `slippageBps` bounds the fill, against the
  route's own quote. [Swap checked against an oracle](/examples/protocols/jupiter-oracle-swap)
  checks both.
- **The run's builder.** The feed, exponent, age, confidence and band are run inputs. The gate
  protects whoever builds the run from the price moving before the run lands, not the signer from
  that builder.
- **The choice of price within `maximumAge`.** Whoever posts the price update can pick any Pyth
  price from that window.
- **Spending the actor's other token accounts.** The actor signs `route`, and Jupiter passes that
  authority to every step.

## Template

::: code-group

<<< @/../clients/js/examples/protocols/pyth-fresh-price-gate.ts#template [TypeScript · Template]

<<< @/../clients/rust/examples/protocol_templates.rs#pyth-gate [Rust · Template]

<<< @/../clients/js/examples/protocols/run/pyth-gate.ts [TypeScript · Run]

<<< @/../clients/rust/examples/protocol_templates_run.rs#pyth-gate [Rust · Run]

:::

The Rust tabs' `program`, `anchor` and account flags are
[shared helpers](/examples/protocols/#rust-helpers).

## Run it

The swap is Jupiter's `route` instruction, which starts its account list with the token program and
the signer. The template passes those two itself; the rest of the route's accounts, including its
token accounts, arrive as the `actionAccounts` [account group](/guide/account-groups).

The Run tabs pass the four declared accounts, `priceUpdate`, `actionProgram`, `tokenProgram` and
`actor`, then the inputs `feedId`, `exponent`, `maximumAge`, `maximumConfidence`, `floorPrice`,
`ceilingPrice`, `routePlan`, `inAmount`, `quotedOutAmount`, `slippageBps` and `platformFeeBps`,
then the group. `exponent` is an `i64` input, though Pyth stores it as an `i32`.

`priceUpdate` is a fully verified `PriceUpdateV2` for the feed: the account Pyth's push oracle keeps
updated for it, or an update you post with Pyth's receiver earlier in the transaction. A partially
verified update fails the first check. The tests pass SOL/USD's push-oracle account,
`7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE`, whose feed id is
`ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d`.

[Getting a Jupiter route](/examples/protocols/#jupiter-routes) says how to request the route from
Jupiter's Swap API and what to keep from its response.

## What has been tested

- **In LiteSVM.** `tests/protocols/tests/pyth_fresh_price_gate.rs` sells 1 SOL for
  USDC through Jupiter and a Meteora pool, gated on Pyth's SOL/USD price, with the run in place of
  `route` in the transaction Jupiter's API built. With a fresh price in band, it fills exactly as
  that transaction does alone, for 180 more bytes. A price exactly `maximumAge` old passes, and so
  does a band of exactly the price.
- **Failures.** A price one second past `maximumAge` fails at `priceIsFresh`, and one raw unit past
  either end of the band at `priceAboveFloor` or `priceBelowCeiling`. An in-band price with another
  feed's id fails at `priceIsTheExpectedFeed`, and the SOL/USD account with its exponent moved from
  −8 to −7 at `priceExponentIsExpected`. No test fails the verification-level or confidence
  check. A route that charges a platform fee fails at `platformFeeWithinCap`, before Jupiter is
  called.
- An opt-in test checks the Pyth offsets against devnet accounts.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
