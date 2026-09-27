# Assertions and snapshots

This page shows how a template checks conditions with `step.require`, and how a snapshot lets it
compare an account before and after a call.

`step.require(condition)` fails the whole transaction unless its condition is true. A condition can
combine account reads, inputs, the clock, checked arithmetic, comparisons, and the boolean
operators `and`, `or`, and `not`.

## Why snapshots exist

An expression is evaluated again everywhere it appears. A read of an account's balance happens anew
at each place it is used, so the same expression written before and after a CPI (a call to another
program) gives two different values, and there is no way to keep the first one. Comparing the
balance after a transfer with a second read of that same balance proves nothing.

`step.snapshot(name, value)` evaluates its expression once, at that point in the steps, and keeps
the result for the rest of the run. That makes a before-and-after check possible: the snapshot holds
the old value, and a later read of the same account gives the new one.

`step.let` does exactly the same thing, under a name that reads better when you are not comparing
before and after. Read either back with `expression.snapshot` or `expression.variable`. These named
values, called bindings, follow a few rules:

- A name can be used only by the steps after it, and a name defined inside a loop body cannot be
  read after the loop ends.
- A binding cannot be changed, with one exception: inside a batch loop, `step.assign` can update a
  variable that the loop carries from row to row, as shown in
  [Batch execution](/guide/batching#carry-a-total-across-rows).
- Bindings create no accounts, cost no rent, and do not outlast the transaction. The compiler turns
  each name into a numbered slot for the run's working values, and the name itself never goes on
  chain. Bindings made inside a loop body are set again on every row.

## Exact lamport delta

This template records the sender's balance in lamports (the smallest unit of SOL), transfers an
amount, then requires that the balance fell by exactly that amount.

::: code-group

```ts [TypeScript · template]
steps: [
  step.snapshot(
    'before',
    expression.accountField(account.fixed('sender'), 'lamports'),
  ),
  systemTransfer({
    systemProgram: account.fixed('systemProgram'),
    from: account.fixed('sender'),
    to: account.fixed('recipient'),
    lamports: expression.input('amount'),
  }),
  step.require(
    expression.equal(
      expression.accountField(account.fixed('sender'), 'lamports'),
      expression.subtract(
        expression.snapshot('before'),
        expression.input('amount'),
      ),
    ),
  ),
]
```

```rust [Rust · run]
let amount = 50_000_000u64;
let run = ballista_sdk::run_instruction(
    delta_checked_template,
    vec![
        AccountMeta::new_readonly(system_program, false),
        AccountMeta::new(sender, true),
        AccountMeta::new(recipient, false),
    ],
    &amount.to_le_bytes(),
);
// If the balance did not fall by exactly `amount`, the run fails with RequirementFailed
// and the transfer is rolled back.
```

:::

## Token amount delta

The same check works for tokens. An SPL token account stores its balance as a `u64` at byte offset
64.

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

## Compound guards

Conditions can be combined. This one lets the run continue when it is enabled, the deadline has not
passed, and the expected output meets the minimum, or when an emergency override is set.

```ts
const canExecute = expression.and(
  expression.input('enabled'),
  expression.and(
    expression.lessThanOrEqual(expression.clockUnixTimestamp(), expression.input('deadline')),
    expression.greaterThanOrEqual(expression.input('expectedOut'), expression.input('minimumOut')),
  ),
);

step.require(expression.or(canExecute, expression.input('emergencyOverride')));
```
