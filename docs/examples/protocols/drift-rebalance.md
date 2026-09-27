# Move funds from marginfi to Drift

This template withdraws a whole position from marginfi and deposits exactly what came out into
Drift, in one transaction.

Drift's `deposit(market_index, amount, reduce_only)` needs the amount up front, but the amount only
exists once the marginfi withdrawal has run. A plain transaction has to guess it. Guess too high
and the deposit fails, taking the withdrawal with it. Guess too low and the rest sits in the wallet
instead of in Drift. Without a template, the choice is two separate transactions with a gap between
them, or a custom program.

The template records the wallet's token balance, withdraws everything from marginfi, and measures
how much arrived. If that is less than `minimumMoved`, the run reverts. Otherwise it deposits
exactly that amount into Drift.

::: code-group

<<< ../../../clients/js/examples/protocols/drift-rebalance-exact.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#plain [Run · Rust]

:::

The deposit sets `reduce_only` to `false` on purpose: the funds are moving into Drift, not closing
a position there. The spot market index is fixed at 0, which is USDC on mainnet; change it for
another asset.

The Rust tab builds the run for [act only on a fresh price](/examples/protocols/pyth-gate). Build
this template's run the same way, with its own accounts and inputs in the order the template
declares them. This template has no account group, so leave out the `.groups(...)` call and the
extra accounts at the end.

Not yet run against marginfi or Drift: the template compiles and passes Ballista's verifier, but no
test calls either program.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
