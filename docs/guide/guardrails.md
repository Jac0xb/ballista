# Safety guardrails

A guardrail is a check that runs in the same transaction as the operation it protects. This page
shows five: a swap deadline and minimum output, a fixed program address and account owner, an
oracle price band, a cap on how much SOL the payer can spend, and a check that an account is the
one a program would derive.

If a guardrail fails, the whole transaction fails, and every CPI (cross-program invocation: a call
from the template into another program) made before it is rolled back. Most of the examples use
`step.require`, which stops the transaction unless its condition holds.

::: info How to read the cost tables
Each example ends with measured costs. Compute units measure on-chain work. The Ballista column is
one run of the template. The plain column is the closest sequence of ordinary instructions, labeled
one of two ways. "Weaker checks" means the same call can be sent without Ballista, but the check is
not enforced on chain. "Closest plain instructions" means no sequence of ordinary instructions can
make the check at all; without Ballista it would take a program of your own. The "upload once" and
rent rows are one-time costs of storing the template on chain. Every example here calls another
protocol, so the measurement calls a SOL transfer in its place, and neither column includes that
protocol's own work. The figures come from Mollusk, a harness that runs Solana programs without a
validator. The [benchmarks page](/benchmarks) describes the method.
:::

## Deadline and minimum output

Before calling a swap program with route data the client built, check that the quote has not
expired and that it promises at least the minimum output.

::: code-group

```ts [TypeScript · template]
const quoteIsValid = expression.and(
  expression.lessThanOrEqual(expression.clockUnixTimestamp(), expression.input('deadline')),
  expression.greaterThanOrEqual(expression.input('quotedOut'), expression.input('minimumOut')),
);

steps: [
  step.require(quoteIsValid),
  step.invoke({
    program: account.fixed('swapProgram'),
    accounts: swapAccounts,
    data: [data.encode('bytes', expression.input('routeData'))],
  }),
]
```

```rust [Rust · inputs]
let mut inputs = Vec::new();
inputs.extend_from_slice(&deadline.to_le_bytes());
inputs.extend_from_slice(&quoted_out.to_le_bytes());
inputs.extend_from_slice(&minimum_out.to_le_bytes());
inputs.extend_from_slice(&(route_data.len() as u16).to_le_bytes());
inputs.extend_from_slice(&route_data);
let run = ballista_sdk::run_instruction(template, swap_metas, &inputs);
```

:::

`clockUnixTimestamp` is the network's clock when the transaction executes. The quoted and minimum
outputs are both inputs from the caller, so this check only enforces what the client claims. To
protect the output without trusting the caller, record the destination token account's balance
with `step.snapshot` before the swap, and check how much it grew after the swap. [Assertions and
snapshots](/guide/assertions) shows how.

<!-- benchmark:deadline-and-minimum-output -->

| Cost | Ballista | Plain instructions, weaker checks | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 4,064 | 150 | +3,914 |
| Transaction bytes, every run | 333 | 240 | +93 |
| Compute units, upload once | 8,079 | none | — |
| Transaction bytes, upload once | 556 in 1 transaction | none | — |
| Rent locked in the template account | 0.00248 SOL for 360 bytes | none | — |

One Ballista instruction, compared with 1 plain instruction. A SOL transfer stands in for the protocol call, so neither column includes the protocol's own work. The swap instruction alone. The deadline and minimum output are only what the client checked before signing. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->

## Pinned program and owner

Fix the address of each program the template calls, and the owner of each protocol account, in
the template's account schema: its list of accounts and the rules each must meet.

::: code-group

```ts [TypeScript · schema]
accounts: {
  protocolProgram: { executable: true, address: PROTOCOL_PROGRAM_BYTES },
  position: {
    writable: true,
    owner: PROTOCOL_PROGRAM_BYTES,
    minDataLength: 128,
  },
}
```

```rust [Rust · run]
let run = ballista_sdk::run_instruction(
    template,
    vec![
        AccountMeta::new_readonly(PROTOCOL_PROGRAM_ID, false),
        AccountMeta::new(position, false),
    ],
    &inputs,
);
// Ballista rejects substituted program IDs and wrong account owners before execution.
```

:::

`executable: true` with an `address` means the account must be that exact program. `owner` means
the account must belong to the given program, `minDataLength` means it must hold at least that
many bytes of data, and `writable` means the transaction may change it. If any account breaks its
rules, the run fails before the first step, so a caller cannot swap in a different program or a
look-alike account.

<!-- benchmark:pinned-program-and-owner -->

| Cost | Ballista | Plain instructions, weaker checks | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 2,959 | 150 | +2,809 |
| Transaction bytes, every run | 283 | 220 | +63 |
| Compute units, upload once | 4,158 | none | — |
| Transaction bytes, upload once | 428 in 1 transaction | none | — |
| Rent locked in the template account | 0.00183 SOL for 232 bytes | none | — |

One Ballista instruction, compared with 1 plain instruction. A SOL transfer stands in for the protocol call, so neither column includes the protocol's own work. The same instruction without the template's account checks: the transaction itself accepts a different program or an account with the wrong owner. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->

## Oracle price band

Read a price from an oracle account (an account in which a price feed publishes prices) and stop
the run unless the price lies between a minimum and a maximum. `PRICE_OFFSET` stands for the byte
offset of the price in the oracle's account layout. Fix the oracle account's address or owner in
the account schema, as in the previous example, so a caller cannot pass a fake oracle.

::: code-group

```ts [TypeScript · template]
const price = expression.accountData(account.fixed('oracle'), PRICE_OFFSET, 'i64');

steps: [step.require(expression.and(
  expression.greaterThanOrEqual(price, expression.input('minimumPrice')),
  expression.lessThanOrEqual(price, expression.input('maximumPrice')),
))]
```

```rust [Rust · inputs]
let mut inputs = Vec::new();
inputs.extend_from_slice(&minimum_price.to_le_bytes());
inputs.extend_from_slice(&maximum_price.to_le_bytes());
let run = ballista_sdk::run_instruction(template, oracle_and_protocol_metas, &inputs);
```

:::

::: warning Layout and freshness
Ballista does not understand oracle formats. The template must read the price at the offset the
oracle protocol documents, and it should also check the oracle's publish slot or timestamp so that
a stale price is rejected.
:::

<!-- benchmark:oracle-price-band -->

| Cost | Ballista | Closest plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 4,081 | 150 | +3,931 |
| Transaction bytes, every run | 324 | 220 | +104 |
| Compute units, upload once | 5,134 | none | — |
| Transaction bytes, upload once | 568 in 1 transaction | none | — |
| Rent locked in the template account | 0.00254 SOL for 372 bytes | none | — |

One Ballista instruction, compared with 1 plain instruction. A SOL transfer stands in for the protocol call, so neither column includes the protocol's own work. No instruction sequence reads an oracle account and refuses to continue. Enforcing a band on chain needs a program.

<!-- /benchmark -->

## Maximum lamport spend

Record a signer's balance in lamports (the smallest unit of SOL) before calling another program,
and fail the whole transaction if the balance dropped by more than a limit. A signer is an account
that signed the transaction; here it is the payer.

::: code-group

```ts [TypeScript · template]
step.snapshot('before', expression.accountField(account.fixed('payer'), 'lamports')),
step.invoke({ /* client-selected protocol instruction */ }),
step.require(expression.lessThanOrEqual(
  expression.subtract(
    expression.snapshot('before'),
    expression.accountField(account.fixed('payer'), 'lamports'),
  ),
  expression.input('maximumSpend'),
)),
```

```rust [Rust · run]
let run = ballista_sdk::run_instruction(
    template,
    protected_transaction_metas,
    &maximum_spend.to_le_bytes(),
);
```

:::

`step.snapshot` saves the payer's balance before the call. After the call, the template reads the
balance again and requires that it dropped by at most `maximumSpend`. Because the check runs after
the call, it limits what the call actually spent, whatever instruction the client chose.
Ballista's subtraction fails instead of going below zero, so the run also fails if the call leaves
the payer with more lamports than before.

<!-- benchmark:maximum-lamport-spend -->

| Cost | Ballista | Closest plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,624 | 150 | +3,474 |
| Transaction bytes, every run | 283 | 220 | +63 |
| Compute units, upload once | 4,814 | none | — |
| Transaction bytes, upload once | 524 in 1 transaction | none | — |
| Rent locked in the template account | 0.00232 SOL for 328 bytes | none | — |

One Ballista instruction, compared with 1 plain instruction. A SOL transfer stands in for the protocol call, so neither column includes the protocol's own work. A transaction cannot compare a balance before and after one of its own instructions. Capping the debit on chain needs a program.

<!-- /benchmark -->

## Canonical position account

Check that the position account the caller passed is the one the protocol derives for this owner
and position ID. An owner check alone would accept any account the protocol owns that has a valid
layout, including another position.

::: code-group

```ts [TypeScript · template]
assertPda({
  account: account.fixed('position'),
  program: account.fixed('protocolProgram'),
  seeds: [
    expression.bytes(new TextEncoder().encode('position')),
    expression.accountField(account.fixed('owner'), 'key'),
    expression.input('positionId'),
  ],
});
```

```rust [Rust · run]
let (position, _) = Pubkey::find_program_address(
    &[b"position", owner.as_ref(), &position_id.to_le_bytes()],
    &protocol_program,
);
let run = ballista_sdk::run_instruction(template, position_metas, &position_id.to_le_bytes());
```

:::

`assertPda` computes a PDA (program derived address: an address derived from a program ID and a
list of seeds, with no private key) from the protocol program and the seeds. If `position` has a
different address, the run fails. The derivation searches for the bump (an extra seed byte that
makes the result a valid PDA), so its compute cost varies. The caller derives the same address in
Rust to build the account list. The check only proves how the accounts relate; it does not let
Ballista sign for the PDA.

<!-- benchmark:canonical-position-account -->

| Cost | Ballista | Closest plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 5,630 | 150 | +5,480 |
| Transaction bytes, every run | 285 | 220 | +65 |
| Compute units, upload once | 11,109 | none | — |
| Transaction bytes, upload once | 572 in 1 transaction | none | — |
| Rent locked in the template account | 0.00256 SOL for 376 bytes | none | — |

One Ballista instruction, compared with 1 plain instruction. A SOL transfer stands in for the protocol call, so neither column includes the protocol's own work. A transaction cannot derive a PDA and compare it to a supplied account. Rejecting a substituted account on chain needs a program.

<!-- /benchmark -->
