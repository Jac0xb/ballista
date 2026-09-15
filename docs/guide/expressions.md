# Inputs and expressions

Expressions produce one of six VM types: `bool`, `u64`, `i64`, `u128`, `pubkey`, or bounded
`bytes`. Arithmetic and casts are checked; overflow, invalid narrowing, and division by zero fail
the transaction.

## Named inputs

```ts
const template = defineTemplate({
  inputs: {
    amount: { type: 'u64' },
    deadline: { type: 'i64' },
    enabled: { type: 'bool' },
    routeData: { type: 'bytes', maxLength: 512 },
  },
  // ...
});
```

Inputs are encoded in deterministic declaration order. Byte inputs carry a little-endian `u16`
length followed by their contents.

::: code-group

```ts [TypeScript encoding]
const data = encodeRunInputs(compiled, {
  amount: 25_000n,
  deadline: 1_800_000_000n,
  enabled: true,
  routeData: quote.swapInstructionData,
});
```

```rust [Rust encoding]
let mut data = Vec::new();
data.extend_from_slice(&25_000u64.to_le_bytes());
data.extend_from_slice(&1_800_000_000i64.to_le_bytes());
data.push(1); // true
data.extend_from_slice(&(route_data.len() as u16).to_le_bytes());
data.extend_from_slice(&route_data);
```

:::

## Checked math and booleans

```ts
const acceptable = expression.and(
  expression.greaterThanOrEqual(expression.input('received'), expression.input('minimum')),
  expression.lessThanOrEqual(expression.clockUnixTimestamp(), expression.input('deadline')),
);

const capped = expression.min(
  expression.input('requested'),
  expression.input('available'),
);

const chosen = expression.select(
  expression.input('useFallback'),
  expression.input('fallbackAmount'),
  expression.input('primaryAmount'),
);
```

## Fixed-width account reads

```ts
const tokenAmount = expression.accountData(account.fixed('tokenAccount'), 64, 'u64');
const mint = expression.accountData(account.fixed('tokenAccount'), 0, 'pubkey');
const initialized = expression.accountData(account.fixed('stateAccount'), 8, 'bool');
```

Account offsets and widths are fixed in the template. The account schema should additionally pin
the expected owner and minimum data length.

## Values intentionally missing

There are no strings, maps, floating-point numbers, arbitrary structs, heap objects, or dynamically
decoded protocol accounts. Read fixed fields and let SDK helpers map well-known layouts.
