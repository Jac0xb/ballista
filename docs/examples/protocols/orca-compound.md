# Compound the fees you collected

`increase_liquidity` takes caps — `token_max_a` and `token_max_b` — not amounts. Set them too low
and the call fails. Set them too high and it tops up from the wallet without being asked, which
is how a compounder quietly becomes a depositor.

The right caps are the fees the position had actually earned when the block ran. They are read
before collecting, because collecting zeroes them.

::: code-group

<<< ../../../clients/js/examples/protocols/orca-compound-fees.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#plain [Run · Rust]

:::

The `when` guard is the other half. A scheduled compounder that finds nothing to compound should
land a no-op rather than revert and burn the fee.

[All live-protocol examples](/examples/protocols/) · [reading offsets](/examples/protocols/#reading-offsets-from-an-account)
