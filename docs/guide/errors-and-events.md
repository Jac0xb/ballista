# Errors and events

Every failure a run produces says where it happened, and templates can opt into a compact event
that says what ran.

## Error code layout

Ballista reports failures as custom program errors. The low 16 bits are the error kind; the high 16
bits are a context that locates the failure.

| Kind range | Source | Context |
| --- | --- | --- |
| 6000 to 6020 | Runtime | Program counter of the failing instruction, except as noted below |
| 6100 to 6127 | Verifier, at create or finalize | Offending instruction, account, input, register, or CPI index |

Three runtime kinds carry a different context:

| Kind | Name | Context |
| ---: | --- | --- |
| 6008 | `InvalidRunInputs` | Index of the input that failed to decode, or the input count for trailing bytes |
| 6010 | `InvalidAccountRange` | The runtime account or row count that was rejected |
| 6020 | `AccountConstraintFailed` | Index of the runtime account that failed its constraint |

The full name tables live in `fixtures/runtime-error-names.txt` and
`fixtures/verifier-error-names.txt`; the program, the Rust SDK, and the TypeScript SDK are all
tested against them.

## Decoding

::: code-group

```ts [TypeScript]
import { decodeBallistaError, explainRunError } from '@jac0xb/ballista';

decodeBallistaError((7 << 16) | 6015);
// { kind: 6015, name: 'RequirementFailed', context: 7, source: 'runtime' }

explainRunError((7 << 16) | 6015, compiled).message;
// 'RequirementFailed at steps[2] (withinBudget)'
```

```rust [Rust]
use ballista_sdk::decode_ballista_error;

let decoded = decode_ballista_error((7 << 16) | 6015).unwrap();
assert_eq!(decoded.name, "RequirementFailed");
assert_eq!(decoded.context, 7);
```

:::

`explainRunError` uses the compiled template's source map, which records the authoring step and
optional label for every emitted instruction. Give steps labels where a failure would be hard to
place otherwise:

```ts
step.require(expression.lessThanOrEqual(total, budget), 'withinBudget');
```

Codes outside both ranges belong to an invoked program and were passed through unchanged; both
decoders return nothing for them.

## The failure log line

When a VM failure occurs the program logs one `sol_log_64` line before returning: the program
counter, the opcode, operands `a` and `b`, and the destination register. It costs nothing on the
success path and lets you read the failing instruction straight from a simulation without decoding
the template.

## Run events

Set `emitEvent: true` on a template and every successful run ends with one `sol_log_data` entry.
Indexers read it from the transaction's `Program data:` log line.

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 4 | Magic `BEV1` |
| 4 | 1 | Bytecode version |
| 5 | 1 | Iterations executed |
| 6 | 1 | Invoke instructions reached, counted across iterations |
| 7 | 8 | Bitmask of which reached invokes actually ran, little-endian |
| 15 | 32 | Template address |

A guarded CPI that was skipped appears as a reached invoke whose bit is clear, so the event
distinguishes "the ATA already existed" from "the ATA was created" without inspecting inner
instructions. The event is opt-in because it costs a few hundred compute units.
