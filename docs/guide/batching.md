# Batch execution

A template can declare one batch: a row of 1 to 8 accounts, plus optional row inputs, that the
caller repeats once per item, such as one recipient per row. `step.forEach` runs its steps once per
row.

The caller passes the rows after the fixed accounts and before any
[account group](/guide/accounts-and-cpis#account-groups) members, and the row inputs in the run
data, one set per row. The program counts the rows from the accounts left over: they must divide
evenly into rows, and the count must fall between the batch's minimum and maximum (at most 60), or
the run fails.

## Variable payroll

This template pays each of 1 to 30 recipients its own number of
[lamports](/reference/glossary#lamports) from a treasury, which must sign. `step.forEach` runs its
steps once per row, and `account.iteration('recipient')` refers to the current row's account.
`batch.rowInputs` declares inputs that every row carries, and `expression.rowInput('amount')` reads
the current row's value. At run time the caller passes one set of values per row, in the same order
as the rows.

::: code-group

<<< @/../clients/js/examples/docs/row-amounts.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/row-amounts.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#row-amounts [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#row-amounts [Rust · Run]

:::

These limits apply to row inputs:

- Fixed inputs and row inputs together are limited to 32 declarations, with at most 8 per row.
- A run can carry at most 256 values: the fixed inputs plus the row inputs times the template's
  maximum number of rows.
- The whole run data is limited to 1,024 bytes, which in practice bounds `bytes` row inputs.

If the row values do not match the rows, the run fails with `InvalidRunInputs`, and the error
reports the index of the first missing or malformed value, counting fixed values first.

Rows supply accounts and values, not new calls: the loop body's [CPIs](/reference/glossary#cpi) are
the same for every row. An `invoke` inside the loop that forwards an account group forwards the
same group on every row.

## Carry a total across rows

Values set inside the loop body are discarded after each row, unless the loop carries them. A
carried variable is defined with `step.let` before the loop, listed in the loop's `carry` option,
updated inside the loop with `step.assign`, and still readable after the loop ends. That lets a
template enforce a limit over the whole batch, such as a total budget.

::: code-group

<<< @/../clients/js/examples/docs/budgeted-payroll.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/budgeted-payroll.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#budgeted-payroll [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#budgeted-payroll [Rust · Run]

:::

The `require` carries the label `withinBudget`, so a run that goes over budget fails with an error
that names that step; the TypeScript Run tab shows how to read it with `explainRunError`.

In Rust, the loop lists its carried variables with `.carry("total")`. Finalization, the one-time
check before a template is locked on chain, confirms that every carried value is set before the loop
and keeps its type through the loop body. A carried `bytes` value must also keep its maximum length.

## Stride-two rows

A row can hold more than one account. The number of accounts in each row is its stride; here each
row has two, a recipient's wallet and a token account. For every row, the template checks that the
token account is the wallet's [ATA](/reference/glossary#ata), creates the account if it does not
exist, and transfers tokens to it. The same template appears on
[token-account patterns](/examples/token-accounts#assert-create-then-transfer).

::: code-group

<<< @/../clients/js/examples/docs/assert-create-then-transfer.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/assert-create-then-transfer.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#assert-create-then-transfer [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#assert-create-then-transfer [Rust · Run]

:::

The caller passes each row's accounts together, in the order `row` declares them: wallet, ATA,
wallet, ATA, and so on.

## Rules and limits

- **Checked at finalization.** Up to eight loops (`forEach` and
  [`repeat`](/guide/loops#crank-once-per-waiting-entry)), one after another and never nested, else
  `InvalidLoop` (6129). At most 64 CPIs in the worst case: each loop's calls times its maximum, plus
  the calls outside loops. A 30-row loop with two calls counts 60.
- **Capped by Solana.** The [instruction trace](/reference/limits#instruction-trace) counts the run
  and every call the called programs make, and often runs out before 64. Size loops to it.
