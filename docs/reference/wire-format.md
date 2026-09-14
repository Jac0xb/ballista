# Wire format

The template payload is a canonical sequence of alignment-free fixed-record tables followed by
constant pubkeys and literal bytes.

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
- The parser rejects truncation, overlap by construction, trailing bytes, and reserved-bit use.
- Instructions refer to registers, schemas, and descriptor ranges by bounded indices.
- Literal data uses offset/length pairs into the final blob.
- Finalization verifies the complete program; `Run` borrows it without re-hashing or rebuilding it.

## Instruction record

Every VM instruction is 16 bytes:

| Field | Bytes | Meaning |
| --- | ---: | --- |
| opcode | 1 | operation |
| destination | 1 | output register or `0xff` |
| a / b / c | 3 | register, account, or descriptor operands |
| flags | 1 | reserved in v2 |
| immediate | 8 | scalar or packed offset/length |
| reserved | 2 | must be zero |

## Why this is zero-copy

The parser returns slices into account memory for all record tables. The runtime does allocate its
bounded register file, CPI metadata, generated CPI bytes, and PDA seed scratch; “zero-copy” refers
to parsing the immutable stored program, not to an allocation-free execution engine.

See `common/src/template/wire.rs` for the normative Rust layout and the shared fixture tests in
`common/src/template/verify.rs` and `clients/js/src/compiler.test.ts`.
