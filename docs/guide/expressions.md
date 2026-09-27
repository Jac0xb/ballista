# Inputs and expressions

This page covers the values a template works with: the inputs a caller passes, the expressions that
combine them, and the account data a template can read.

Every expression has one of six types: `bool`, `u64`, `i64`, `u128`, `pubkey`, or `bytes` with a
declared maximum length. Arithmetic and casts are checked: an overflow, a cast whose target type
cannot hold the value, or a division by zero fails the transaction.

## Named inputs

A template names each input and gives it a type. A `bytes` input also declares its maximum length.

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

Inputs are encoded one after another in declaration order, with no padding. Numbers are
little-endian: 8 bytes for `u64` and `i64`, 16 for `u128`. A `pubkey` is its 32 bytes, a `bool` is
one byte (0 or 1), and a `bytes` input is a little-endian `u16` length followed by the contents.

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
use ballista_sdk::RunInputs;

let data = RunInputs::new()
    .u64(25_000)
    .i64(1_800_000_000)
    .bool(true)
    .bytes(&route_data) // a u16 length, then the bytes
    .finish();
```

:::

## Checked math and booleans

Expressions combine inputs, account reads, and the clock with checked arithmetic, comparisons, and
the boolean operators `and`, `or`, and `not`. `min` and `max` return the smaller or larger of two
values, and `select` returns one of two values depending on a condition.

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

`expression.accountData(account, offset, type)` reads the value stored at a byte offset in an
account's data. The type sets the width: 8 bytes for `u64`, 32 for `pubkey`, 1 for `bool`, and so
on. Reads of `u8`, `u16`, and `u32` produce a `u64`. The offset is usually a number fixed in the
template, but it can also be a `u64` expression evaluated during the run.

To find a field's offset, add up the sizes of the fields before it in the program's account layout;
[amounts read at run time](/guide/runtime-values#forward-the-whole-token-balance) shows how. An
offset only means something in a known layout, so the compiler refuses to read an account's data
unless the account's declaration fixes its `owner` or its `address`, or explicitly
[opts out](/guide/trust-model#opting-out). For a fixed offset, the
compiler also raises the account's minimum data length so the read always fits, and a run rejects a
shorter account before any step runs. An offset computed during the run gets no such check: a read
past the end of the data fails the run.

## Values intentionally missing

There are no strings, maps, floating-point numbers, structs, or heap objects, and a template cannot
decode a protocol's account format during a run. Read the fields you need at their known offsets
instead. The [protocol templates](/examples/protocols/) read real protocol accounts this way.
