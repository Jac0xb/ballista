# Settle PnL, then withdraw

<p class="protocol-line">Drift</p>

**Status:** Compiles and passes the verifier; not yet run against Drift.

## What it does

Settles a Drift user's profit and loss (PnL) on a perpetual market, then withdraws a set amount to
the user's token account.

`settle_pnl` is permissionless: anyone can call it for any user and market. That makes it easy to
schedule, and easy to pair with a withdrawal that depends on it. The catch is that a settle with
nothing to settle wastes a transaction, and it takes anything sent with it down too.

The template records the token account's balance, calls `settle_pnl`, and withdraws
`minimumSettled` from Drift into the token account. The withdrawal is reduce-only, so it can draw
down a deposit but never opens a borrow. The template then requires the balance to have grown by at
least `minimumSettled`. If it hasn't, the whole run reverts, the settle included.

## Template

::: code-group

<<< ../../../clients/js/examples/protocols/drift-settle-when-profitable.ts [TypeScript · Template]

<<< ../../../clients/rust/examples/protocol_templates.rs#drift-settle [Rust · Template]

<<< ../../../clients/js/examples/protocols/run/drift-settle.ts [TypeScript · Run]

<<< ../../../clients/rust/examples/protocol_templates_run.rs#drift-settle [Rust · Run]

:::

The final check confirms that the withdrawal arrived in full, but it can't tell settled PnL apart
from money deposited earlier. To tell them apart, or to skip the withdrawal instead of reverting
when there is nothing to settle, read the user's positions from Drift's `User` account and add a
`when` condition. That needs the byte layout of the `User` account, which these examples don't
include.

## Run it

The Run tabs pass the template's 11 accounts in the order it declares them, and one input,
`minimumSettled`. There is no account group.

## What has been tested

- The template compiles and passes Ballista's verifier.
- A test checks that the withdrawal sets `reduce_only`, so it can never open a borrow
  (`clients/js/src/protocol-semantics.test.ts`).
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares (`clients/rust/tests/protocol_templates.rs`).
- No test calls Drift.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
