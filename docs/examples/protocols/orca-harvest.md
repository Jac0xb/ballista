# Harvest only the positions that earned

This template collects fees from up to 12 Orca positions in one transaction, skipping the ones
that have not earned enough.

A liquidity provider may hold dozens of positions. Some have earned fees since the last harvest
and some haven't, and which is which depends on trades that happen after the transaction is
signed.

Sending one `collect_fees` per position fails the whole transaction at the first position Orca
refuses. Filtering the list before signing doesn't solve it: a position that looked empty may have
earned by the time the transaction lands, and the reverse.

For each position, the template reads `fee_owed_a` from the position account during the run and
calls `collect_fees` only if it is above `dustFloor`.

::: code-group

<<< ../../../clients/js/examples/protocols/orca-harvest-many-positions.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#rows [Run · Rust]

:::

The positions form a batch. In the run, each position is one row of two accounts, the position and
its position token account, and the template repeats its steps once per row. You choose which
positions to include; the template decides during the run which of them to collect from.

The fee offset comes from Orca's `Position` account; see
[reading offsets](/examples/protocols/#reading-offsets-from-an-account).

Not yet run against Orca: the template compiles and passes Ballista's verifier, and an opt-in test
checks its offsets against devnet accounts, but no test calls Orca.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
