# Batch execution

This page shows how a template repeats its steps over a list of rows, such as paying 30 recipients
in one run, and the limits that apply.

A template can declare one batch: a row of accounts, and optionally a row of input values, that the
caller repeats once per item. The caller passes the rows after the template's fixed accounts (the
ones declared in `accounts`), and the program works out the number of rows from how many accounts
follow, not counting members of [account groups](./account-groups). Each row's input values travel
in the run data, so every row can carry its own amount or flag. A template can also require a
minimum number of rows, so that a run with no rows fails instead of succeeding without doing
anything.

## Thirty-recipient payroll

This template sends the same number of [lamports](/reference/glossary#lamports) from a treasury,
which must sign, to each of 1 to 30 recipients. `step.forEach` runs its steps once per row, and
`account.iteration('recipient')` refers to the current row's account. The same template appears on
[payment patterns](/examples/payments#bounded-sol-payroll).

::: code-group

<<< @/../clients/js/examples/docs/bounded-sol-payroll.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/bounded-sol-payroll.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#bounded-sol-payroll [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#bounded-sol-payroll [Rust · Run]

:::

## A different amount per row

`batch.rowInputs` declares inputs that every row carries. Inside `forEach`,
`expression.rowInput(name)` reads the current row's value. At run time the caller passes one set of
values per row, in the same order as the rows.

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

In Rust, values live in registers (numbered slots the builder hands back), and the first argument to
`for_each` lists the registers to carry as a bit mask: `1 << total` carries the register that holds
the total, and `0` carries nothing. Finalization, the one-time check before a template is locked on
chain, confirms that every carried value is set before the loop and keeps its type through the loop
body. A carried `bytes` value must also keep its maximum length.

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

A row holds 1 to 8 accounts. The accounts passed for rows must make up a whole number of rows, and
the row count must lie between `minIterations` and `maxIterations`. A template can have up to eight
loops, run one after another. There are no nested loops, no backward jumps, and no `while` loops
that run until a condition changes. Steps outside a loop can run before it and after it.

::: warning Count CPIs, not just rows
A template can make at most 64 CPIs, counted for the worst case: the calls outside loops, plus
each loop's calls times its maximum (`maxIterations` for `forEach`, `max` for `repeat`). Calls with
a `when` condition count too. A 30-row
loop with two calls counts as 60; a third call in the loop body would make 90, and finalization
would reject the template. The stride-two example above makes two calls per row, so its 8 rows count
as 16.
:::
