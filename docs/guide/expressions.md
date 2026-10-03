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

<<< @/../clients/rust/examples/docs_limits.rs#encode-run-inputs [Rust encoding]

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

## Prices and decimals

Token amounts are counted in a mint's base units, its smallest units: with 6 decimals, 1.5 tokens
is 1,500,000. Pricing an amount means multiplying by one number and dividing by another, and the
product can overflow even when the answer would fit.

`expression.multiplyDivide(a, b, divisor, rounding)` computes `a × b ÷ divisor`. The product
`a × b` is an intermediate value, used only on the way to the result, and it is held at twice the
inputs' width (256 bits for `u128` inputs), so it cannot overflow. Only the result has to fit.
`rounding` is `'down'`, the default, or `'up'`. The three values are all `u64` or all `u128`, and
the result has the same type.

`expression.powerOfTen(exponent)` is `10^exponent` as a `u128`, for a `u64` exponent of at most 38.
A larger exponent fails the run.

For example, the cost of `amount` base units at a `price` per whole token is
`amount × price ÷ 10^decimals`, rounded up so the buyer never pays less than the exact cost:

```ts
const decimals = expression.accountData(account.fixed('mint'), 44, 'u8'); // an SPL mint's decimals

// powerOfTen gives a u128, so the inputs are cast up to match, and the result back to a u64.
const cost = expression.cast(
  'u64',
  expression.multiplyDivide(
    expression.cast('u128', expression.input('amount')),
    expression.cast('u128', expression.input('price')),
    expression.powerOfTen(decimals),
    'up',
  ),
);
```

The [oracle-checked swap](/examples/protocols/jupiter-oracle-swap) does the same with a Pyth price:
it reads both mints' decimals and the price's exponent during the run, and computes its floor with
`powerOfTen` and `multiplyDivide`.

## Remainder, shifts and bitwise operations

- `remainder(a, b)` is what is left after dividing `a` by `b`, for `u64`, `i64` or `u128`. For an
  `i64` it has the sign of `a`. A zero `b` fails the run.
- `shiftLeft(a, bits)` is `a × 2^bits`, and `shiftRight(a, bits)` is `a ÷ 2^bits`, rounded down.
  `a` is a `u64` or `u128`, and `bits` is a `u64`. A left shift that would push a set bit out
  fails the run.
- `bitAnd`, `bitOr` and `bitXor` combine two `u64`s or two `u128`s bit by bit, for flags packed
  into one number. They are separate from the boolean `and` and `or`.

## Fixed-width account reads

```ts
const tokenAmount = expression.accountData(account.fixed('tokenAccount'), 64, 'u64');
const mint = expression.accountData(account.fixed('tokenAccount'), 0, 'pubkey');
const initialized = expression.accountData(account.fixed('stateAccount'), 8, 'bool');
```

`expression.accountData(account, offset, type)` reads the value stored at a byte offset in an
account's data. The type sets the width: 8 bytes for `u64`, 32 for `pubkey`, 1 for `bool`, and so
on. Reads of `u8`, `u16`, and `u32` produce a `u64`. An `i32` read produces an `i64` and is
sign-extended: widened with its sign kept, so four bytes that a `u32` read gives as 4,294,967,288
read as -8. Pyth stores its price exponent this way, and it is usually negative. The offset is
usually a number fixed in the template, but it can also be a `u64` expression evaluated during the
run.

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
