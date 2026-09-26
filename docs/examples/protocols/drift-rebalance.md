# Rebalance between venues

Drift's `deposit(market_index, amount, reduce_only)` needs a number the marginfi withdrawal
produces moments earlier. Guess high and the deposit fails, taking the withdrawal down with it.
Guess low and the remainder sits in the wallet, out of the market, until someone notices.

Rate-shopping bots run this loop constantly. Today it is two transactions with an unhedged gap
between them, or a custom program.

::: code-group

<<< ../../../clients/js/examples/protocols/drift-rebalance-exact.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#plain [Run · Rust]

:::

`reduce_only: false` on the deposit is deliberate: this is moving a position, not closing one.

[All live-protocol examples](/examples/protocols/) · [reading offsets](/examples/protocols/#reading-offsets-from-an-account)
