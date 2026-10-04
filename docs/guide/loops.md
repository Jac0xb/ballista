# Loops over rows and counts

In these loops each row decides what to do from what it reads during the run: pay creditors in
order, collect only funded token accounts, process only due entries, split by weight. The last
example repeats a call a number of times read during the run.

Row loops run over a [batch](/guide/batching): `step.forEach` runs once per row, with
`account.iteration('name')` as the row's account and `expression.rowInput('name')` as its value.
The caller fixes how many rows run; each row can still act on what earlier rows spent, and `when`
skips one call. Calls to other protocols use marked stand-ins.

## Waterfall until the money runs out

Pay creditors from a treasury in priority order, the first row first. Each payment is capped by
what is left, and what is left is known only when the transaction executes.

::: code-group

<<< @/../clients/js/examples/docs/waterfall-until-the-money-runs-out.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/waterfall-until-the-money-runs-out.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#waterfall-until-the-money-runs-out [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#waterfall-until-the-money-runs-out [Rust · Run]

:::

Each row is one creditor account, writable because it receives
[lamports](/reference/glossary#lamports). `remaining` starts as the treasury's balance minus a
reserve. Each row pays the smaller of `remaining` and that creditor's `owed` amount, then
subtracts the payment. [`carry`](/guide/batching#carry-a-total-across-rows) passes `remaining`
from row to row, so row four sees what rows one to three paid, while `pay` starts fresh on every
row. Once the money runs out, `pay` is zero and `when` skips the transfer, so the later creditors
get nothing and the transaction still succeeds.

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

## Crank once per waiting entry

`step.repeat(count, steps, { max })` is a count loop: it runs its steps `count` times, and it has
no rows. This template reads how many entries wait in a queue when the transaction executes, and
cranks the queue that many times, up to eight. A plain transaction fixes its number of crank
instructions when it is signed.

::: code-group

<<< @/../clients/js/examples/docs/crank-once-per-waiting-entry.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/crank-once-per-waiting-entry.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#crank-once-per-waiting-entry [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#crank-once-per-waiting-entry [Rust · Run]

:::

- The count is a `u64`, read once, when the loop starts. A count of 0 skips the loop.
- `max`, from 1 to 255, is the most times the loop may run. A run whose count is above it fails
  with `LoopCountExceeded` (6022), so this template caps the count with `min`.
- Finalization counts the loop's calls at `max`, here 8 of the 64 a run may make.
  [Rules and limits](/guide/batching#rules-and-limits) covers that count and Solana's instruction
  trace, which can run out first.
- `carry` and `expression.loopIndex()` (the pass number, from 0) work as they do in `forEach`.
  There are no rows, so `account.iteration` and `expression.rowInput` are rejected inside.
