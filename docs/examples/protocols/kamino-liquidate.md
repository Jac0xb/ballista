# Liquidate with a minimum payout

This template liquidates a Kamino loan and reverts unless you receive at least a set amount of
collateral.

A Kamino liquidation is only priced correctly if the reserve and the obligation (the borrower's
loan account) are refreshed in the same transaction, which happens after you sign. Kamino's
liquidation instruction accepts a minimum for the liquidity it pays out,
`min_acceptable_received_liquidity_amount`, but nothing limits what you receive across the whole
operation.

The template refreshes the reserve and the obligation, records the balance of your collateral
token account, and liquidates. It then requires that balance to have grown by at least
`minimumBounty`. If it grew by less, the whole run reverts, so you never complete a liquidation
that paid less than you required.

::: code-group

<<< ../../../clients/js/examples/protocols/kamino-liquidate-with-proof.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#plain [Run · Rust]

:::

If another liquidator gets there first, this run reverts. To skip the liquidation instead, read
the obligation's borrowed value and the value at which it becomes unhealthy, and add a `when`
condition to the liquidation. These examples don't include those offsets; take them from the
current Kamino Lend (klend) IDL, the program's published interface description.

The Rust tab builds the run for [act only on a fresh price](/examples/protocols/pyth-gate). Build
this template's run the same way, with its own accounts and inputs in the order the template
declares them. This template has no account group, so leave out the `.groups(...)` call and the
extra accounts at the end.

Not yet run against Kamino: the template compiles and passes Ballista's verifier, but no test calls
Kamino.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
