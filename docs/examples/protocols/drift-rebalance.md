# Move funds from marginfi to Drift

<p class="protocol-line">Drift · marginfi</p>

**Status:** Compiles and passes the verifier; not yet run against marginfi or Drift.

## What it does

Withdraws a whole position from marginfi and deposits exactly what came out into Drift, in one
transaction.

Drift's `deposit(market_index, amount, reduce_only)` needs the amount up front, but the amount only
exists once the marginfi withdrawal has run. A plain transaction has to guess it. Guess too high
and the deposit fails, taking the withdrawal with it. Guess too low and the rest sits in the wallet
instead of in Drift.

The template records the wallet's token balance, withdraws everything from marginfi, and measures
how much arrived. If that is less than `minimumMoved`, the run reverts. Otherwise it deposits
exactly that amount into Drift.

## Template

::: code-group

<<< ../../../clients/js/examples/protocols/drift-rebalance-exact.ts [TypeScript · Template]

<<< ../../../clients/rust/examples/protocol_templates.rs#drift-rebalance [Rust · Template]

<<< ../../../clients/js/examples/protocols/run/drift-rebalance.ts [TypeScript · Run]

<<< ../../../clients/rust/examples/protocol_templates_run.rs#drift-rebalance [Rust · Run]

:::

The deposit sets `reduce_only` to `false` on purpose: the funds are moving into Drift, not closing
a position there. The spot market index is fixed at 0, which is USDC on mainnet; change it for
another asset.

## Run it

The Run tabs pass the template's 14 accounts in the order it declares them, and one input,
`minimumMoved`. The three programs are pinned, so the run passes exactly those addresses. There is
no account group.

## What has been tested

- The template compiles and passes Ballista's verifier.
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares (`clients/rust/tests/protocol_templates.rs`).
- No test calls marginfi or Drift.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
