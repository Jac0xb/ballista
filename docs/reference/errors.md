# Error codes

Ballista fails with a Solana custom program error, `Custom(u32)`. The low 16 bits are the error
kind, listed below, and the high 16 bits are a context number that says where the failure happened.
[Errors and events](/guide/errors-and-events#decoding) shows how to decode a code and name the step
that failed.

```text
code = kind | (context << 16)
```

In hex, the last four digits are the kind: `0x7177F` is `0x177F` (6015, `RequirementFailed`) at
program counter 7. A program the run calls can fail with its own code, which passes through
unchanged, and Anchor programs number theirs from 6000 too. Check
[which program failed](/guide/errors-and-events#which-program-failed) before you read a code as
Ballista's.

## Context

For most runtime kinds, the context is the program counter: the index of the failing bytecode
instruction. Four carry something else:

| Kind | Name | Context |
| ---: | --- | --- |
| 6008 | `InvalidRunInputs` | Index of the value that could not be decoded, fixed values first. For leftover bytes, the number of values. For missing group-length bytes, `0` |
| 6010 | `InvalidAccountRange` | The number of accounts, or of rows, that was out of range. The program reads only an instruction's first 128 accounts, the template's included, so more than 127 runtime accounts report 127 |
| 6020 | `AccountConstraintFailed` | Index of the account, where `0` is the first account after the template account |
| 6021 | `CpiAccountLimitExceeded` | The CPI's total account count, which exceeded 64 |

For a verifier kind, the context is the index of the offending instruction, account, input,
register, CPI or data segment, where there is one.

## Runtime codes

Raised while uploading or running a template.

| Code | Hex | Name | Meaning |
| ---: | --- | --- | --- |
| 6000 | `0x1770` | `InvalidInstructionData` | The instruction data could not be parsed |
| 6001 | `0x1771` | `InvalidTemplateAccount` | The template account has the wrong address, owner or layout |
| 6002 | `0x1772` | `InvalidTemplateProgram` | The stored template does not match what the run expects |
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
| 6025 | `0x1789` | `InvalidRegistryEntry` | An account passed as a registry entry is not that entry (wrong address, owner, size or header), or is an entry this run already has open (two entries of one registry with equal keys) |
| 6026 | `0x178A` | `RegistryReentry` | A CPI passed an open registry entry as writable, or passed one read-only with the template's own account writable |

A declared signer that did not sign fails with Solana's `MissingRequiredSignature`, not a Ballista
code. A registry entry passed read-only fails before the first step with `AccountConstraintFailed`
(6020) and the entry's account index.

The upload instructions take an exact account list and report a wrong one with Solana's errors:
an extra or missing account fails with `NotEnoughAccountKeys`, a creator or template account that
is not writable with `MissingRequiredSignature`, and a third account that is not the System program
with `IncorrectProgramId`. An empty chunk, or a size of 0 or over 10,160 bytes, fails with
`InvalidInstructionData` (6000).

## Verifier codes

Raised by `CreateTemplate` or `FinalizeTemplate` when the template fails its checks.

| Code | Hex | Name | Meaning |
| ---: | --- | --- | --- |
| 6100 | `0x17D4` | `Truncated` | The template is shorter than its header says |
| 6101 | `0x17D5` | `PayloadTooLarge` | The template is over 10,160 bytes. Only off-chain verification reports it: an upload that large is refused first, with `InvalidInstructionData` |
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
| 6114 | `0x17E2` | `InvalidInstruction` | An instruction is invalid, or sets a field its opcode doesn't use |
| 6115 | `0x17E3` | `InvalidCpi` | A CPI is invalid, is never invoked, or asks for more privilege than the account allows |
| 6116 | `0x17E4` | `InvalidDataSegment` | A data part or PDA seed is invalid, or nothing uses it |
| 6117 | `0x17E5` | `InvalidRegister` | A register index is out of range |
| 6118 | `0x17E6` | `RegisterNotInitialized` | A register is read before it is set |
| 6119 | `0x17E7` | `TypeMismatch` | An instruction receives the wrong type |
| 6120 | `0x17E8` | `InvalidBlobRange` | A `u128` or `bytes` constant points outside the stored bytes, or has the wrong length |
| 6121 | `0x17E9` | `ExcessiveCpiExpansion` | More than 64 CPIs in the worst case, with every loop run to its maximum |
| 6122 | `0x17EA` | `InvalidFlags` | An instruction sets flags its opcode does not accept |
| 6123 | `0x17EB` | `InvalidCarry` | A carried value is unset before the loop or changes type in it |
| 6124 | `0x17EC` | `ReadOutOfBounds` | A fixed-offset read runs past the account's minimum length |
| 6125 | `0x17ED` | `TooManyCpiAccounts` | A CPI lists more than 64 accounts |
| 6126 | `0x17EE` | `InvalidReturnData` | A return-data read is not directly after an unguarded invoke |
| 6127 | `0x17EF` | `InvalidMinIterations` | The minimum rows exceed the maximum, or are set without a batch |
| 6128 | `0x17F0` | `TooManyAccountGroups` | More than 8 account groups |
| 6129 | `0x17F1` | `InvalidLoop` | More than 8 loops, a `repeat` inside another loop or naming a row, or a malformed `repeat` |
| 6130 | `0x17F2` | `InvalidOutput` | An `emit` or `setReturnData` breaks a rule in [Output](/reference/language#output) |
| 6131 | `0x17F3` | `InvalidIntrospection` | A read of the transaction's instructions names an account not pinned to the Instructions sysvar |
| 6132 | `0x17F4` | `InvalidRegistry` | A registry entry is opened in a loop or after a CPI, passed writable to a CPI, read as raw data, or otherwise breaks a rule in [Registries](/reference/language#registries) |
| 6133 | `0x17F5` | `InvalidAccountGroup` | A group expression names an undeclared group, or its filter breaks a rule in [Account groups](/reference/language#account-groups) |

The same names, in code order, are in `fixtures/runtime-error-names.txt` and
`fixtures/verifier-error-names.txt`, which the program and both SDKs are tested against. The
TypeScript SDK exports them as `RUNTIME_ERROR_NAMES` and `VERIFIER_ERROR_NAMES`.
