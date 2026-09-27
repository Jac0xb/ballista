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

This template sends the same number of lamports (the smallest unit of SOL) from a treasury, which
must sign, to each of up to 30 recipients. `step.forEach` runs its steps once per row, and
`account.iteration('recipient')` refers to the current row's account.

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

`batch.rowInputs` declares inputs that every row carries. Inside `forEach`,
`expression.rowInput(name)` reads the current row's value. At run time the caller passes one set of
values per row, in the same order as the rows.

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

These limits apply to row inputs:

- Fixed inputs and row inputs together are limited to 32 declarations, with at most 8 per row.
- A run can carry at most 256 values: the fixed inputs plus the row inputs times the template's
  maximum number of rows.
- The whole run data is limited to 1,024 bytes, which in practice bounds `bytes` row inputs.

If the row values do not match the rows, the run fails with `InvalidRunInputs`, and the error
reports the index of the first missing or malformed value, counting fixed values first.

Rows supply accounts and values, not new calls: the loop body's CPIs (calls to other programs) are
the same for every row. An `invoke` inside the loop that forwards an account group forwards the
same group on every row.

## Carry a total across rows

Values set inside the loop body are discarded after each row, unless the loop carries them. A
carried variable is defined with `step.let` before the loop, listed in the loop's `carry` option,
updated inside the loop with `step.assign`, and still readable after the loop ends. That lets a
template enforce a limit over the whole batch, such as a total budget.

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

// Going over budget fails the labelled require, and explainRunError names that step:
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

In Rust, values live in registers (numbered slots the builder hands back), and the first argument to
`for_each` lists the registers to carry as a bit mask: `1 << total` carries the register that holds
the total, and `0` carries nothing. Finalization, the one-time check before a template is locked on
chain, confirms that every carried value is set before the loop and keeps its type through the loop
body. A carried `bytes` value must also keep its maximum length.

## Stride-two rows

A row can hold more than one account. The number of accounts in each row is its stride; here each
row has two, an owner and a token account. For every row, the template checks that the token account
is the owner's ATA (associated token account), creates the account if it does not exist, and
transfers tokens to it.

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

A row holds 1 to 8 accounts. The accounts passed for rows must make up a whole number of rows, and
the row count must lie between `minIterations` and `maxIterations`. There are no nested loops, no
backward jumps, and no `while` loops that run until a condition changes. Steps outside the loop can
run before it and after it.

::: warning Count CPIs, not just rows
A template can make at most 64 CPIs, counted for the worst case: the calls outside the loop, plus
the calls in the loop body times `maxIterations`. Calls with a `when` condition count too. A 30-row
loop with two calls counts as 60; a third call in the loop body would make 90, and finalization
would reject the template.
:::
