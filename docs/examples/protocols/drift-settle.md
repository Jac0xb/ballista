# Settle, then withdraw what settled

`settle_pnl` is permissionless, which makes it natural to schedule and natural to batch with a
withdrawal that depends on it. The catch is that a settle finding nothing to settle is a wasted
transaction, and batched with anything else it takes that down too.

The settlement is measured by its effect on the user's spot account, and the run reverts before
the dependent withdrawal if nothing moved.

::: code-group

<<< ../../../clients/js/examples/protocols/drift-settle-when-profitable.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#plain [Run · Rust]

:::

Reading Drift's unsettled PnL directly would let the run land as a no-op instead of reverting.
That needs the `User` account's layout, a zero-copy struct whose offsets are not derived here.

[All live-protocol examples](/examples/protocols/) · [reading offsets](/examples/protocols/#reading-offsets-from-an-account)
