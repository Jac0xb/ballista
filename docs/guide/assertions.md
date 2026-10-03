# Assertions and snapshots

This page shows how a template checks conditions with `step.require`, and how a snapshot lets it
compare an account before and after a call.

`step.require(condition)` fails the whole transaction unless its condition is true. A condition can
combine account reads, inputs, the clock, checked arithmetic, comparisons, and the boolean
operators `and`, `or`, and `not`. Inputs come from the caller, so a check that an input can turn
off, such as one joined with `or` to an input flag, protects nothing.

## Why snapshots exist

Reading an account is live: `expression.accountField(vault, 'lamports')` reads the balance again
every time it appears. After a [CPI](/reference/glossary#cpi) changes the balance, every read
returns the new value, and the old one is gone.

`step.snapshot(name, value)` reads once and keeps the result for the rest of the run. Compare it
with a live read after the call to check what the call did. `step.let` is the same step, named for
values that aren't a before-and-after; read either with `expression.snapshot` or
`expression.variable`.

- **Scope.** A name is readable only by the steps after it. One defined inside a loop is gone after
  the loop.
- **Fixed.** A name can't be reassigned, except a variable a loop
  [carries](/guide/batching#carry-a-total-across-rows), which `step.assign` updates.
- **Free.** Names cost no accounts or rent and vanish with the transaction.

## Exact lamport delta

This template requires the sender and recipient to be different accounts, records the sender's
balance in [lamports](/reference/glossary#lamports), transfers an amount, then requires that the
balance fell by exactly that amount.

::: code-group

<<< @/../clients/js/examples/docs/exact-lamport-delta.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/exact-lamport-delta.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#exact-lamport-delta [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#exact-lamport-delta [Rust · Run]

:::

The first `require` refuses one account in both slots. A run accepts that and checks each slot on
its own, so a check that adds up changes across accounts could count one account twice. Here an
alias would only fail the exact check, at a less clear step. See
[Aliased accounts](/guide/trust-model#aliased-accounts).

## Token amount delta

The same check works for tokens. An SPL token account stores its balance as a `u64` at byte offset
64. [Exact token debit](/examples/token-accounts#exact-token-debit) is the complete template, in
TypeScript and Rust.

```ts
step.snapshot(
  'sourceBefore',
  expression.accountData(account.fixed('source'), 64, 'u64'),
),
tokenTransfer({ /* ... */ }),
step.require(
  expression.equal(
    expression.accountData(account.fixed('source'), 64, 'u64'),
    expression.subtract(expression.snapshot('sourceBefore'), expression.input('amount')),
  ),
),
```
