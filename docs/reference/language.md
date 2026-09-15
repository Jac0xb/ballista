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
- Fixed-width account data reads: bool, u8, u16, u32, u64, i64, u128, and pubkey.
- Clock slot and Unix timestamp.
- Current loop index.
- Canonical PDA derivation.

## Computation

- Checked add, subtract, multiply, and divide.
- `min`, `max`, checked numeric casts, and typed `select`.
- Equality/inequality plus ordered numeric comparisons.
- Boolean `AND`, `OR`, and `NOT`.
- Lexical `let` and `snapshot` bindings.

## Effects

- `require(bool)`.
- Guarded generic CPI.
- One top-level bounded `forEach` over inferred account rows.

## Deliberately absent

- Backward jumps, recursion, nested loops, and `while`.
- Dynamic account discovery or RPC access.
- Strings, floating point, maps, and dynamic protocol deserialization.
- Mutable template state or cross-transaction variables.
- Ballista PDA signing, custody, scheduling, replay policy, or keeper incentives.
