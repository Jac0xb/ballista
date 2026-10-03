# Assertions and snapshots

This page shows how a template checks conditions with `step.require`, and how a snapshot lets it
compare an account before and after a call.

`step.require(condition)` fails the whole transaction unless its condition is true. A condition can
combine account reads, inputs, the clock, checked arithmetic, comparisons, and the boolean
operators `and`, `or`, and `not`. Inputs come from the caller, so a check that an input can turn
off, such as one joined with `or` to an input flag, protects nothing.

## Why snapshots exist

An expression is evaluated again everywhere it appears. A read of an account's balance happens anew
at each place it is used, so the same expression written before and after a
[CPI](/reference/glossary#cpi) gives two different values, and there is no way to keep the first
one. Comparing the balance after a transfer with a second read of that same balance proves
nothing.

`step.snapshot(name, value)` evaluates its expression once, at that point in the steps, and keeps
the result for the rest of the run. That makes a before-and-after check possible: the snapshot holds
the old value, and a later read of the same account gives the new one.

`step.let` does exactly the same thing, under a name that reads better when you are not comparing
before and after. Read either back with `expression.snapshot` or `expression.variable`. These named
values, called bindings, follow a few rules:

- A name can be used only by the steps after it, and a name defined inside a loop body cannot be
  read after the loop ends.
- A binding cannot be changed, with one exception: inside a loop, `step.assign` can update a
  variable that the loop carries from one pass to the next, as shown in
  [Batch execution](/guide/batching#carry-a-total-across-rows).
- Bindings create no accounts, cost no rent, and do not outlast the transaction. The compiler turns
  each name into a numbered slot for the run's working values, and the name itself never goes on
  chain. Bindings made inside a loop body are set again on every row.

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

In Rust there are no names: the snapshot is simply the register that holds the first balance read,
and the check after the transfer reads the balance into a new register.

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
