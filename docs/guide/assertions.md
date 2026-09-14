# Assertions and snapshots

`step.require(condition)` aborts the complete transaction unless its boolean expression is true.
Conditions can combine account reads, inputs, time, checked math, comparisons, `AND`, `OR`, and
`NOT`.

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

## What `let` means

`let` evaluates once and assigns a compiler-visible name to the result register. It is immutable,
lexically scoped, and creates no account or persistent state. `snapshot` is an alias chosen to make
pre/post checks read naturally. Loop-local bindings are rebuilt each iteration and cannot escape
the loop body.
