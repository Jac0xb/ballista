# Wire format

The template payload is a canonical sequence of alignment-free fixed-record tables followed by
constant pubkeys and literal bytes. This page describes bytecode version 1, the only version the program accepts.

```text
ProgramHeader
AccountConstraint[]
InputDescriptor[]
InstructionRecord[]
CpiDescriptor[]
CpiAccountRecord[]
DataSegment[]
PubkeyRecord[]
blob bytes
```

## Design rules

- Multi-byte integers are little-endian byte arrays.
- All record types implement Zerocopy `FromBytes`, `IntoBytes`, `KnownLayout`, and `Unaligned`.
- Header counts determine every section boundary.
- The parser rejects truncation, trailing bytes, unknown flags, and reserved-bit use.
- Instructions refer to registers, schemas, and descriptor ranges by bounded indices.
- Literal data uses offset/length pairs into the final blob.
- Finalization verifies the complete program; `Run` re-parses structure but trusts verification.

## Program header

The header is 24 bytes.

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 4 | Magic `BVM1` |
| 4 | 1 | Version, `1` |
| 5 | 1 | Fixed account count |
| 6 | 1 | Batch stride (row accounts) |
| 7 | 1 | Batch maximum iterations |
| 8 | 1 | Input count |
| 9 | 1 | Register count |
| 10 | 1 | Instruction count |
| 11 | 1 | CPI descriptor count |
| 12 | 2 | CPI account record count |
| 14 | 2 | Data segment count |
| 16 | 1 | Pubkey count |
| 17 | 1 | Flags |
| 18 | 2 | Blob length |
| 20 | 1 | Batch minimum iterations |
| 21 | 1 | Row input count |
| 22 | 1 | Account group count |
| 23 | 1 | Reserved, zero |

Flag bit 0 (`PROGRAM_FLAG_EMIT_EVENT`) asks the runtime to log a run event after success. Other
bits are reserved and rejected.

## Instruction record

Every VM instruction is 16 bytes:

| Field | Bytes | Meaning |
| --- | ---: | --- |
| opcode | 1 | operation |
| destination | 1 | output register or `0xff` |
| a / b / c | 3 | register, account, or descriptor operands |
| flags | 1 | bit 0 on read opcodes: offset comes from register `b` |
| immediate | 8 | scalar, packed offset/length, or carry mask |
| reserved | 2 | must be zero |

Batch, return-data, move, and derivation opcodes:

| Opcode | Name | Operands |
| ---: | --- | --- |
| 42 | `FOREACH` | `a` body length; immediate is a 64-bit carry mask of registers that survive iterations |
| 47 | `DERIVE_PDA` | `a` the executable program account; immediate a packed seed-segment range; searches for the canonical bump |
| 48 | `RETURN_DATA` | `a` a read opcode selecting width and type; immediate the byte offset; must directly follow an unconditional `INVOKE` |
| 49 | `MOVE` | copies register `a` into the destination |
| 50 | `CREATE_PDA` | as `DERIVE_PDA`, plus `b` a `u64` register holding the bump; derives once, and fails with `InvalidPdaDerivation` if the bump exceeds 255 or the result is on the curve |

Read opcodes (`13` to `16`, `43` to `46`) with the dynamic-offset flag take their offset from the
`u64` register in `b` and must have a zero immediate. Without the flag, the immediate offset plus
the read width must fit inside the account's declared minimum data length.

## Inputs table and CPI descriptors

The inputs table holds the fixed input descriptors followed by the row input descriptors, `input
count + row input count` records of 4 bytes each. A `LOAD_INPUT` whose `a` operand has bit `0x80`
set loads row input `a & 0x7f` of the current iteration and is valid only inside `FOREACH`.

Each 12-byte CPI descriptor names its program account, then the account group it forwards (`0xff`
for none), the account record range, the segment range, and the maximum data length. Group members
follow the declared accounts in the CPI and are passed with the transaction's writable flag and
signer cleared.

## Run data

```text
group lengths [u8; account group count] | fixed input values | row input values × iterations
```

Values are encoded by type: `bool` one byte, `u64` and `i64` eight bytes little-endian, `u128`
sixteen, `pubkey` thirty-two, `bytes` a little-endian `u16` length then the bytes. Runtime accounts
are laid out as fixed accounts, batch rows, then the groups in declaration order; the iteration
count is `(accounts − fixed − Σ group lengths) / stride`.

## Error codes

Custom error codes pack a 16-bit kind and a 16-bit context:

```text
code = kind | (context << 16)
```

Runtime kinds start at 6000 and verifier kinds at 6100. The tables of names live in
`fixtures/runtime-error-names.txt` and `fixtures/verifier-error-names.txt`. See
[Errors and events](/guide/errors-and-events).

## Run event

```text
"BEV1" | version u8 | iterations u8 | expanded u8 | executed u64 | template address [u8; 32]
```

## Why this is zero-copy

The parser returns slices into account memory for all record tables. The runtime allocates its
register file and one set of CPI scratch buffers per run, and derives PDA seeds on the stack, so
heap use is constant regardless of how many CPIs a run performs. "Zero-copy" refers to parsing the
immutable stored program, not to an allocation-free execution engine.

See `common/src/template/wire.rs` for the normative Rust layout. The compiler fixtures in
`fixtures/` are produced by the TypeScript SDK and verified by the Rust suites.
