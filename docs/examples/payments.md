# Payment patterns

## Bounded SOL payroll

Pay up to 30 recipients the same amount while sending only one Ballista instruction.

::: code-group

```ts [TypeScript · template]
const payroll = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    treasury: { signer: true, writable: true },
  },
  batch: { maxIterations: 30, minIterations: 1, row: { recipient: { writable: true } } },
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

```ts [TypeScript · run]
const instruction = buildKitRunInstruction({
  compiled: compileTemplate(payroll),
  templateAddress,
  inputs: { amount: 10_000n },
  accounts: {
    systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
    treasury: { address: treasury },
  },
  batchRows: recipients.map((recipient) => ({ recipient: { address: recipient } })),
});
```

```rust [Rust · template]
let mut builder = ProgramBuilder::new();
let system = builder.account(ACCOUNT_EXECUTABLE, Some(SYSTEM_PROGRAM_ID.to_bytes()), None, 0);
let treasury = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
let recipient = builder.row_account(ACCOUNT_WRITABLE, None, None, 0);
builder.batch(30, 1);
let amount_input = builder.input(VALUE_U64, 0);
let amount = builder.load_input(amount_input);
let discriminator = builder.blob(&[2, 0, 0, 0]);
let transfer = builder.cpi(
    system,
    &[(treasury, ACCOUNT_SIGNER | ACCOUNT_WRITABLE), (recipient, ACCOUNT_WRITABLE)],
    &[Segment::Literal(discriminator), Segment::Register(DATA_REG_U64, amount)],
);
builder.for_each(0, |body| body.invoke(transfer, None));
let payload = builder.build()?;
```

```rust [Rust · inputs and run]
let inputs = RunInputs::new().u64(amount).finish();
let mut metas = vec![
    AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
    AccountMeta::new(treasury, true),
];
metas.extend(recipients.iter().map(|key| AccountMeta::new(*key, false)));
let run = ballista_sdk::run_instruction(template, metas, &inputs);
```

:::

<!-- benchmark:bounded-sol-payroll -->

| Approach | Compute units | Transaction bytes | Stored on chain |
| --- | ---: | ---: | --- |
| Ballista | 16,712 | 514 | 168-byte template, once |
| Plain instructions | 1,200 | 570 | none |
| Difference | +15,512 | −56 | — |

One Ballista instruction covering 8 rows against 8 plain instructions, measured with Mollusk. One System transfer per recipient does the same work. Ballista buys one instruction and a stored, verified shape, not a capability you lack.

<!-- /benchmark -->

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
let inputs = RunInputs::new().u64(total).u64(partner_bps).finish();
let run = ballista_sdk::run_instruction(template, metas, &inputs);
```

:::

<!-- benchmark:basis-point-revenue-split -->

| Approach | Compute units | Transaction bytes | Stored on chain |
| --- | ---: | ---: | --- |
| Ballista | 5,948 | 324 | 376-byte template, once |
| Plain instructions, weaker | 300 | 270 | none |
| Difference | +5,648 | +54 | — |

One Ballista instruction against 2 plain instructions, measured with Mollusk. Two transfers with client-computed amounts settle the same way, but nothing on chain ties the two amounts to one total or bounds the share. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->

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

<!-- benchmark:index-weighted-rewards -->

| Approach | Compute units | Transaction bytes | Stored on chain |
| --- | ---: | ---: | --- |
| Ballista | 20,500 | 514 | 232-byte template, once |
| Plain instructions, weaker | 1,200 | 570 | none |
| Difference | +19,300 | −56 | — |

One Ballista instruction covering 8 rows against 8 plain instructions, measured with Mollusk. Transfers with client-computed weights settle the same way; the weighting rule itself is not enforced on chain. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->

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
let inputs = RunInputs::new().u64(refund_amount).i64(deadline).finish();
let run = ballista_sdk::run_instruction(template, metas, &inputs);
```

:::

::: warning Authorization
Ballista does not control the escrow. `escrowAuthority` must already sign the transaction, or the
downstream program must authorize the operation from its own state.
:::

<!-- benchmark:deadline-refund -->

| Approach | Compute units | Transaction bytes | Stored on chain |
| --- | ---: | ---: | --- |
| Ballista | 3,450 | 291 | 204-byte template, once |
| Plain instructions, weaker | 150 | 220 | none |
| Difference | +3,300 | +71 | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. A bare transfer refunds unconditionally; the deadline is only checked by whoever builds the transaction. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->

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
let inputs = RunInputs::new().u64(reserve).u64(cap).finish();
let run = ballista_sdk::run_instruction(template, metas, &inputs);
```

:::

<!-- benchmark:reserve-preserving-sweep -->

| Approach | Compute units | Transaction bytes | Stored on chain |
| --- | ---: | ---: | --- |
| Ballista | 4,257 | 291 | 332-byte template, once |
| Plain instructions, weaker | 150 | 220 | none |
| Difference | +4,107 | +71 | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. A transfer of a client-computed amount can be built, but no on-chain check proves the reserve survived. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->
