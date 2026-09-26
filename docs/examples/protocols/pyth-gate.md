# Gate on a fresh price

Inside a program this is `get_price_no_older_than`. A transaction can read the feed while it is
being built, but it acts on the price at execution, and the gap between the two is the whole
risk: slots pass, a publisher stalls, the market moves.

Three things are checked, not one. Staleness, because a price from four minutes ago is not a
price. The confidence interval, because a wide one means the publishers disagree and there is
effectively no price at all. And the band itself.

::: code-group

<<< ../../../clients/js/examples/protocols/pyth-fresh-price-gate.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#plain [Run · Rust]

:::

The first requirement is not about the price. It pins the account's verification level, which is
what fixes the offsets the other three reads depend on.

[All live-protocol examples](/examples/protocols/) · [reading offsets](/examples/protocols/#reading-offsets-from-an-account)
