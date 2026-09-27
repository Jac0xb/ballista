# Loops that decide per row

This page shows loops in which each row decides what to do from state read during the run: pay
creditors in priority order, collect only the token accounts that hold a balance, process only
the queue entries that are due, and split a balance by weight.

A template declares its loop as a **batch**, and each item the loop runs over is a **row**. In
`batch`, `row` names the accounts in each row, `rowInputs` declares values the caller supplies for
each row, and `maxIterations` caps the number of rows. `step.forEach` holds the steps that run once
per row. Inside it, `account.iteration('name')` is the current row's account and
`expression.rowInput('name')` is the current row's value. [Batch execution](/guide/batching)
covers the rules and limits.

The accounts the caller passes fix how many rows run. What each row does can still depend on what
earlier rows spent, or on the row's own account. `when` skips a single call when its condition is
false. In a run, the row accounts follow the fixed accounts, and the row inputs follow the fixed
inputs. The examples that call another protocol use marked stand-ins (the System program and its
Transfer data) so they compile and run as written.

## Waterfall until the money runs out

Pay creditors from a treasury in priority order, the first row first. Each payment is capped by
what is left, and what is left is known only when the transaction executes.

::: code-group

<<< @/../clients/js/examples/docs/waterfall-until-the-money-runs-out.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/waterfall-until-the-money-runs-out.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#waterfall-until-the-money-runs-out [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#waterfall-until-the-money-runs-out [Rust · Run]

:::

Each row is one creditor account, marked writable (the transaction may change it) because it
receives [lamports](/reference/glossary#lamports). `remaining` starts as the treasury's balance
minus a reserve. Each row pays the smaller of `remaining` and that creditor's `owed` amount, then
subtracts the payment.

`carry: ['remaining']` is what passes the balance from one row to the next. A variable created
before the loop and listed in `carry` can be updated with `step.assign` inside the loop, so row
four sees what rows one to three paid. Variables created inside the loop, such as `pay`, start
fresh on every row. Once the money runs out, `pay` is zero and `when` skips the transfer, so the
later creditors get nothing and the transaction still succeeds. In Rust, the first argument to
`for_each` is the carry: a bit mask with one bit per carried register.

## Consolidate only the funded accounts

Move the whole balance of each token account in the batch into one vault, and skip the accounts
that are empty.

::: code-group

<<< @/../clients/js/examples/docs/consolidate-only-the-funded-accounts.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/consolidate-only-the-funded-accounts.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#consolidate-only-the-funded-accounts [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#consolidate-only-the-funded-accounts [Rust · Run]

:::

Each row reads its own account's balance (the `u64` at offset 64 of an SPL Token account) and
transfers all of it, and `when` skips the transfer when the balance is zero. Sending one plain
Transfer instruction per account needs every amount before signing, and the whole transaction
fails at the first account that turns out to be empty.

## Crank only the ripe entries

A crank is a transaction that processes the items waiting in a protocol's queue. Keepers (bots
that do routine maintenance for a protocol) send them. This template settles only the entries
whose deadline has passed.

::: code-group

<<< @/../clients/js/examples/docs/crank-only-the-ripe-entries.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/crank-only-the-ripe-entries.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#crank-only-the-ripe-entries [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#crank-only-the-ripe-entries [Rust · Run]

:::

Each row is one queue entry. The template reads the entry's deadline, a Unix timestamp at
`DEADLINE_OFFSET`, and compares it with the network's clock when the transaction executes. For
each entry that is due, `step.invoke` makes a [CPI](/reference/glossary#cpi) into the protocol.
The keeper can pass the whole queue and let the run decide. Filtering the queue before sending
would compare the deadlines with a time that has already passed when the transaction runs.

## Distribute a runtime pot pro rata

Split a vault's balance above a reserve (the pot) among holders, each in proportion to a weight
the caller passes.

::: code-group

<<< @/../clients/js/examples/docs/distribute-a-runtime-pot-pro-rata.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/distribute-a-runtime-pot-pro-rata.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#distribute-a-runtime-pot-pro-rata [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#distribute-a-runtime-pot-pro-rata [Rust · Run]

:::

`weightBps` is each holder's share in basis points (hundredths of a percent, so 10,000 is 100%).
The caller chooses the weights, and the template reads the pot when the transaction executes.
Computing the shares off chain would divide a balance that may have changed by then. Each share
is rounded down, and whatever the rounding leaves over stays in the vault.
