# Payment patterns

Templates that pay out SOL: a payroll, a revenue split, weighted rewards, a refund with a deadline
and a capped sweep. Each shows the template, the code that runs it, and a cost table.

Each cost table was measured with Mollusk, a tool that runs Solana programs locally. It compares
one Ballista run with the plain System Program transfers that do the same work, in compute units
(Solana's measure of execution cost) and transaction bytes. A column headed **Plain instructions,
weaker checks** means the plain transfers move the same SOL but can't enforce the template's check on
chain. The upload rows are paid once, when the template is stored.

::: warning Batching alone is not a reason
A plain transaction can already send many transfers. Thirty System transfers fit in one 1,670-byte
transaction, well inside the 4,096-byte limit of a [v1 transaction](/guide/transaction-v1), and use
far less compute than the template. The payroll below is one of those cases: the template adds a
single instruction and a sequence of calls that is stored on chain and was checked when it was
uploaded, and nothing else. Templates earn their cost when an amount or a decision only exists
while the transaction runs; see [amounts nobody knows at signing](/guide/runtime-values) and
[loops that read as they go](/guide/loops).
:::

## Bounded SOL payroll

Pay the same amount to up to 30 recipients with one Ballista instruction. The recipients form a
batch: a list of accounts supplied when the template runs, where each entry is a row.
`step.forEach` runs the transfer once per row. The Rust tabs build the same template with the
lower-level `ProgramBuilder`, introduced in [Author it in Rust](/guide/getting-started#author-it-in-rust).

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

One Ballista instruction for 30 rows, compared with 30 plain instructions. One System transfer per recipient does the same work. What the template adds is one instruction and a sequence of calls stored on chain, not something plain instructions cannot do.

<!-- /benchmark -->

## Basis-point revenue split

Split `total` lamports (the smallest unit of SOL) between a partner and a treasury. `partnerBps` is
the partner's share in basis points, or hundredths of a percent, so 10,000 is 100%. The template
rejects a share above 10,000, pays the partner `total × partnerBps / 10,000` rounded down, and pays
the treasury `total` minus that. Because the treasury's amount is a subtraction, rounding can't
create or lose lamports.

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

| Cost | Ballista | Plain instructions, weaker checks | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 5,729 | 300 | +5,429 |
| Transaction bytes, every run | 324 | 270 | +54 |
| Compute units, upload once | 5,367 | none | — |
| Transaction bytes, upload once | 604 in 1 transaction | none | — |
| Rent locked in the template account | 0.00272 SOL for 408 bytes | none | — |

One Ballista instruction, compared with 2 plain instructions. Two transfers with client-computed amounts settle the same way, but nothing on chain ties the two amounts to one total or bounds the share. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->

## Index-weighted rewards

Pay each recipient a multiple of `base` set by its place in the list: the first gets 1 × `base`,
the second 2 × `base`, and so on. `expression.loopIndex()` is the current row's position, starting
at 0.

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
// The order of the recipient accounts sets each one's multiple of base.
```

:::

<!-- benchmark:index-weighted-rewards -->

| Cost | Ballista | Plain instructions, weaker checks | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 68,356 | 4,500 | +63,856 |
| Transaction bytes, every run | 1,240 | 1,670 | −430 |
| Compute units, upload once | 5,742 | none | — |
| Transaction bytes, upload once | 508 in 1 transaction | none | — |
| Rent locked in the template account | 0.00224 SOL for 312 bytes | none | — |

One Ballista instruction for 30 rows, compared with 30 plain instructions. Transfers with client-computed weights settle the same way; the weighting rule itself is not enforced on chain. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->

## Deadline refund

Refund `refundAmount` lamports to the customer only if the run executes at or before `deadline`, a
Unix timestamp. After the deadline, the `when` condition skips the transfer and the run still
succeeds.

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
Ballista has no authority over the escrow. `escrowAuthority` must sign the transaction itself. If
the refund comes from a call to an escrow program instead, that program must approve it by its own
rules.
:::

<!-- benchmark:deadline-refund -->

| Cost | Ballista | Plain instructions, weaker checks | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,480 | 150 | +3,330 |
| Transaction bytes, every run | 291 | 220 | +71 |
| Compute units, upload once | 4,550 | none | — |
| Transaction bytes, upload once | 480 in 1 transaction | none | — |
| Rent locked in the template account | 0.00209 SOL for 284 bytes | none | — |

One Ballista instruction, compared with 1 plain instruction. A bare transfer refunds unconditionally; the deadline is only checked by whoever builds the transaction. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->

## Reserve-preserving sweep

Move up to `cap` lamports from `payer` to `vault` without letting `payer` fall below `reserve`. The
template records the balance and fails if it is already below `reserve`. It then sends the smaller
of `cap` and the amount above the reserve, and checks that the balance is still at least
`reserve`.

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

| Cost | Ballista | Plain instructions, weaker checks | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 4,097 | 150 | +3,947 |
| Transaction bytes, every run | 291 | 220 | +71 |
| Compute units, upload once | 15,710 | none | — |
| Transaction bytes, upload once | 576 in 1 transaction | none | — |
| Rent locked in the template account | 0.00258 SOL for 380 bytes | none | — |

One Ballista instruction, compared with 1 plain instruction. A transfer of a client-computed amount can be built, but no on-chain check proves the reserve survived. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->
