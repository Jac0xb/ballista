# Swap checked against an oracle

`slippageBps` protects the quote Jupiter produced against movement between quote and execution.
It does not protect against a bad quote, a manipulated pool inside the route, or a route
assembled by someone other than the signer — in all three the quote itself is the problem, and
slippage is measured from it.

This keeps a second opinion. It reads Pyth during execution, works out what the input is worth at
that price less a tolerance, runs the route, then measures the destination account. Two
independent sources have to agree before the transaction is allowed to stand.

::: code-group

<<< ../../../clients/js/examples/protocols/jupiter-oracle-checked-swap.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#group [Run · Rust]

:::

The first requirement pins the oracle's verification level, because that is what decides where
every later field sits. See [reading offsets](/examples/protocols/#reading-offsets-from-an-account).

[All live-protocol examples](/examples/protocols/) · [reading offsets](/examples/protocols/#reading-offsets-from-an-account)
