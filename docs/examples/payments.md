# Payment patterns

::: warning Batching alone is not a reason
Thirty transfers fit in a 1,240-byte transaction, comfortably inside the 4,096-byte limit. Where
the table below says **Yes, same guarantees**, a plain transaction already does the job for a
fraction of the compute and no rent; the template buys one instruction and a stored, verified
shape, and nothing else. The patterns worth reaching for are the ones whose amounts or decisions
only exist during execution — see [amounts nobody knows at signing](/examples/runtime-values) and
[loops that read as they go](/examples/loops).
:::

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

| Cost | Ballista | Plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 51,741 | 4,500 | +47,241 |
| Transaction bytes, every run | 1,240 | 1,670 | −430 |
| Compute units, upload once | 9,817 | none | — |
| Transaction bytes, upload once | 444 in 1 transaction | none | — |
| Rent locked in the template account | 0.00191 SOL for 248 bytes | none | — |

One Ballista instruction covering 30 rows against 30 plain instructions, measured with Mollusk. One System transfer per recipient does the same work. Ballista buys one instruction and a stored, verified shape, not a capability you lack.

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

| Cost | Ballista | Plain instructions, weaker | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 5,729 | 300 | +5,429 |
| Transaction bytes, every run | 324 | 270 | +54 |
| Compute units, upload once | 5,367 | none | — |
| Transaction bytes, upload once | 604 in 1 transaction | none | — |
| Rent locked in the template account | 0.00272 SOL for 408 bytes | none | — |

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

| Cost | Ballista | Plain instructions, weaker | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 68,356 | 4,500 | +63,856 |
| Transaction bytes, every run | 1,240 | 1,670 | −430 |
| Compute units, upload once | 5,742 | none | — |
| Transaction bytes, upload once | 508 in 1 transaction | none | — |
| Rent locked in the template account | 0.00224 SOL for 312 bytes | none | — |

One Ballista instruction covering 30 rows against 30 plain instructions, measured with Mollusk. Transfers with client-computed weights settle the same way; the weighting rule itself is not enforced on chain. Enforcing that on chain any other way means deploying your own program.

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

| Cost | Ballista | Plain instructions, weaker | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,480 | 150 | +3,330 |
| Transaction bytes, every run | 291 | 220 | +71 |
| Compute units, upload once | 4,550 | none | — |
| Transaction bytes, upload once | 480 in 1 transaction | none | — |
| Rent locked in the template account | 0.00209 SOL for 284 bytes | none | — |

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

| Cost | Ballista | Plain instructions, weaker | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 4,097 | 150 | +3,947 |
| Transaction bytes, every run | 291 | 220 | +71 |
| Compute units, upload once | 15,710 | none | — |
| Transaction bytes, upload once | 576 in 1 transaction | none | — |
| Rent locked in the template account | 0.00258 SOL for 380 bytes | none | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. A transfer of a client-computed amount can be built, but no on-chain check proves the reserve survived. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->
