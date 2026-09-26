# Assertions and snapshots

`step.require(condition)` aborts the complete transaction unless its boolean expression is true.
Conditions can combine account reads, inputs, time, checked math, comparisons, `AND`, `OR`, and
`NOT`.

## Why snapshots exist

An expression is re-evaluated everywhere it appears. A read of an account balance compiles to a
fresh read at each use site, so the same expression written before and after a CPI yields two
different values, and there is no way to hold on to the first one. Comparing the balance after a
transfer against a second read of the balance after that transfer proves nothing.

`step.snapshot(name, value)` evaluates its expression once, at that position in the step list, and
keeps the result in a register for the rest of the run. That is what makes a before-and-after check
expressible. The snapshot is the old value, and a later read of the same account is the new one.

`step.let` is the identical operation under a name that reads better away from pre/post checks, and
either can be read back with `expression.snapshot` or `expression.variable`. Both are immutable and
lexically scoped. Neither allocates an account, costs rent, or survives the transaction: the name
is resolved by the compiler and never reaches the wire format. Bindings created inside a loop body
are rebuilt on each iteration and cannot be read after the loop ends.

## Exact lamport delta

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
// A wrong post-CPI delta returns RequirementFailed and rolls back the transfer.
```

:::

## Token amount delta

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
