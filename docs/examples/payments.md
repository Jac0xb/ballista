# Payment patterns

## Bounded SOL payroll

Pay up to 30 recipients the same amount while sending only one Ballista instruction.

::: code-group

```ts [TypeScript · template]
const payroll = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_BYTES },
    treasury: { signer: true, writable: true },
  },
  batch: { maxIterations: 30, row: { recipient: { writable: true } } },
  steps: [step.forEach([
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('treasury'),
      to: account.iteration('recipient'),
      lamports: expression.input('amount'),
    }),
  ])],
});
```

```rust [Rust · run]
let mut metas = vec![
    AccountMeta::new_readonly(system_program, false),
    AccountMeta::new(treasury, true),
];
metas.extend(recipients.iter().map(|key| AccountMeta::new(*key, false)));
let run = ballista_sdk::run_instruction(template, metas, &amount.to_le_bytes());
```

:::

## Basis-point revenue split

Split an input amount among two fixed recipients. The final transfer uses subtraction, so integer
rounding cannot create or lose lamports inside the split.

::: code-group

```ts [TypeScript · template]
const partnerAmount = expression.divide(
  expression.multiply(expression.input('total'), expression.input('partnerBps')),
  expression.u64(10_000),
);

steps: [
  step.require(expression.lessThanOrEqual(expression.input('partnerBps'), expression.u64(10_000))),
  step.let('partnerAmount', partnerAmount),
  systemTransfer({
    ...shared,
    to: account.fixed('partner'),
    lamports: expression.variable('partnerAmount'),
  }),
  systemTransfer({
    ...shared,
    to: account.fixed('treasury'),
    lamports: expression.subtract(
      expression.input('total'),
      expression.variable('partnerAmount'),
    ),
  }),
]
```

```rust [Rust · inputs]
let mut inputs = Vec::new();
inputs.extend_from_slice(&total.to_le_bytes());
inputs.extend_from_slice(&partner_bps.to_le_bytes());
let run = ballista_sdk::run_instruction(template, metas, &inputs);
```

:::

## Index-weighted rewards

Use the bounded iteration index to pay row `n` exactly `(n + 1) × base`.

::: code-group

```ts [TypeScript · loop body]
step.forEach([
  systemTransfer({
    systemProgram: account.fixed('systemProgram'),
    from: account.fixed('treasury'),
    to: account.iteration('recipient'),
    lamports: expression.multiply(
      expression.add(expression.loopIndex(), expression.u64(1)),
      expression.input('base'),
    ),
  }),
]);
```

```rust [Rust · run]
let inputs = base.to_le_bytes();
let run = ballista_sdk::run_instruction(template, ordered_recipient_metas, &inputs);
// Account-row order defines the reward multiplier.
```

:::

## Deadline refund

Execute a refund only while a signed quote remains valid.

::: code-group

```ts [TypeScript · template]
systemTransfer({
  systemProgram: account.fixed('systemProgram'),
  from: account.fixed('escrowAuthority'),
  to: account.fixed('customer'),
  lamports: expression.input('refundAmount'),
  when: expression.lessThanOrEqual(
    expression.clockUnixTimestamp(),
    expression.input('deadline'),
  ),
});
```

```rust [Rust · inputs]
let mut inputs = Vec::new();
inputs.extend_from_slice(&refund_amount.to_le_bytes());
inputs.extend_from_slice(&deadline.to_le_bytes());
let run = ballista_sdk::run_instruction(template, metas, &inputs);
```

:::

::: warning Authorization
Ballista does not control the escrow. `escrowAuthority` must already sign the transaction, or the
downstream program must authorize the operation from its own state.
:::

## Reserve-preserving sweep

Sweep at most `cap` while proving the payer remains above its required reserve.

::: code-group

```ts [TypeScript · template]
const balance = expression.accountField(account.fixed('payer'), 'lamports');

steps: [
  step.snapshot('before', balance),
  step.require(expression.greaterThanOrEqual(expression.snapshot('before'), expression.input('reserve'))),
  systemTransfer({
    systemProgram: account.fixed('systemProgram'),
    from: account.fixed('payer'),
    to: account.fixed('vault'),
    lamports: expression.min(
      expression.subtract(expression.snapshot('before'), expression.input('reserve')),
      expression.input('cap'),
    ),
  }),
  step.require(expression.greaterThanOrEqual(balance, expression.input('reserve'))),
]
```

```rust [Rust · inputs]
let mut inputs = Vec::new();
inputs.extend_from_slice(&reserve.to_le_bytes());
inputs.extend_from_slice(&cap.to_le_bytes());
let run = ballista_sdk::run_instruction(template, metas, &inputs);
```

:::
