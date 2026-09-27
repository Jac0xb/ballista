# Act only on a fresh price

This template reads a Pyth price during the transaction and runs a Jupiter swap only if the price
passes your checks.

Inside a Solana program you would call Pyth's `get_price_no_older_than`. A transaction can't: it
can read the price while it is being built, but it executes later, against whatever the price is
then. In between, a publisher can stall and the market can move.

The template runs the swap only if all of these hold:

- the price account's verification level is `Full`, which fixes where the other fields sit (see
  [reading offsets](/examples/protocols/#reading-offsets-from-an-account));
- the price was published at most `maximumAge` seconds ago;
- the confidence interval is at most `maximumConfidence` (a wide interval means the publishers
  disagree);
- the price is between `floorPrice` and `ceilingPrice`.

`floorPrice`, `ceilingPrice` and `maximumConfidence` are in the feed's raw units, before its
exponent is applied.

The swap is Jupiter's `route` instruction, which starts its account list with the token program and
the signer. The template passes those two itself; the rest of the route's accounts, including its
token accounts, arrive as an [account group](/guide/account-groups). `actionData` is Jupiter's
instruction data without its eight-byte discriminator; the template adds the `route` discriminator
itself.

::: tip Requesting the route
Ask Jupiter's Swap API for `useSharedAccounts: false`. The template always sends Jupiter's `route`
instruction. The API's default, `shared_accounts_route`, is a different instruction whose accounts
are in a different order.
:::

::: code-group

<<< ../../../clients/js/examples/protocols/pyth-fresh-price-gate.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#plain [Run · Rust]

:::

The Rust tab builds this template's run: the declared accounts and inputs in order, then the
route's accounts from the third one on, as the account group.

Not yet run against Jupiter: the template compiles and passes Ballista's verifier, a test checks
that it calls `route` with its accounts in `route`'s order, and an opt-in test checks the Pyth
offsets against devnet accounts. No test calls Jupiter.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
