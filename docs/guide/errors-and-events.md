# Errors and events

This page explains how to read the error code of a failed run, how to find the step that failed,
and how to make a template log an event that records what each run did.

## Error code layout

Ballista reports failures as custom program errors, Solana's `Custom(u32)` error. The low 16 bits
hold the error kind. The high 16 bits hold a context number that locates the failure.

| Kind range | Source | Context |
| --- | --- | --- |
| 6000 to 6021 | Runtime: the program, while uploading or running a template | During a run, the program counter (the index of the failing instruction in the compiled template), except as noted below |
| 6100 to 6128 | Verifier: the checks that run when a template is created or finalized | The index of the offending instruction, account, input, register, or CPI (call to another program), where the error has one |

Four runtime kinds carry a different context:

| Kind | Name | Context |
| ---: | --- | --- |
| 6008 | `InvalidRunInputs` | Index of the value that could not be decoded, counting fixed values first and then row values. For leftover bytes after the last value, the number of values. For missing account-group length bytes, `0` |
| 6010 | `InvalidAccountRange` | The number of accounts, or of rows, that was out of range |
| 6020 | `AccountConstraintFailed` | Index of the account that did not meet its declaration, where `0` is the first account after the template account |
| 6021 | `CpiAccountLimitExceeded` | The CPI's total account count (declared accounts plus account-group members), which exceeded 64 |

A declared signer that did not sign fails with Solana's standard `MissingRequiredSignature` error,
not a Ballista code.

The full lists of names are in `fixtures/runtime-error-names.txt` and
`fixtures/verifier-error-names.txt` in the repository. The program, the Rust SDK, and the TypeScript
SDK are all tested against them.

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

`explainRunError` names the step that failed. It uses the compiled template's source map, which
records the step and optional label that produced each compiled instruction. For
`AccountConstraintFailed` it names the account instead, and for `InvalidRunInputs` the input. Give a
step a label when its position alone would not make a failure clear:

```ts
step.require(expression.lessThanOrEqual(total, budget), 'withinBudget');
```

Codes outside both ranges come from a program the template called, passed through unchanged. Both
decoders return nothing for them: `undefined` in TypeScript, `None` in Rust.

## The failure log line

When a run fails inside Ballista, the program writes one line to the transaction log before it
returns, using `sol_log_64`, which logs five numbers. For a failure in a step, they are the program
counter (the index of the failing instruction), the instruction's opcode (the number that identifies
its operation), its two operands `a` and `b` (the registers, or numbered value slots, that it
reads), and its destination register (the one it writes).

With that line you can find the failing instruction in a simulation's logs without decoding the
error. A step failure that carries its own context, such as `CpiAccountLimitExceeded`, logs the
error kind and that context in place of the operands. A failure found while checking accounts and
inputs, before the first step runs, logs `u64::MAX` in place of the program counter, followed by the
error kind and its context. The line is written only when a run fails, so successful runs pay
nothing for it.

## Run events

Set `emitEvent: true` on a template, and every successful run ends by logging one event with
`sol_log_data`. The event appears in the transaction's logs as a base64-encoded `Program data:`
line, where indexers can read it.

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 4 | The ASCII bytes `BEV1`, which identify the event |
| 4 | 1 | Template format version, currently 1 |
| 5 | 1 | Number of batch rows the run processed |
| 6 | 1 | Number of calls the run reached, counting each row's calls separately |
| 7 | 8 | Which reached calls actually ran, as a little-endian bit mask: bit `n` is set if reached call `n`, counting from 0, ran |
| 15 | 32 | Template address |

A call skipped by its `when` condition counts as reached, with its bit clear. So the event can tell
"the ATA (associated token account) already existed" apart from "the ATA was created" without
inspecting inner instructions. The event is off by default because it costs a few hundred compute
units, Solana's measure of execution cost.
