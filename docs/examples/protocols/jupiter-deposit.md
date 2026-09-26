# Deposit exactly what a swap produced

Jupiter's `route` carries the amount in, the *quoted* amount out, `slippageBps` and
`platformFeeBps`. What actually comes out is reported as an Anchor `SwapEvent` emitted through a
self-CPI. That is an event, not return data, so a caller cannot read it back with
`get_return_data`. The destination token account is the only reliable source, and only after the
route has run.

Kamino's `deposit_reserve_liquidity_and_obligation_collateral_v2` wants that number as its
`liquidity_amount`. A transaction has to write it before the swap has happened. Quote it high and
the deposit fails; quote it low and the difference sits in the ATA until someone notices.

::: code-group

<<< ../../../clients/js/examples/protocols/jupiter-deposit-exact-output.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#group [Run · Rust]

:::

Route account lists vary in length, so they arrive as an [account group](/guide/account-groups)
rather than declared slots. One template then serves every route the aggregator returns. Group
members are forwarded with the transaction's own writable flag and never sign.

[All live-protocol examples](/examples/protocols/) · [reading offsets](/examples/protocols/#reading-offsets-from-an-account)
