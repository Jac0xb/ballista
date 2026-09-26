# Batch execution

A template may declare one repeated account row, and beside it a row of inputs. The caller supplies
rows after the fixed accounts, and the VM infers the iteration count from the remaining account
count. Each row's input values travel in the run data, so every iteration can carry its own
amount, recipient-specific parameter, or flag. A template can require a minimum number of rows so
a batch cannot succeed vacuously with none.

## Thirty-recipient payroll

::: code-group

```ts [TypeScript · template]
const payroll = defineTemplate({
  inputs: { lamportsPerRecipient: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    treasury: { signer: true, writable: true },
  },
  batch: {
    maxIterations: 30,
    minIterations: 1,
    row: { recipient: { writable: true } },
  },
  steps: [
    step.forEach([
      systemTransfer({
        systemProgram: account.fixed('systemProgram'),
        from: account.fixed('treasury'),
        to: account.iteration('recipient'),
        lamports: expression.input('lamportsPerRecipient'),
      }),
    ]),
  ],
});
```

```ts [TypeScript · run]
const instruction = buildKitRunInstruction({
  compiled,
  templateAddress,
  inputs: { lamportsPerRecipient: 10_000n },
  accounts: {
    systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
    treasury: { address: treasury },
  },
  batchRows: recipients.map((recipient) => ({ recipient: { address: recipient } })),
});
// Throws before sending if there are more than 30 rows or fewer than 1.
```

```rust [Rust · template]
let mut builder = ProgramBuilder::new();
let system = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ID.to_bytes()), None, 0);
let treasury = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
let recipient = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
builder.batch(30, 1);
let lamports_input = builder.input(VALUE_U64, 0);
let lamports = builder.load_input(lamports_input);
let discriminator = builder.blob(&[2, 0, 0, 0]);
let transfer = builder.cpi(
    system,
    &[(treasury, ACCOUNT_SIGNER | ACCOUNT_WRITABLE), (recipient, ACCOUNT_WRITABLE)],
    &[Segment::Literal(discriminator), Segment::Register(DATA_REG_U64, lamports)],
);
builder.for_each(0, |body| body.invoke(transfer, None));
let payload = builder.build()?;
```

```rust [Rust · inputs and run]
let inputs = RunInputs::new().u64(lamports_per_recipient).finish();
let mut accounts = vec![
    AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
    AccountMeta::new(treasury, true),
];
accounts.extend(recipients.iter().map(|key| AccountMeta::new(*key, false)));
let run = ballista_sdk::run_instruction(template, accounts, &inputs);
```

:::

## A different amount per row

`batch.rowInputs` declares inputs that are carried once per iteration. Inside `forEach`,
`expression.rowInput(name)` reads the current row's value. At run time the caller passes one
input record per row, in the same order as the rows.

::: code-group

```ts [TypeScript · template]
const payroll = defineTemplate({
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    treasury: { signer: true, writable: true },
  },
  batch: {
    maxIterations: 30,
    minIterations: 1,
    row: { recipient: { writable: true } },
    rowInputs: { amount: { type: 'u64' } },
  },
  steps: [
    step.forEach([
      systemTransfer({
        systemProgram: account.fixed('systemProgram'),
        from: account.fixed('treasury'),
        to: account.iteration('recipient'),
        lamports: expression.rowInput('amount'),
      }),
    ]),
  ],
});
```

```ts [TypeScript · run]
const instruction = buildKitRunInstruction({
  compiled,
  templateAddress,
  accounts: {
    systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
    treasury: { address: treasury },
  },
  batchRows: payees.map((payee) => ({ recipient: { address: payee.address } })),
  batchInputs: payees.map((payee) => ({ amount: payee.lamports })),
});
```

```rust [Rust · template]
let mut builder = ProgramBuilder::new();
let system = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ID.to_bytes()), None, 0);
let treasury = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
let recipient = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
builder.batch(30, 1);
let amount_input = builder.row_input(VALUE_U64, 0);
let discriminator = builder.blob(&[2, 0, 0, 0]);
builder.for_each(0, |body| {
    let amount = body.load_input(amount_input);
    let transfer = body.cpi(
        system,
        &[(treasury, ACCOUNT_SIGNER | ACCOUNT_WRITABLE), (recipient, ACCOUNT_WRITABLE)],
        &[Segment::Literal(discriminator), Segment::Register(DATA_REG_U64, amount)],
    );
    body.invoke(transfer, None);
});
```

```rust [Rust · inputs and run]
// Fixed inputs first (none here), then one row of values per recipient, in row order.
let mut inputs = RunInputs::new();
for payee in &payees {
    inputs = inputs.u64(payee.lamports);
}
let run = ballista_sdk::run_instruction(template, accounts, &inputs.finish());
```

:::

Row inputs share the input table with the fixed inputs (32 descriptors in total, at most 8 per row),
and the run may carry at most 256 values: fixed inputs plus row inputs times the maximum iteration
count. Encoded run data is still capped at 1,024 bytes, which is the practical bound on `bytes` row
inputs. A run whose row values do not match its rows fails with `InvalidRunInputs`, and the error's
context is the index of the first missing or malformed value, counting fixed values first.

Rows carry values and accounts, not CPI shapes: an `invoke` inside the loop forwards the same
[account group](./account-groups) on every iteration.

## Carry a total across rows

Registers written inside the loop body are discarded after each iteration, except the ones the
loop carries. A carried variable is defined before the loop, reassigned inside it, and readable
after it, so a template can enforce a bound over the whole batch.

::: code-group

```ts [TypeScript · template]
steps: [
  step.let('total', expression.u64(0)),
  step.forEach(
    [
      systemTransfer({ /* row transfer as above */ }),
      step.assign(
        'total',
        expression.add(expression.variable('total'), expression.input('lamportsPerRecipient')),
      ),
    ],
    { carry: ['total'] },
  ),
  step.require(
    expression.lessThanOrEqual(expression.variable('total'), expression.input('budget')),
    'withinBudget',
  ),
]
```

```ts [TypeScript · run]
const instruction = buildKitRunInstruction({
  compiled,
  templateAddress,
  inputs: { lamportsPerRecipient: 10_000n, budget: 250_000n },
  accounts: { systemProgram: { address: SYSTEM_PROGRAM_ADDRESS }, treasury: { address: treasury } },
  batchRows: recipients.map((recipient) => ({ recipient: { address: recipient } })),
});

// A breach fails the labelled require; the code's high bits name the instruction:
explainRunError(code, compiled)?.message; // 'RequirementFailed at steps[2] (withinBudget)'
```

```rust [Rust · template]
let amount_input = builder.input(VALUE_U64, 0);
let budget_input = builder.input(VALUE_U64, 0);
let amount = builder.load_input(amount_input);
let budget = builder.load_input(budget_input);
let total = builder.const_u64(0);
builder.for_each(1 << total, |body| {
    body.invoke(transfer, None);
    let sum = body.binary(OP_ADD, total, amount);
    body.mov(total, sum);
});
let within = builder.binary(OP_LTE, total, budget);
builder.require(within);
```

```rust [Rust · inputs and run]
let inputs = RunInputs::new().u64(lamports_per_recipient).u64(budget).finish();
let run = ballista_sdk::run_instruction(template, accounts, &inputs);
```

:::

The carry mask lives in the `forEach` instruction's immediate. The verifier requires every carried
register to be initialized before the loop and to keep its type through the body; a carried `bytes`
value must also keep its maximum length.

## Stride-two rows

```ts
batch: {
  maxIterations: 20,
  row: {
    owner: {},
    tokenAccount: { writable: true },
  },
},
steps: [
  step.forEach([
    assertAta({
      associatedTokenAccount: account.iteration('tokenAccount'),
      owner: account.iteration('owner'),
      mint: account.fixed('mint'),
      tokenProgram: account.fixed('tokenProgram'),
      associatedTokenProgram: account.fixed('associatedTokenProgram'),
    }),
    ensureAssociatedTokenAccount({ /* row account references */ }),
    tokenTransfer({ /* row destination */ }),
  ]),
]
```

The row stride must be `1..=8`, the tail count must divide evenly by the stride, and rows cannot
exceed `maxIterations` or fall below `minIterations`. There is no nested loop, backward jump, or
condition-controlled `while`. Root steps may execute before and after the loop.

::: warning Count CPIs, not just rows
The hard ceiling is 64 expanded CPIs. A 30-row body with two CPIs expands to 60; a third CPI would
make the template invalid at finalization.
:::
