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
