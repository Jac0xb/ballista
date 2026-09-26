# Safety guardrails

Guardrails execute inside the same transaction as the operation they protect. If a requirement
fails, every earlier CPI in that transaction rolls back.

## Deadline and minimum output

Require both a live quote and acceptable output before invoking a client-built route.

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

The quote is still client-provided. For trust-minimized output protection, snapshot and read the
actual destination token-account amount after the CPI.

<!-- benchmark:deadline-and-minimum-output -->

| Cost | Ballista | Plain instructions, weaker | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 4,064 | 150 | +3,914 |
| Transaction bytes, every run | 333 | 240 | +93 |
| Compute units, upload once | 8,079 | none | — |
| Transaction bytes, upload once | 556 in 1 transaction | none | — |
| Rent locked in the template account | 0.00248 SOL for 360 bytes | none | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. The protocol call is stood in by a System transfer, so neither row includes the protocol's own work. The route instruction alone. Deadline and minimum output are whatever the client checked before signing. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->

## Pinned program and owner

Pin executable program IDs and protocol account owners in the account schema.

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

```rust [Rust · caller check]
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

<!-- benchmark:pinned-program-and-owner -->

| Cost | Ballista | Plain instructions, weaker | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 2,959 | 150 | +2,809 |
| Transaction bytes, every run | 283 | 220 | +63 |
| Compute units, upload once | 4,158 | none | — |
| Transaction bytes, upload once | 428 in 1 transaction | none | — |
| Rent locked in the template account | 0.00183 SOL for 232 bytes | none | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. The protocol call is stood in by a System transfer, so neither row includes the protocol's own work. The same instruction with no schema: a substituted program ID or a wrong-owner account is accepted as far as the transaction is concerned. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->

## Oracle price band

Read a fixed-width price field from a schema-pinned oracle account and enforce a band.

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
The template must use the oracle protocol's documented field layout and should also check its
published slot or timestamp. Ballista has no built-in oracle evaluator.
:::

<!-- benchmark:oracle-price-band -->

| Cost | Ballista | Plain instructions, not equivalent | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 4,081 | 150 | +3,931 |
| Transaction bytes, every run | 324 | 220 | +104 |
| Compute units, upload once | 5,134 | none | — |
| Transaction bytes, upload once | 568 in 1 transaction | none | — |
| Rent locked in the template account | 0.00254 SOL for 372 bytes | none | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. The protocol call is stood in by a System transfer, so neither row includes the protocol's own work. No instruction sequence reads an oracle account and refuses to continue. Enforcing a band on chain needs a program.

<!-- /benchmark -->

## Maximum lamport spend

Snapshot a signer before arbitrary CPIs and cap the actual debit afterward.

::: code-group

```ts [TypeScript · invariant]
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

<!-- benchmark:maximum-lamport-spend -->

| Cost | Ballista | Plain instructions, not equivalent | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,624 | 150 | +3,474 |
| Transaction bytes, every run | 283 | 220 | +63 |
| Compute units, upload once | 4,814 | none | — |
| Transaction bytes, upload once | 524 in 1 transaction | none | — |
| Rent locked in the template account | 0.00232 SOL for 328 bytes | none | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. The protocol call is stood in by a System transfer, so neither row includes the protocol's own work. A transaction cannot compare a balance before and after one of its own instructions. Capping the debit on chain needs a program.

<!-- /benchmark -->

## Canonical position account

Prevent a caller from substituting a different protocol-owned account with otherwise valid layout.

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

```rust [Rust · bind]
let (position, _) = Pubkey::find_program_address(
    &[b"position", owner.as_ref(), &position_id.to_le_bytes()],
    &protocol_program,
);
let run = ballista_sdk::run_instruction(template, position_metas, &position_id.to_le_bytes());
```

:::

<!-- benchmark:canonical-position-account -->

| Cost | Ballista | Plain instructions, not equivalent | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 5,630 | 150 | +5,480 |
| Transaction bytes, every run | 285 | 220 | +65 |
| Compute units, upload once | 11,109 | none | — |
| Transaction bytes, upload once | 572 in 1 transaction | none | — |
| Rent locked in the template account | 0.00256 SOL for 376 bytes | none | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. The protocol call is stood in by a System transfer, so neither row includes the protocol's own work. A transaction cannot derive a PDA and compare it to a supplied account. Rejecting a substituted account on chain needs a program.

<!-- /benchmark -->
