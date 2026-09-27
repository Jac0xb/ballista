# Settle PnL, then withdraw

This template settles a Drift user's profit and loss (PnL) on a perpetual market, then withdraws a
set amount to the user's token account.

`settle_pnl` is permissionless: anyone can call it for any user and market. That makes it easy to
schedule, and easy to pair with a withdrawal that depends on it. The catch is that a settle with
nothing to settle wastes a transaction, and it takes anything sent with it down too.

The template records the token account's balance, calls `settle_pnl`, and withdraws
`minimumSettled` from Drift into the token account. The withdrawal is reduce-only, so it can draw
down a deposit but never opens a borrow. The template then requires the balance to have grown by at
least `minimumSettled`. If it hasn't, the whole run reverts, the settle included.

::: code-group

<<< ../../../clients/js/examples/protocols/drift-settle-when-profitable.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#plain [Run · Rust]

:::

The final check confirms that the withdrawal arrived in full, but it can't tell settled PnL apart
from money deposited earlier. To tell them apart, or to skip the withdrawal instead of reverting
when there is nothing to settle, read the user's positions from Drift's `User` account and add a
`when` condition. That needs the byte layout of the `User` account, which these examples don't
include.

The Rust tab builds the run for [act only on a fresh price](/examples/protocols/pyth-gate). Build
this template's run the same way, with its own accounts and inputs in the order the template
declares them. This template has no account group, so leave out the `.groups(...)` call and the
extra accounts at the end.

Not yet run against Drift: the template compiles and passes Ballista's verifier, but no test calls
Drift.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
