# Conditional calls

A Solana transaction cannot skip one of its own instructions. If one call fails, the whole
transaction fails: the fee is still paid, nothing takes effect, and every other instruction in the
transaction is lost with it. Checking the state before sending does not solve this, because the
state can change between the check and the moment the transaction executes.

A Ballista template can attach a condition to a single call with `when`. The template evaluates
the condition while it runs. If the condition is false, the template skips that call and carries
on with the next step.

This page shows four examples: claim rewards only when some are pending, liquidate a position only
when it is unhealthy, top up a balance only when it is low, and create an account only if it does
not exist yet. In the code, `step.invoke` makes a CPI (a cross-program invocation: one program
calling another), and `systemTransfer` is a shortcut for a CPI to the System program. Both accept
`when`.

::: info How to read the cost tables
Each example ends with measured costs. Compute units measure on-chain work. The Ballista column
is one run of the template. The plain column is the same call sent as an ordinary instruction. It
does not do the same job: it cannot check the condition during execution, so it always makes the
call. The "upload once" and rent rows are one-time costs of storing the template on chain. Where
an example calls another protocol, the measurement calls a SOL transfer in its place, so neither
column includes that protocol's own work. The figures come from Mollusk, a harness that runs
Solana programs without a validator. The [benchmarks page](/benchmarks) describes the method.
:::

## Claim only when there is something

Call a protocol's claim instruction only when its rewards account shows a pending amount above
zero. `PENDING_OFFSET` stands for the byte offset of that amount in the rewards account. A keeper
(a bot that sends routine transactions for a protocol) can send this on a schedule without
checking first.

::: code-group

```ts [TypeScript · template]
step.invoke({
  program: account.fixed('protocolProgram'),
  accounts: claimAccounts,
  data: [data.literal(CLAIM_DISCRIMINATOR)],
  when: expression.greaterThan(
    expression.accountData(account.fixed('rewards'), PENDING_OFFSET, 'u64'),
    expression.u64(0),
  ),
})
```

```rust [Rust · run]
// Safe to run on a schedule: an empty epoch is a no-op, not a failure.
let run = ballista_sdk::run_instruction(template, claim_metas, &[]);
```

:::

<!-- benchmark:claim-only-when-there-is-something -->

| Cost | Ballista | Closest plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,336 | 150 | +3,186 |
| Transaction bytes, every run | 308 | 220 | +88 |
| Compute units, upload once | 10,526 | none | — |
| Transaction bytes, upload once | 480 in 1 transaction | none | — |
| Rent locked in the template account | 0.00209 SOL for 284 bytes | none | — |

One Ballista instruction, compared with 1 plain instruction. A SOL transfer stands in for the protocol call, so neither column includes the protocol's own work. Claiming nothing is an error in most protocols, and an error reverts the transaction. A caller that guesses wrong pays the fee and gets nothing done, including the other work in the same transaction.

<!-- /benchmark -->

## Liquidate only when unhealthy

Liquidate a lending position only when its health value is below a threshold the caller passes.
The template reads the health value from the position account at `HEALTH_OFFSET`.

::: code-group

```ts [TypeScript · template]
step.invoke({
  program: account.fixed('protocolProgram'),
  accounts: liquidateAccounts,
  data: liquidateData,
  when: expression.lessThan(
    expression.accountData(account.fixed('position'), HEALTH_OFFSET, 'u64'),
    expression.input('threshold'),
  ),
})
```

```rust [Rust · run]
let run = ballista_sdk::run_instruction(template, position_metas, &threshold.to_le_bytes());
```

:::

Keepers that watch lending positions often send the same liquidation for the same position. Only
the first can succeed. The others execute after the position has already changed. With a plain
liquidation instruction, each of those transactions fails. With `when`, they succeed and skip the
call, so any other work in them still takes effect.

<!-- benchmark:liquidate-only-when-unhealthy -->

| Cost | Ballista | Closest plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,435 | 150 | +3,285 |
| Transaction bytes, every run | 316 | 220 | +96 |
| Compute units, upload once | 10,571 | none | — |
| Transaction bytes, upload once | 484 in 1 transaction | none | — |
| Rent locked in the template account | 0.00211 SOL for 288 bytes | none | — |

One Ballista instruction, compared with 1 plain instruction. A SOL transfer stands in for the protocol call, so neither column includes the protocol's own work. Health is read from the position at execution. A liquidation sent on a stale read reverts when someone else got there first, and again for every other liquidator trying the same position.

<!-- /benchmark -->

## Top up only when low

Send `topUp` lamports (the smallest unit of SOL) from a funder to a bot's account, but only when
the bot's balance is below `floor`.

::: code-group

```ts [TypeScript · template]
systemTransfer({
  systemProgram: account.fixed('systemProgram'),
  from: account.fixed('funder'),
  to: account.fixed('bot'),
  lamports: expression.input('topUp'),
  when: expression.lessThan(
    expression.accountField(account.fixed('bot'), 'lamports'),
    expression.input('floor'),
  ),
})
```

```rust [Rust · inputs]
let mut inputs = floor.to_le_bytes().to_vec();
inputs.extend_from_slice(&top_up.to_le_bytes());
let run = ballista_sdk::run_instruction(template, top_up_metas, &inputs);
```

:::

<!-- benchmark:top-up-only-when-low -->

| Cost | Ballista | Closest plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,376 | 150 | +3,226 |
| Transaction bytes, every run | 291 | 220 | +71 |
| Compute units, upload once | 7,558 | none | — |
| Transaction bytes, upload once | 480 in 1 transaction | none | — |
| Rent locked in the template account | 0.00209 SOL for 284 bytes | none | — |

One Ballista instruction, compared with 1 plain instruction. A scheduled job that always tops up drains the funder; one that checks first has read a balance that may have changed by the time the transfer lands.

<!-- /benchmark -->

## Initialize only if missing

Create an account only if it does not exist yet.

::: code-group

```ts [TypeScript · template]
step.invoke({
  program: account.fixed('protocolProgram'),
  accounts: initializeAccounts,
  data: initializeData,
  when: expression.accountField(account.fixed('position'), 'isEmpty'),
})
```

```rust [Rust · run]
// The same instruction whether or not the position exists yet.
let run = ballista_sdk::run_instruction(template, position_metas, &[]);
```

:::

`isEmpty` is true when the account holds no data, as it does before it is created. A few programs
offer an idempotent `Create`, one that succeeds even when the account already exists. For the
others, the caller must know whether the account exists, and still be right when the transaction
executes.

<!-- benchmark:initialize-only-if-missing -->

| Cost | Ballista | Closest plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 2,956 | 150 | +2,806 |
| Transaction bytes, every run | 275 | 220 | +55 |
| Compute units, upload once | 8,747 | none | — |
| Transaction bytes, upload once | 440 in 1 transaction | none | — |
| Rent locked in the template account | 0.00189 SOL for 244 bytes | none | — |

One Ballista instruction, compared with 1 plain instruction. A SOL transfer stands in for the protocol call, so neither column includes the protocol's own work. Only a few programs have an instruction that creates an account only if it is missing. For the rest the caller must know whether the account exists, and be right about it at execution, or the whole transaction fails.

<!-- /benchmark -->
