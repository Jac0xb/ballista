# Repay what the swap produced

Deleveraging means selling collateral and repaying with the proceeds. Both halves are unknown at
signing: what the swap returns, and what the debt has grown to. Interest accrues per slot, so a
figure quoted to the client is stale on arrival, and repaying more than is owed is rejected.

The template swaps, measures what landed, and repays the smaller of that and the wallet's
balance. `refresh_reserve` runs first because a repayment is priced against a refreshed reserve.

::: code-group

<<< ../../../clients/js/examples/protocols/kamino-repay-swap-output.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#group [Run · Rust]

:::

The `min` is the point. The swap output is the intent; the balance is the truth; the smaller of
the two is what can actually be repaid.

[All live-protocol examples](/examples/protocols/) · [reading offsets](/examples/protocols/#reading-offsets-from-an-account)
