# Loops that decide per row

This page shows loops in which each row decides what to do from state read during the run: pay
creditors in priority order, collect only the token accounts that hold a balance, process only
the queue entries that are due, and split a balance by weight.

A template can repeat a set of steps once for each row of accounts the caller passes. The template
declares the loop in `batch`: `row` names the accounts in each row, `rowInputs` declares values the
caller supplies for each row, and `maxIterations` caps the number of rows. `step.forEach` holds
the steps that run for each row. Inside it, `account.iteration('name')` is the current row's
account and `expression.rowInput('name')` is the current row's value.

The accounts the caller passes fix how many rows run. What each row does can still depend on what
earlier rows spent, or on the row's own account. `when` skips a single call when its condition is
false. In the Rust tab, the row accounts follow the fixed accounts, and the row inputs follow the
fixed inputs.

::: info How to read the cost tables
Each example ends with measured costs for eight rows. Compute units measure on-chain work. The
Ballista column is one run of the template. The plain column is the closest sequence of ordinary
instructions, one per row. It does not do the same job, because every amount and every call in it
is fixed at signing. The "upload once" and rent rows are one-time costs of storing the template on
chain. Where an example calls another protocol, the measurement calls a SOL transfer in its place,
so neither column includes that protocol's own work. The figures come from Mollusk, a harness that
runs Solana programs without a validator. The [benchmarks page](/benchmarks) describes the method.
:::

## Waterfall until the money runs out

Pay creditors from a treasury in priority order, the first row first. Each payment is capped by
what is left, and what is left is known only when the transaction executes.

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

Each row is one creditor account, marked writable (the transaction may change it) because it
receives lamports (the smallest unit of SOL). `remaining` starts as the treasury's balance minus a
reserve. Each row pays the smaller of `remaining` and that creditor's `owed` amount, then
subtracts the payment.

`carry: ['remaining']` is what passes the balance from one row to the next. A variable created
before the loop and listed in `carry` can be updated with `step.assign` inside the loop, so row
four sees what rows one to three paid. Variables created inside the loop, such as `pay`, start
fresh on every row. Once the money runs out, `pay` is zero and `when` skips the transfer, so the
later creditors get nothing and the transaction still succeeds.

<!-- benchmark:waterfall-until-the-money-runs-out -->

| Cost | Ballista | Closest plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 23,452 | 1,200 | +22,252 |
| Transaction bytes, every run | 578 | 570 | +8 |
| Compute units, upload once | 9,285 | none | — |
| Transaction bytes, upload once | 576 in 1 transaction | none | — |
| Rent locked in the template account | 0.00258 SOL for 380 bytes | none | — |

One Ballista instruction for 8 rows, compared with 8 plain instructions. Paying creditors in priority order until the money runs out means each payment depends on the ones before it and on a balance read at execution. A fixed list of transfers either overdraws or stops early.

<!-- /benchmark -->

## Consolidate only the funded accounts

Move the whole balance of each token account in the batch into one vault, and skip the accounts
that are empty.

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

Each row reads its own account's balance (the `u64` at offset 64 of an SPL Token account) and
transfers all of it, and `when` skips the transfer when the balance is zero. Sending one plain
Transfer instruction per account needs every amount before signing, and the whole transaction
fails at the first account that turns out to be empty.

<!-- benchmark:consolidate-only-the-funded-accounts -->

| Cost | Ballista | Closest plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 18,611 | 608 | +18,003 |
| Transaction bytes, every run | 539 | 586 | −47 |
| Compute units, upload once | 7,110 | none | — |
| Transaction bytes, upload once | 479 in 1 transaction | none | — |
| Rent locked in the template account | 0.00209 SOL for 283 bytes | none | — |

One Ballista instruction for 8 rows, compared with 8 plain instructions. Each account moves its own balance, which nobody knows until execution, and an empty one must be skipped rather than fail. A fixed list of transfers needs every amount up front and reverts on the first empty account.

<!-- /benchmark -->

## Crank only the ripe entries

A crank is a transaction that processes the items waiting in a protocol's queue. Keepers (bots
that do routine maintenance for a protocol) send them. This template settles only the entries
whose deadline has passed.

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

Each row is one queue entry. The template reads the entry's deadline, a Unix timestamp at
`DEADLINE_OFFSET`, and compares it with the network's clock when the transaction executes. For
each entry that is due, `step.invoke` makes a CPI (a cross-program invocation: one program calling
another) into the protocol. The keeper can pass the whole queue and let the run decide. Filtering
the queue before sending would compare the deadlines with a time that has already passed when the
transaction runs.

<!-- benchmark:crank-only-the-ripe-entries -->

| Cost | Ballista | Closest plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 19,209 | 1,200 | +18,009 |
| Transaction bytes, every run | 506 | 570 | −64 |
| Compute units, upload once | 5,653 | none | — |
| Transaction bytes, upload once | 488 in 1 transaction | none | — |
| Rent locked in the template account | 0.00213 SOL for 292 bytes | none | — |

One Ballista instruction for 8 rows, compared with 8 plain instructions. A SOL transfer stands in for the protocol call, so neither column includes the protocol's own work. Which entries are due depends on the clock at execution. Sending one instruction per entry reverts the whole transaction on the first one that is not ready yet, and filtering beforehand can be out of date by the time it runs.

<!-- /benchmark -->

## Distribute a runtime pot pro rata

Split a vault's balance above a reserve (the pot) among holders, each in proportion to a weight
the caller passes.

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

`weightBps` is each holder's share in basis points (hundredths of a percent, so 10,000 is 100%).
The caller chooses the weights, and the template reads the pot when the transaction executes.
Computing the shares off chain would divide a balance that may have changed by then. Each share
is rounded down, and whatever the rounding leaves over stays in the vault.

<!-- benchmark:distribute-a-runtime-pot-pro-rata -->

| Cost | Ballista | Closest plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 21,022 | 1,200 | +19,822 |
| Transaction bytes, every run | 578 | 570 | +8 |
| Compute units, upload once | 10,518 | none | — |
| Transaction bytes, upload once | 544 in 1 transaction | none | — |
| Rent locked in the template account | 0.00242 SOL for 348 bytes | none | — |

One Ballista instruction for 8 rows, compared with 8 plain instructions. Shares are a fraction of a pot that is still filling. Transfers computed off chain divide yesterday’s number, and the rounding remainder has to go somewhere the caller cannot predict.

<!-- /benchmark -->
