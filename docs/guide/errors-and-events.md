# Errors and events

How to read a failed run's error code, find the step that failed, and get data out of a run: the
run event, logs of the template's own, and return data.

## Error code layout

Ballista reports failures as Solana custom program errors (`Custom(u32)`). The low 16 bits hold
the error kind; the high 16 bits hold a context number that locates the failure.

```text
code = kind | (context << 16)
```

- **6000 to 6026** (`0x1770` to `0x178A`) are runtime errors, raised while uploading or running.
  The context is usually the program counter: the index of the failing compiled instruction.
- **6100 to 6132** (`0x17D4` to `0x17F4`) are verifier errors, raised when a template is created
  or finalized. The context is the index of the offending instruction, account, input, register or
  CPI, where there is one.
- In hex, the last four digits are the kind. `0x7177F` is `0x177F` (6015, `RequirementFailed`)
  at program counter 7.

Four runtime kinds carry a different context:

| Kind | Name | Context |
| ---: | --- | --- |
| 6008 | `InvalidRunInputs` | Index of the value that could not be decoded, fixed values first. For leftover bytes, the number of values. For missing group-length bytes, `0` |
| 6010 | `InvalidAccountRange` | The number of accounts, or of rows, that was out of range |
| 6020 | `AccountConstraintFailed` | Index of the account, where `0` is the first account after the template account |
| 6021 | `CpiAccountLimitExceeded` | The CPI's total account count, which exceeded 64 |

A declared signer that did not sign fails with Solana's standard `MissingRequiredSignature` error,
not a Ballista code.

A [registry entry](/guide/registries) passed read-only fails before the first step with
`AccountConstraintFailed` (6020) and the entry's account index, not with `InvalidRegistryEntry`
(6025).

## Error codes

### Runtime codes

| Code | Hex | Name | Meaning |
| ---: | --- | --- | --- |
| 6000 | `0x1770` | `InvalidInstructionData` | The instruction data could not be parsed |
| 6001 | `0x1771` | `InvalidTemplateAccount` | The template account has the wrong address, owner or layout |
| 6002 | `0x1772` | `InvalidTemplateProgram` | The template size is invalid, or the stored template does not match what the run expects |
| 6003 | `0x1773` | `TemplateNotUploading` | Write, finalize or cancel on a template that is already finalized |
| 6004 | `0x1774` | `TemplateNotFinalized` | Run on a template that is still uploading |
| 6005 | `0x1775` | `InvalidCreator` | The signer is not the template's creator |
| 6006 | `0x1776` | `InvalidChunkOffset` | A chunk does not start where the last one ended, or bytes are missing at finalize |
| 6007 | `0x1777` | `HashMismatch` | The uploaded bytes do not match the hash recorded at the start |
| 6008 | `0x1778` | `InvalidRunInputs` | The run data could not be decoded |
| 6009 | `0x1779` | `InvalidRuntimeAccount` | An account failed a check while the template ran |
| 6010 | `0x177A` | `InvalidAccountRange` | Too many or too few accounts or rows |
| 6011 | `0x177B` | `InvalidRegister` | A step read a value that was never set |
| 6012 | `0x177C` | `TypeMismatch` | A value had the wrong type |
| 6013 | `0x177D` | `ArithmeticOverflow` | Arithmetic or a cast overflowed |
| 6014 | `0x177E` | `DivisionByZero` | Division by zero |
| 6015 | `0x177F` | `RequirementFailed` | A `require` step was false |
| 6016 | `0x1780` | `CpiDataTooLarge` | A CPI built more data than its declared maximum |
| 6017 | `0x1781` | `InvalidPdaDerivation` | A PDA could not be derived, or the bump was invalid |
| 6018 | `0x1782` | `MissingReturnData` | The CPI returned no data, or too little for the read |
| 6019 | `0x1783` | `ReturnDataMismatch` | The return data came from a different program |
| 6020 | `0x1784` | `AccountConstraintFailed` | An account does not match its declaration |
| 6021 | `0x1785` | `CpiAccountLimitExceeded` | A CPI's accounts plus its account group exceed 64 |
| 6022 | `0x1786` | `LoopCountExceeded` | A `repeat` count was above the loop's `max` |
| 6023 | `0x1787` | `InstructionOutOfRange` | A read asked for an instruction, account position or byte range that does not exist |
| 6024 | `0x1788` | `WritableAccountBytesRead` | `accountDataBytes` read an account the transaction passed as writable |
| 6025 | `0x1789` | `InvalidRegistryEntry` | An account passed as a registry entry is not that entry: wrong address, owner, size or header |
| 6026 | `0x178A` | `RegistryReentry` | A CPI passed an open registry entry as writable |

### Verifier codes

Raised by `CreateTemplate` or `FinalizeTemplate` when the template fails its checks.

| Code | Hex | Name | Meaning |
| ---: | --- | --- | --- |
| 6100 | `0x17D4` | `Truncated` | The template is shorter than its header says |
| 6101 | `0x17D5` | `PayloadTooLarge` | The template is over 10,240 bytes |
| 6102 | `0x17D6` | `InvalidMagic` | The template does not start with `BVM1` |
| 6103 | `0x17D7` | `UnsupportedVersion` | Unknown bytecode version |
| 6104 | `0x17D8` | `InvalidReservedBytes` | A reserved byte or flag is set |
| 6105 | `0x17D9` | `SectionLengthMismatch` | The sections do not add up to the template's length |
| 6106 | `0x17DA` | `CountOverflow` | A section count overflows |
| 6107 | `0x17DB` | `TooManyAccounts` | Too many accounts at the maximum rows |
| 6108 | `0x17DC` | `TooManyInputs` | Too many inputs or input values |
| 6109 | `0x17DD` | `TooManyRegisters` | More than 64 registers |
| 6110 | `0x17DE` | `TooManyInstructions` | No instructions, or more than 128 |
| 6111 | `0x17DF` | `InvalidBatch` | The batch or a `forEach` is malformed, or a batch has no `forEach` |
| 6112 | `0x17E0` | `InvalidAccountConstraint` | An account declaration is invalid |
| 6113 | `0x17E1` | `InvalidInput` | An input declaration is invalid |
| 6114 | `0x17E2` | `InvalidInstruction` | An instruction is invalid |
| 6115 | `0x17E3` | `InvalidCpi` | A CPI is invalid, or asks for more privilege than the account allows |
| 6116 | `0x17E4` | `InvalidDataSegment` | A data part or PDA seed is invalid |
| 6117 | `0x17E5` | `InvalidRegister` | A register index is out of range |
| 6118 | `0x17E6` | `RegisterNotInitialized` | A register is read before it is set |
| 6119 | `0x17E7` | `TypeMismatch` | An instruction receives the wrong type |
| 6120 | `0x17E8` | `InvalidBlobRange` | A literal points outside the stored bytes |
| 6121 | `0x17E9` | `ExcessiveCpiExpansion` | More than 64 CPIs in the worst case, with every loop run to its maximum |
| 6122 | `0x17EA` | `InvalidFlags` | An instruction sets flags its opcode does not accept |
| 6123 | `0x17EB` | `InvalidCarry` | A carried value is unset before the loop or changes type in it |
| 6124 | `0x17EC` | `ReadOutOfBounds` | A fixed-offset read runs past the account's minimum length |
| 6125 | `0x17ED` | `TooManyCpiAccounts` | A CPI lists more than 64 accounts |
| 6126 | `0x17EE` | `InvalidReturnData` | A return-data read is not directly after an unguarded invoke |
| 6127 | `0x17EF` | `InvalidMinIterations` | The minimum rows exceed the maximum, or are set without a batch |
| 6128 | `0x17F0` | `TooManyAccountGroups` | More than 8 account groups |
| 6129 | `0x17F1` | `InvalidLoop` | More than 8 loops, a `repeat` inside another loop or naming a row, or a malformed `repeat` |
| 6130 | `0x17F2` | `InvalidOutput` | An `emit` or `setReturnData` breaks a rule in [Logs and return data](#logs-and-return-data) |
| 6131 | `0x17F3` | `InvalidIntrospection` | A read of the transaction's instructions names an account not pinned to the Instructions sysvar |
| 6132 | `0x17F4` | `InvalidRegistry` | A registry entry is opened in a loop, passed writable to a CPI, read as raw data, or otherwise breaks a rule in [Registries](/reference/language#registries) |

The same names are in `fixtures/runtime-error-names.txt` and `fixtures/verifier-error-names.txt`.
The program, the Rust SDK and the TypeScript SDK are all tested against them.

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

A called program's error passes through unchanged. Codes outside both ranges are always from a
called program, and both decoders return nothing for them (`undefined` in TypeScript, `None` in
Rust). A called program can also return a number inside Ballista's ranges; the transaction logs
show which program failed.

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
| 5 | 1 | Number of batch rows the run processed. Passes of a `repeat` loop are not counted |
| 6 | 1 | Number of calls the run reached, counting a loop's calls once per row or pass |
| 7 | 8 | Which reached calls actually ran, as a little-endian bit mask: bit `n` is set if reached call `n`, counting from 0, ran |
| 15 | 32 | Template address |

A call skipped by its `when` condition counts as reached, with its bit clear. So the event can tell
"the ATA (associated token account) already existed" apart from "the ATA was created" without
inspecting inner instructions. The event is off by default because it costs a few hundred compute
units, Solana's measure of execution cost.

## Logs and return data

The run event's fields are fixed. To send out values a template computes, use one of two steps.
Their parts are built the way a call's data is, from `data.literal` and `data.encode`, and each
output is at most 1,024 bytes, counting a `bytes` value at its maximum length.

- `step.emit(parts)` logs one `Program data:` field, like the run event, and can appear anywhere,
  loops included. Its first part must be a literal tag of at least 4 bytes that doesn't start with
  `BEV`. A log line names the program that wrote it, Ballista, but not the template, so without
  this rule a template could log a copy of another template's run event.
- `step.setReturnData(parts)` sets the run's return data: bytes a program hands back to whoever
  called it. A program that calls Ballista can read them as soon as the call returns. It can appear
  once, outside every loop, with no invoke after it, because calling a program clears return data.

```ts
const TAG = new TextEncoder().encode('PAID'); // 4 bytes or more, not starting with "BEV"

// The last steps of a template whose earlier steps add up `paid`:
const outputSteps = [
  step.emit([data.literal(TAG), data.encode('u64', expression.variable('paid'))]),
  step.setReturnData([data.encode('u64', expression.variable('paid'))]),
];
```

The TypeScript compiler refuses a template that breaks these rules. One built another way fails
when it is created or finalized, with `InvalidOutput` (6130).
