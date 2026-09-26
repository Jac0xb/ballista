# Harvest only the positions that earned

A liquidity manager holds dozens of positions. Most have earned something since the last harvest
and some have not, and which is which depends on trades that happen after the transaction is
signed.

One `collect_fees` per position reverts the whole batch on the first position the protocol
refuses. Filtering beforehand races the block: a position that looked empty when the list was
built may have earned by the time it lands, and the reverse.

::: code-group

<<< ../../../clients/js/examples/protocols/orca-harvest-many-positions.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#rows [Run · Rust]

:::

The row count is fixed by the account list. Whether each row acts is decided during execution
from that row's own `fee_owed_a`.

[All live-protocol examples](/examples/protocols/) · [reading offsets](/examples/protocols/#reading-offsets-from-an-account)
