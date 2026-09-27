# Amounts read at run time

A Solana transaction fixes its instruction data when it is signed, so every amount in it must be
known in advance. Many useful amounts are not: the balance to sweep, the debt to repay, the part of
a deposit to pass on. They exist only when the transaction executes.

A Ballista template can read a number from an account while it runs and pass it to the next call.
This page shows four ways to use that: sweep a balance, forward a token balance, repay a debt, and
split a deposit.

In the examples, `step.let` computes a value once and gives it a name, and `step.require` stops
the whole transaction unless its condition holds. `systemTransfer` and `tokenTransfer` call the
System program and the Token program. The TypeScript tab shows the template's steps. The Rust tab
shows how a caller builds the instruction that runs the template, from the template's address, the
accounts, and the inputs encoded as little-endian bytes in the order the template declares them.

::: info How to read the cost tables
Each example ends with measured costs. Compute units measure on-chain work. The Ballista column
is one run of the template. The plain column is the closest sequence of ordinary instructions. It
does not do the same job, because its amounts are fixed at signing. The "upload once" and rent
rows are one-time costs of storing the template on chain. Where an example calls another
protocol, the measurement calls a SOL transfer in its place, so neither column includes that
protocol's own work. The figures come from Mollusk, a harness that runs Solana programs without a
validator. The [benchmarks page](/benchmarks) describes the method.
:::

## Sweep above a reserve

Move everything above a minimum balance (the reserve) from a vault to a destination, whatever the
balance is when the transaction executes.

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

The template reads the vault's balance in lamports (the smallest unit of SOL) and transfers
whatever is above the reserve. The caller passes only the reserve. If the balance is not above
the reserve, the `require` stops the run before the subtraction.

<!-- benchmark:sweep-above-a-reserve -->

| Cost | Ballista | Closest plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,437 | 150 | +3,287 |
| Transaction bytes, every run | 283 | 220 | +63 |
| Compute units, upload once | 4,615 | none | — |
| Transaction bytes, upload once | 492 in 1 transaction | none | — |
| Rent locked in the template account | 0.00215 SOL for 296 bytes | none | — |

One Ballista instruction, compared with 1 plain instruction. The balance at execution is not known when the transaction is signed. A fixed amount either leaves dust behind or overdraws and fails, and anything that arrives between signing and execution is stranded.

<!-- /benchmark -->

## Forward the whole token balance

Move a token account's entire balance to another token account.

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

`accountData` reads a value of the given type at a byte offset in an account's data. An SPL Token
account stores its balance as a `u64` at offset 64, so the template reads it there. That read is
only meaningful if the account really is a token account. The template's account schema (its list
of accounts and the rules each must meet, not shown above) therefore requires both accounts to be
owned by the Token program and to hold at least 165 bytes of data. The `require` stops the run
when the balance is zero.

<!-- benchmark:forward-the-whole-token-balance -->

| Cost | Ballista | Closest plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,350 | 76 | +3,274 |
| Transaction bytes, every run | 308 | 250 | +58 |
| Compute units, upload once | 4,520 | none | — |
| Transaction bytes, upload once | 479 in 1 transaction | none | — |
| Rent locked in the template account | 0.00209 SOL for 283 bytes | none | — |

One Ballista instruction, compared with 1 plain instruction. A Transfer carries a fixed amount. Emptying an account whose balance is still moving — fees accruing, a swap landing — needs the figure read during execution.

<!-- /benchmark -->

## Repay exactly what is owed

Repay a loan in full, or with everything the borrower holds if that is less. The debt grows with
every slot (the interval in which Solana produces a block), so an amount the client computes
before signing is already out of date when the transaction executes.

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

The template reads the debt from the loan account (a `u64` at byte offset 8 in this example's
layout) and the borrower's balance, then pays the smaller of the two. `step.invoke` makes a CPI (a
cross-program invocation: one program calling another) into the lending program. `data.literal`
writes fixed bytes, here the discriminator that selects the lending program's repay instruction,
and `data.encode` appends the amount as a `u64`.

A fixed amount would be wrong in one direction or the other: most lending programs reject an
overpayment, and an underpayment leaves the loan open. `min` never pays more than is owed, and it
pays the whole debt whenever the borrower can cover it.

<!-- benchmark:repay-exactly-what-is-owed -->

| Cost | Ballista | Closest plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,252 | 150 | +3,102 |
| Transaction bytes, every run | 308 | 220 | +88 |
| Compute units, upload once | 4,407 | none | — |
| Transaction bytes, upload once | 464 in 1 transaction | none | — |
| Rent locked in the template account | 0.00201 SOL for 268 bytes | none | — |

One Ballista instruction, compared with 1 plain instruction. A SOL transfer stands in for the protocol call, so neither column includes the protocol's own work. Debt accrues every slot, so the figure the client quoted is stale by the time the transaction lands. Repaying a fixed amount leaves a remainder or overpays.

<!-- /benchmark -->

## Split what arrived

Send a percentage of a vault's balance above a reserve to a partner, and the rest to a treasury.
The balance keeps changing as deposits arrive, so the split is computed when the transaction
executes.

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

`shareBps` is the partner's share in basis points (hundredths of a percent, so 10,000 is 100%).
Integer division rounds the partner's share down. The treasury receives the remainder rather than
a second percentage, so rounding never leaves a lamport of the distributable amount behind.

<!-- benchmark:split-what-arrived -->

| Cost | Ballista | Closest plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 5,754 | 300 | +5,454 |
| Transaction bytes, every run | 324 | 270 | +54 |
| Compute units, upload once | 5,371 | none | — |
| Transaction bytes, upload once | 604 in 1 transaction | none | — |
| Rent locked in the template account | 0.00272 SOL for 408 bytes | none | — |

One Ballista instruction, compared with 2 plain instructions. The split is a percentage of a balance nobody can read until execution. Two transfers with client-computed amounts divide a number that has already changed.

<!-- /benchmark -->
