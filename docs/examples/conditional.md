# Work that should not always happen

A transaction cannot skip one of its own instructions. If a call would fail, the whole
transaction reverts: the fee is spent, nothing lands, and every other instruction batched
alongside it is lost too. Filtering beforehand only moves the problem, because the state can
change between the read and the block.

`when` attaches a condition to a single call. The condition is evaluated during execution, and a
false condition skips that call and nothing else.

## Claim only when there is something

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

| Cost | Ballista | Plain instructions, not equivalent | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,336 | 150 | +3,186 |
| Transaction bytes, every run | 308 | 220 | +88 |
| Compute units, upload once | 10,526 | none | — |
| Transaction bytes, upload once | 480 in 1 transaction | none | — |
| Rent locked in the template account | 0.00209 SOL for 284 bytes | none | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. The protocol call is stood in by a System transfer, so neither row includes the protocol's own work. Claiming nothing is an error in most protocols, and an error reverts the transaction. A keeper that guesses wrong pays the fee and lands nothing, including the work batched alongside it.

<!-- /benchmark -->

## Liquidate only when unhealthy

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

Every keeper watching the same position sends the same transaction. All but one of them are
wrong by the time the block is built, and the losers pay for a revert. Here the losers land a
no-op.

<!-- benchmark:liquidate-only-when-unhealthy -->

| Cost | Ballista | Plain instructions, not equivalent | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,435 | 150 | +3,285 |
| Transaction bytes, every run | 316 | 220 | +96 |
| Compute units, upload once | 10,571 | none | — |
| Transaction bytes, upload once | 484 in 1 transaction | none | — |
| Rent locked in the template account | 0.00211 SOL for 288 bytes | none | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. The protocol call is stood in by a System transfer, so neither row includes the protocol's own work. Health is read from the position at execution. A liquidation sent on a stale read reverts when someone else got there first, and reverts again for every keeper racing the same block.

<!-- /benchmark -->

## Top up only when low

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

| Cost | Ballista | Plain instructions, not equivalent | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,376 | 150 | +3,226 |
| Transaction bytes, every run | 291 | 220 | +71 |
| Compute units, upload once | 7,558 | none | — |
| Transaction bytes, upload once | 480 in 1 transaction | none | — |
| Rent locked in the template account | 0.00209 SOL for 284 bytes | none | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. A cron that tops up unconditionally drains the funder; one that checks first has read a balance that may have changed by the time the transfer lands.

<!-- /benchmark -->

## Initialize only if missing

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

A handful of programs ship an idempotent `Create`. For the rest, the caller has to know whether
the account exists and still be right about it when the block is built.

<!-- benchmark:initialize-only-if-missing -->

| Cost | Ballista | Plain instructions, not equivalent | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 2,956 | 150 | +2,806 |
| Transaction bytes, every run | 275 | 220 | +55 |
| Compute units, upload once | 8,747 | none | — |
| Transaction bytes, upload once | 440 in 1 transaction | none | — |
| Rent locked in the template account | 0.00189 SOL for 244 bytes | none | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. The protocol call is stood in by a System transfer, so neither row includes the protocol's own work. Only a handful of programs ship an idempotent Create. For the rest the caller must know whether the account exists, and be right about it at execution, or the whole transaction fails.

<!-- /benchmark -->
