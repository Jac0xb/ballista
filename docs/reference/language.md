# Language surface

## Values

| Type | Width or bound | Operations |
| --- | ---: | --- |
| `bool` | 1 byte | `AND`, `OR`, `NOT`, equality, select condition |
| `u64` | 8 bytes | checked math, comparisons, casts, encoding |
| `i64` | 8 bytes | checked math, comparisons, casts, encoding |
| `u128` | 16 bytes | checked math, comparisons, casts, encoding |
| `pubkey` | 32 bytes | equality, CPI encoding, PDA seed |
| `bytes` | declared maximum | equality, whole-input CPI insertion, PDA seed up to 32 bytes |

## Sources

- Typed execution inputs and literals.
- Account key, owner, lamports, data length, and emptiness.
- Fixed-offset account data reads: bool, u8, u16, u32, u64, i64, u128, and pubkey. The account
  must pin an owner or address, and the read must fit the declared minimum data length.
- Dynamic-offset account data reads, where a `u64` expression supplies the offset at run time.
- Return data of the invoke immediately before the current step, read as any fixed width.
- Clock slot and Unix timestamp.
- Current loop index.
- Canonical PDA derivation.

## Computation

- Checked add, subtract, multiply, and divide.
- `min`, `max`, checked numeric casts, and typed `select`.
- Equality/inequality plus ordered numeric comparisons.
- Boolean `AND`, `OR`, and `NOT`.
- Lexical `let` and `snapshot` bindings.
- Loop-carried variables: a variable defined before the loop and listed in `carry` can be
  reassigned inside the body with `assign`, keeps its type, and is readable after the loop.

## Effects

- `require(bool)`.
- Guarded generic CPI, targeting a pinned program.
- One top-level bounded `forEach` over inferred account rows, with a minimum row count.
- An opt-in run event logged after success.

## Deliberately absent

- Backward jumps, recursion, nested loops, and `while`.
- Dynamic account discovery or RPC access.
- Strings, floating point, maps, and dynamic protocol deserialization.
- Mutable template state or cross-transaction variables.
- Ballista PDA signing, custody, scheduling, replay policy, or keeper incentives.
