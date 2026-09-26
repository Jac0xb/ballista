# Loops that read as they go

A batch's row count is fixed by the account list, but what each row *does* need not be. These
templates decide per row, from what the previous rows left behind or from the row's own state.

## Waterfall until the money runs out

Pay creditors in priority order. Each payment is capped by what is left, and the balance is only
known during execution.

::: code-group

```ts [TypeScript · template]
batch: {
  maxIterations: 8,
  row: { creditor: { writable: true } },
  rowInputs: { owed: { type: 'u64' } },
},
steps: [
  step.let('remaining', expression.subtract(
    expression.accountField(account.fixed('treasury'), 'lamports'),
    expression.input('reserve'),
  )),
  step.forEach(
    [
      step.let('pay', expression.min(expression.variable('remaining'), expression.rowInput('owed'))),
      systemTransfer({
        systemProgram: account.fixed('systemProgram'),
        from: account.fixed('treasury'),
        to: account.iteration('creditor'),
        lamports: expression.variable('pay'),
        when: expression.greaterThan(expression.variable('pay'), expression.u64(0)),
      }),
      step.assign('remaining', expression.subtract(
        expression.variable('remaining'),
        expression.variable('pay'),
      )),
    ],
    { carry: ['remaining'] },
  ),
]
```

```rust [Rust · run]
let mut inputs = reserve.to_le_bytes().to_vec();
for owed in schedule {
    inputs.extend_from_slice(&owed.to_le_bytes());
}
let run = ballista_sdk::run_instruction(template, creditor_metas, &inputs);
```

:::

`carry` is what makes this a waterfall rather than a batch: `remaining` survives the iteration,
so row four sees what rows one to three spent. Creditors past the money get a skipped call, not a
failed transaction.

<!-- benchmark:waterfall-until-the-money-runs-out -->

| Cost | Ballista | Plain instructions, not equivalent | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 23,452 | 1,200 | +22,252 |
| Transaction bytes, every run | 578 | 570 | +8 |
| Compute units, upload once | 9,285 | none | — |
| Transaction bytes, upload once | 576 in 1 transaction | none | — |
| Rent locked in the template account | 0.00258 SOL for 380 bytes | none | — |

One Ballista instruction covering 8 rows against 8 plain instructions, measured with Mollusk. Paying creditors in priority order until the money runs out means each payment depends on the ones before it and on a balance read at execution. A fixed list of transfers either overdraws or stops early.

<!-- /benchmark -->

## Consolidate only the funded accounts

Each row moves its own balance and empty rows are skipped.

::: code-group

```ts [TypeScript · loop body]
step.forEach([
  step.let('amount', expression.accountData(account.iteration('source'), 64, 'u64')),
  tokenTransfer({
    tokenProgram: account.fixed('tokenProgram'),
    source: account.iteration('source'),
    destination: account.fixed('vault'),
    authority: account.fixed('authority'),
    amount: expression.variable('amount'),
    when: expression.greaterThan(expression.variable('amount'), expression.u64(0)),
  }),
])
```

```rust [Rust · run]
let mut metas = fixed_metas;
metas.extend(candidates.iter().map(|key| AccountMeta::new(*key, false)));
let run = ballista_sdk::run_instruction(template, metas, &[]);
```

:::

Sending one Transfer per account needs every amount up front and reverts on the first account
that turned out to be empty.

<!-- benchmark:consolidate-only-the-funded-accounts -->

| Cost | Ballista | Plain instructions, not equivalent | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 18,611 | 608 | +18,003 |
| Transaction bytes, every run | 539 | 586 | −47 |
| Compute units, upload once | 7,110 | none | — |
| Transaction bytes, upload once | 479 in 1 transaction | none | — |
| Rent locked in the template account | 0.00209 SOL for 283 bytes | none | — |

One Ballista instruction covering 8 rows against 8 plain instructions, measured with Mollusk. Each account moves its own balance, which nobody knows until execution, and an empty one must be skipped rather than fail. A fixed list of transfers needs every amount up front and reverts on the first empty account.

<!-- /benchmark -->

## Crank only the ripe entries

::: code-group

```ts [TypeScript · loop body]
step.forEach([
  step.invoke({
    program: account.fixed('protocolProgram'),
    accounts: settleAccounts,
    data: settleData,
    when: expression.lessThanOrEqual(
      expression.accountData(account.iteration('entry'), DEADLINE_OFFSET, 'i64'),
      expression.clockUnixTimestamp(),
    ),
  }),
])
```

```rust [Rust · run]
let run = ballista_sdk::run_instruction(template, queue_metas, &[]);
// Pass the whole queue; the run settles the entries that are due in this block.
```

:::

Which entries are due depends on the clock at execution. A keeper can pass the whole queue and
let the run decide, instead of filtering against a slot that has already passed.

<!-- benchmark:crank-only-the-ripe-entries -->

| Cost | Ballista | Plain instructions, not equivalent | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 19,209 | 1,200 | +18,009 |
| Transaction bytes, every run | 506 | 570 | −64 |
| Compute units, upload once | 5,653 | none | — |
| Transaction bytes, upload once | 488 in 1 transaction | none | — |
| Rent locked in the template account | 0.00213 SOL for 292 bytes | none | — |

One Ballista instruction covering 8 rows against 8 plain instructions, measured with Mollusk. The protocol call is stood in by a System transfer, so neither row includes the protocol's own work. Which entries are due depends on the clock at execution. Sending one instruction per entry reverts the whole batch on the first one that is not ready yet, and filtering beforehand races the block.

<!-- /benchmark -->

## Distribute a runtime pot pro rata

::: code-group

```ts [TypeScript · template]
step.let('pot', expression.subtract(
  expression.accountField(account.fixed('vault'), 'lamports'),
  expression.input('reserve'),
)),
step.forEach([
  systemTransfer({
    systemProgram: account.fixed('systemProgram'),
    from: account.fixed('vault'),
    to: account.iteration('holder'),
    lamports: expression.divide(
      expression.multiply(expression.variable('pot'), expression.rowInput('weightBps')),
      expression.u64(10_000),
    ),
  }),
]),
```

```rust [Rust · run]
let mut inputs = reserve.to_le_bytes().to_vec();
for weight_bps in weights {
    inputs.extend_from_slice(&weight_bps.to_le_bytes());
}
let run = ballista_sdk::run_instruction(template, holder_metas, &inputs);
```

:::

The weights are the caller's; the pot is the chain's. Computing the shares off chain divides a
number that has already moved.

<!-- benchmark:distribute-a-runtime-pot-pro-rata -->

| Cost | Ballista | Plain instructions, not equivalent | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 21,022 | 1,200 | +19,822 |
| Transaction bytes, every run | 578 | 570 | +8 |
| Compute units, upload once | 10,518 | none | — |
| Transaction bytes, upload once | 544 in 1 transaction | none | — |
| Rent locked in the template account | 0.00242 SOL for 348 bytes | none | — |

One Ballista instruction covering 8 rows against 8 plain instructions, measured with Mollusk. Shares are a fraction of a pot that is still filling. Transfers computed off chain divide yesterday’s number, and the rounding remainder has to go somewhere the caller cannot predict.

<!-- /benchmark -->
