# Liquidate with a minimum payout

<p class="protocol-line">Kamino</p>

**Status:** Compiles and passes the verifier; not yet run against Kamino.

## What it does

Liquidates a Kamino loan and reverts unless you receive at least a set amount of collateral.

A Kamino liquidation is only priced correctly if the reserve and the obligation (the borrower's
loan account) are refreshed in the same transaction, which happens after you sign. Kamino's
liquidation instruction accepts a minimum for the liquidity it pays out,
`min_acceptable_received_liquidity_amount`, but nothing limits what you receive across the whole
operation.

The template refreshes the reserve and the obligation, records the balance of your collateral
token account, and liquidates. It then requires that balance to have grown by at least
`minimumBounty`. If it grew by less, the whole run reverts, so you never complete a liquidation
that paid less than you required.

## Template

::: code-group

<<< ../../../clients/js/examples/protocols/kamino-liquidate-with-proof.ts [TypeScript · Template]

<<< ../../../clients/rust/examples/protocol_templates.rs#kamino-liquidate [Rust · Template]

<<< ../../../clients/js/examples/protocols/run/kamino-liquidate.ts [TypeScript · Run]

<<< ../../../clients/rust/examples/protocol_templates_run.rs#kamino-liquidate [Rust · Run]

:::

If another liquidator gets there first, this run reverts. To skip the liquidation instead, read
the obligation's borrowed value and the value at which it becomes unhealthy, and add a `when`
condition to the liquidation. These examples don't include those offsets; take them from the
current Kamino Lend (klend) IDL, the program's published interface description.

## Run it

The Run tabs pass the template's 14 accounts in the order it declares them, and three inputs:
`liquidityAmount`, `minAcceptableReceived` and `minimumBounty`. There is no account group.

## What has been tested

- The template compiles and passes Ballista's verifier.
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares (`clients/rust/tests/protocol_templates.rs`).
- No test calls Kamino.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
