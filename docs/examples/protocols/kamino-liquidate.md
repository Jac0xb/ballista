# Liquidate and prove it paid

A liquidation is only priced correctly against a reserve and obligation refreshed in the same
transaction — refreshes that happen after signing. The protocol bounds the liquidity leg with
`min_acceptable_received_liquidity_amount`. It does not bound what the liquidator nets across the
whole operation.

So this refreshes, liquidates, and then requires the liquidator's collateral account to have
grown by at least the bounty it was chasing. Anything less and the run reverts: no
half-executed liquidation, no paying gas to improve someone else's position.

::: code-group

<<< ../../../clients/js/examples/protocols/kamino-liquidate-with-proof.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#plain [Run · Rust]

:::

Gating on health itself, so a losing race lands a no-op instead of reverting, needs the
obligation's borrowed and unhealthy-borrow offsets. Those are not derived here; take them from
the current klend IDL and attach a `when`.

[All live-protocol examples](/examples/protocols/) · [reading offsets](/examples/protocols/#reading-offsets-from-an-account)
