# Amounts nobody knows at signing

A transaction fixes every byte of instruction data when it is signed. That is fine for a transfer
of a known amount and useless for the common case: move *what is there*, repay *what is owed*,
split *what arrived*. The number exists only during execution, and a signature cannot reach it.

These templates read the number out of an account mid-run and feed it to the call.

## Sweep above a reserve

Move everything over a floor, whatever the balance turns out to be.

::: code-group

```ts [TypeScript · template]
steps: [
  step.let('balance', expression.accountField(account.fixed('vault'), 'lamports')),
  step.require(expression.greaterThan(expression.variable('balance'), expression.input('reserve'))),
  systemTransfer({
    systemProgram: account.fixed('systemProgram'),
    from: account.fixed('vault'),
    to: account.fixed('destination'),
    lamports: expression.subtract(expression.variable('balance'), expression.input('reserve')),
  }),
]
```

```rust [Rust · run]
// The caller names the floor, never the amount.
let run = ballista_sdk::run_instruction(template, sweep_metas, &reserve.to_le_bytes());
```

:::

The `require` is what makes the subtraction safe: below the floor the run fails instead of
underflowing.

<!-- benchmark:sweep-above-a-reserve -->

| Cost | Ballista | Plain instructions, not equivalent | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,437 | 150 | +3,287 |
| Transaction bytes, every run | 283 | 220 | +63 |
| Compute units, upload once | 4,615 | none | — |
| Transaction bytes, upload once | 492 in 1 transaction | none | — |
| Rent locked in the template account | 0.00215 SOL for 296 bytes | none | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. The balance at execution is not known when the transaction is signed. A fixed amount either leaves dust behind or overdraws and fails, and anything that arrives between signing and execution is stranded.

<!-- /benchmark -->

## Forward the whole token balance

::: code-group

```ts [TypeScript · template]
steps: [
  step.let('balance', expression.accountData(account.fixed('source'), 64, 'u64')),
  step.require(expression.greaterThan(expression.variable('balance'), expression.u64(0))),
  tokenTransfer({
    tokenProgram: account.fixed('tokenProgram'),
    source: account.fixed('source'),
    destination: account.fixed('destination'),
    authority: account.fixed('authority'),
    amount: expression.variable('balance'),
  }),
]
```

```rust [Rust · run]
let run = ballista_sdk::run_instruction(template, token_metas, &[]);
```

:::

Offset 64 is the amount field of the legacy SPL Token account layout, which is why the schema
pins the owner and a minimum data length.

<!-- benchmark:forward-the-whole-token-balance -->

| Cost | Ballista | Plain instructions, not equivalent | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,350 | 76 | +3,274 |
| Transaction bytes, every run | 308 | 250 | +58 |
| Compute units, upload once | 4,520 | none | — |
| Transaction bytes, upload once | 479 in 1 transaction | none | — |
| Rent locked in the template account | 0.00209 SOL for 283 bytes | none | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. A Transfer carries a fixed amount. Emptying an account whose balance is still moving — fees accruing, a swap landing — needs the figure read during execution.

<!-- /benchmark -->

## Repay exactly what is owed

Debt accrues per slot. A number quoted to the client is stale before the transaction lands.

::: code-group

```ts [TypeScript · template]
step.let('owed', expression.accountData(account.fixed('loan'), 8, 'u64')),
step.let('available', expression.accountField(account.fixed('borrower'), 'lamports')),
step.invoke({
  program: account.fixed('protocolProgram'),
  accounts: repayAccounts,
  data: [
    data.literal(REPAY_DISCRIMINATOR),
    data.encode('u64', expression.min(expression.variable('owed'), expression.variable('available'))),
  ],
}),
```

```rust [Rust · run]
let run = ballista_sdk::run_instruction(template, repay_metas, &[]);
// Repays the debt as it stands, capped by what the borrower actually holds.
```

:::

`min` is the whole point: overpaying is rejected by most lending programs, and underpaying leaves
a position open.

<!-- benchmark:repay-exactly-what-is-owed -->

| Cost | Ballista | Plain instructions, not equivalent | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,252 | 150 | +3,102 |
| Transaction bytes, every run | 308 | 220 | +88 |
| Compute units, upload once | 4,407 | none | — |
| Transaction bytes, upload once | 464 in 1 transaction | none | — |
| Rent locked in the template account | 0.00201 SOL for 268 bytes | none | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. The protocol call is stood in by a System transfer, so neither row includes the protocol's own work. Debt accrues every slot, so the figure the client quoted is stale by the time the transaction lands. Repaying a fixed amount leaves a remainder or overpays.

<!-- /benchmark -->

## Split what arrived

A percentage of a balance that is still filling.

::: code-group

```ts [TypeScript · template]
step.let('distributable', expression.subtract(
  expression.accountField(account.fixed('vault'), 'lamports'),
  expression.input('reserve'),
)),
step.let('partnerShare', expression.divide(
  expression.multiply(expression.variable('distributable'), expression.input('shareBps')),
  expression.u64(10_000),
)),
systemTransfer({ /* vault → partner */ lamports: expression.variable('partnerShare') }),
systemTransfer({
  /* vault → treasury */
  lamports: expression.subtract(expression.variable('distributable'), expression.variable('partnerShare')),
}),
```

```rust [Rust · inputs]
let mut inputs = reserve.to_le_bytes().to_vec();
inputs.extend_from_slice(&share_bps.to_le_bytes());
let run = ballista_sdk::run_instruction(template, split_metas, &inputs);
```

:::

The second transfer takes the remainder rather than a second percentage, so integer division
cannot strand a lamport.

<!-- benchmark:split-what-arrived -->

| Cost | Ballista | Plain instructions, not equivalent | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 5,754 | 300 | +5,454 |
| Transaction bytes, every run | 324 | 270 | +54 |
| Compute units, upload once | 5,371 | none | — |
| Transaction bytes, upload once | 604 in 1 transaction | none | — |
| Rent locked in the template account | 0.00272 SOL for 408 bytes | none | — |

One Ballista instruction against 2 plain instructions, measured with Mollusk. The split is a percentage of a balance nobody can read until execution. Two transfers with client-computed amounts divide a number that has already changed.

<!-- /benchmark -->
