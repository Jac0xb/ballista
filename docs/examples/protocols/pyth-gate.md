# Act only on a fresh price

<p class="protocol-line">Pyth · Jupiter</p>

**Status:** Compiles and passes the verifier; not yet run against Jupiter.

## What it does

Reads a Pyth price during the transaction and runs a Jupiter swap only if the price passes your
checks.

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

The Run tabs pass the four declared accounts in order, then the inputs `maximumAge`,
`maximumConfidence`, `floorPrice`, `ceilingPrice` and `actionData`, then the group. `actionData` is
Jupiter's instruction data without its eight-byte discriminator; the template adds the `route`
discriminator itself.

::: tip Requesting the route
Ask Jupiter's Swap API for `useSharedAccounts: false`. The template always sends Jupiter's `route`
instruction. The API's default, `shared_accounts_route`, is a different instruction whose accounts
are in a different order.
:::

## What has been tested

- The template compiles and passes Ballista's verifier.
- A test checks that it calls `route` with its accounts in `route`'s order
  (`clients/js/src/protocol-semantics.test.ts`).
- An opt-in test checks the Pyth offsets against devnet accounts.
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares (`clients/rust/tests/protocol_templates.rs`).
- No test calls Jupiter.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
