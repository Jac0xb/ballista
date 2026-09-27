# Wire format

This page specifies the bytes of a compiled template and of the data a caller sends to run it. It
describes bytecode version 1, the only version the program accepts. The Rust definitions in
`common/src/template/wire.rs` are the source of truth.

Terms used on this page:

- **Payload:** the compiled template. The program stores it in the template account, after the
  account's own header.
- **Record:** one fixed-size entry in a table.
- **Register:** a numbered slot that holds one value during a run. A template uses at most 64.
- **Opcode:** the number that says what an instruction does.
- **Blob:** the literal bytes at the end of the payload, such as instruction discriminators (the
  leading bytes that tell a program which instruction to run) and `bytes` constants. Other records
  point into it by offset and length.
- **Verifier:** the part of the Ballista program that checks a payload, once, when the template is
  created in one step or finalized after a chunked upload.
- **CPI:** cross-program invocation, a call from the template to another program.
- **PDA:** program-derived address, an address computed from a program ID and a list of seeds.
  The bump is one extra seed byte that makes the result a valid program address; the canonical
  bump is the highest value that does.
- **Runtime accounts:** the accounts passed to a run after the template account. Fixed accounts
  are passed once. A batch row is a set of accounts repeated for each loop iteration. An account
  group is a list of accounts, sized by the caller, that a CPI forwards without the template
  reading them.

The payload is a header followed by eight sections:

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

| Section | Record size | Number of records |
| --- | ---: | --- |
| Program header | 24 bytes | One |
| Account constraints | 8 bytes | Fixed account count plus accounts per batch row |
| Input descriptors | 4 bytes | Fixed input count plus row input count |
| Instructions | 16 bytes | Instruction count |
| CPI descriptors | 12 bytes | CPI descriptor count |
| CPI account records | 2 bytes | CPI account record count |
| Data segments | 8 bytes | Data segment count |
| Pubkeys | 32 bytes | Pubkey count |
| Blob | 1 byte | Blob length |

## Design rules

- Multi-byte integers are little-endian.
- Every record has one-byte alignment and no padding, so the program reads each table in place
  from account memory without copying it. In Rust the records derive the zerocopy crate's
  `FromBytes`, `IntoBytes`, `KnownLayout`, and `Unaligned` traits.
- The header's counts determine where every section starts and ends.
- Parsing rejects a payload that is truncated, has bytes left over, sets an unknown header flag,
  or uses the reserved header byte. The verifier rejects non-zero reserved bytes in records.
- Instructions refer to registers, accounts, and table entries by index, and the verifier checks
  that every index is in range.
- Literal data is stored once in the blob and referred to by offset and length.
- The verifier checks the whole payload once. A run parses the structure again but relies on that
  check instead of repeating it.

## Program header

The header is 24 bytes.

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 4 | Magic bytes `BVM1` |
| 4 | 1 | Bytecode version, `1` |
| 5 | 1 | Fixed account count |
| 6 | 1 | Accounts per batch row (the batch stride), or 0 without a batch |
| 7 | 1 | Maximum batch rows |
| 8 | 1 | Fixed input count |
| 9 | 1 | Register count |
| 10 | 1 | Instruction count |
| 11 | 1 | CPI descriptor count |
| 12 | 2 | CPI account record count |
| 14 | 2 | Data segment count |
| 16 | 1 | Pubkey count |
| 17 | 1 | Flags |
| 18 | 2 | Blob length |
| 20 | 1 | Minimum batch rows |
| 21 | 1 | Row input count |
| 22 | 1 | Account group count |
| 23 | 1 | Reserved, zero |

Flag bit 0 (`PROGRAM_FLAG_EMIT_EVENT`) makes the program log a run event after each successful
run. The other bits are reserved and rejected.

## Account constraints

Each fixed account, then each account of the batch row, has one 8-byte constraint record.

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 1 | Flags: bit 0 signer, bit 1 writable, bit 2 executable |
| 1 | 1 | Required address, as an index into the pubkey table, or `0xff` for none |
| 2 | 1 | Required owner, as an index into the pubkey table, or `0xff` for none |
| 3 | 1 | Reserved, zero |
| 4 | 4 | Minimum data length |

### Account references

Instructions, CPI descriptors, and CPI account records name an account with a one-byte reference.
A value below `0x80` is the index of a fixed account. A value with bit `0x80` set names the account
at position `value & 0x7f` in the current batch row, and is valid only inside `FOREACH`. Account
group members have no references; only a CPI descriptor can forward them.

## Input descriptors {#inputs-table-and-cpi-descriptors}

The inputs table holds one 4-byte descriptor per input: the fixed inputs first, then the row
inputs.

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 1 | Value type: 1 `bool`, 2 `u64`, 3 `i64`, 4 `u128`, 5 `pubkey`, 6 `bytes` |
| 1 | 1 | Reserved, zero |
| 2 | 2 | Maximum length: 1 to 1,024 for `bytes`, zero for every other type |

A `LOAD_INPUT` whose `a` operand has bit `0x80` set loads row input `a & 0x7f` of the current row,
and is valid only inside `FOREACH`.

## Instruction record

Every instruction is 16 bytes:

| Offset | Bytes | Field | Meaning |
| ---: | ---: | --- | --- |
| 0 | 1 | opcode | The operation |
| 1 | 1 | destination | The register that receives the result, or `0xff` for none |
| 2 | 3 | a, b, c | Operands: registers, account references, or table indices, depending on the opcode |
| 5 | 1 | flags | Bit 0 on read opcodes: the offset comes from register `b`. Zero for every other opcode |
| 6 | 8 | immediate | A constant, a packed range, or the loop's carry mask |
| 14 | 2 | reserved | Must be zero |

A packed range holds a start (a blob offset or a first table index) in its low 32 bits and a length
in its high 32 bits.

### Opcodes

`REQUIRE`, `INVOKE`, and `FOREACH` produce no value. Every other instruction writes its result to
the destination register.

| Opcode | Name | Operands | Result |
| ---: | --- | --- | --- |
| 1 | `LOAD_INPUT` | `a`: input index; bit `0x80` selects a row input | The input's type |
| 2 | `CONST_BOOL` | `a`: 0 or 1 | `bool` |
| 3 | `CONST_U64` | immediate: the value | `u64` |
| 4 | `CONST_I64` | immediate: the value, two's complement | `i64` |
| 5 | `CONST_U128` | immediate: packed blob range of exactly 16 bytes | `u128` |
| 6 | `CONST_PUBKEY` | `a`: pubkey table index | `pubkey` |
| 7 | `CONST_BYTES` | immediate: packed blob range of at most 1,024 bytes | `bytes` |
| 8 | `ACCOUNT_KEY` | `a`: account reference | `pubkey` |
| 9 | `ACCOUNT_OWNER` | `a`: account reference | `pubkey` |
| 10 | `ACCOUNT_LAMPORTS` | `a`: account reference | `u64` |
| 11 | `ACCOUNT_DATA_LEN` | `a`: account reference | `u64` |
| 12 | `ACCOUNT_IS_EMPTY` | `a`: account reference | `bool`, true when the account has no data |
| 13 | `READ_U64` | `a`: account reference; immediate: byte offset | `u64` |
| 14 | `READ_I64` | as `READ_U64` | `i64` |
| 15 | `READ_U128` | as `READ_U64` | `u128` |
| 16 | `READ_PUBKEY` | as `READ_U64` | `pubkey` |
| 17 | `CLOCK_SLOT` | none | `u64` |
| 18 | `CLOCK_TIMESTAMP` | none | `i64` |
| 19 | `ADD` | `a`, `b`: registers of the same numeric type | That type; fails on overflow |
| 20 | `SUB` | as `ADD` | That type; fails on overflow |
| 21 | `MUL` | as `ADD` | That type; fails on overflow |
| 22 | `DIV` | as `ADD` | That type; fails on a zero divisor or overflow |
| 23 | `EQ` | `a`, `b`: registers of the same type | `bool` |
| 24 | `NE` | as `EQ` | `bool` |
| 25 | `LT` | `a`, `b`: registers of the same numeric type | `bool` |
| 26 | `LTE` | as `LT` | `bool` |
| 27 | `GT` | as `LT` | `bool` |
| 28 | `GTE` | as `LT` | `bool` |
| 29 | `AND` | `a`, `b`: `bool` registers | `bool` |
| 30 | `OR` | `a`, `b`: `bool` registers | `bool` |
| 31 | `NOT` | `a`: `bool` register | `bool` |
| 32 | `MIN` | `a`, `b`: registers of the same numeric type | That type |
| 33 | `MAX` | as `MIN` | That type |
| 34 | `SELECT` | `a`: `bool` condition; `b`: value if true; `c`: value if false, the same type as `b` | The type of `b` |
| 35 | `CAST_U64` | `a`: numeric register | `u64`; fails if the value does not fit |
| 36 | `CAST_I64` | `a`: numeric register | `i64`; fails if the value does not fit |
| 37 | `CAST_U128` | `a`: numeric register | `u128`; fails if the value does not fit |
| 38 | `LOOP_INDEX` | none; inside `FOREACH` only | `u64`, the zero-based row index |
| 40 | `REQUIRE` | `a`: `bool` register; the run fails if it is false | none |
| 41 | `INVOKE` | `a`: CPI descriptor index; `b`: a `bool` guard register, or `0xff` for none | none |
| 42 | `FOREACH` | `a`: body length in instructions; immediate: carry mask, one bit per register whose value survives each row | none |
| 43 | `READ_U8` | as `READ_U64` | `u64` |
| 44 | `READ_U16` | as `READ_U64` | `u64` |
| 45 | `READ_U32` | as `READ_U64` | `u64` |
| 46 | `READ_BOOL` | as `READ_U64`; fails unless the byte is 0 or 1 | `bool` |
| 47 | `DERIVE_PDA` | `a`: the executable program account; immediate: packed range of 1 to 15 data segments used as seeds | `pubkey`; searches for the canonical bump |
| 48 | `RETURN_DATA` | `a`: a read opcode (13 to 16 or 43 to 46) that selects width and type; immediate: byte offset | The read's type; must directly follow an `INVOKE` with no guard |
| 49 | `MOVE` | `a`: source register | A copy of `a`; used to update carried registers |
| 50 | `CREATE_PDA` | as `DERIVE_PDA`, plus `b`: a `u64` register holding the bump | `pubkey`; derives once, and fails with `InvalidPdaDerivation` if the bump exceeds 255 or the result is on the ed25519 curve (not a valid program address) |

Opcode 39 is unassigned. A `FOREACH` appears exactly once when the header declares a batch and
never otherwise. Its body follows it directly and cannot contain another `FOREACH`.

Read opcodes (13 to 16 and 43 to 46) with the dynamic-offset flag take their offset from the `u64`
register in `b`, and must have a zero immediate. Without the flag, the immediate offset plus the
read width must fit inside the account's declared minimum data length.

## CPI descriptors

Each CPI the template can make is described once by a 12-byte descriptor. An `INVOKE` names a
descriptor, and a loop can invoke the same descriptor on every row.

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 1 | Program account reference; the account must be declared executable |
| 1 | 1 | Account group forwarded after the listed accounts, or `0xff` for none |
| 2 | 2 | Index of the first CPI account record |
| 4 | 1 | Number of CPI account records, at most 64 |
| 5 | 1 | Number of data segments |
| 6 | 2 | Index of the first data segment |
| 8 | 2 | Maximum instruction data length: the sum of the segments' maximum lengths, at most 4,096 |
| 10 | 2 | Reserved, zero |

Group members follow the listed accounts in the CPI. They keep the writable flag the transaction
gave them and are never passed as signers.

### CPI account records

Each record is 2 bytes:

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 1 | Account reference |
| 1 | 1 | Flags: bit 0 signer, bit 1 writable. May not include a flag that the account's constraint lacks |

### Data segments

Each segment is 8 bytes. CPI descriptors use segments to build instruction data, and
`DERIVE_PDA` and `CREATE_PDA` use them as seeds.

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 1 | Kind: 0 literal, 1 `u8`, 2 `u16`, 3 `u32`, 4 `u64`, 5 `i64`, 6 `u128`, 7 `pubkey`, 8 `bool`, 9 `bytes` |
| 1 | 1 | Source register, or `0xff` for a literal |
| 2 | 2 | Literal: offset into the blob. Zero otherwise |
| 4 | 2 | Literal: length. Zero otherwise |
| 6 | 2 | Reserved, zero |

Kinds 1 to 4 encode a `u64` or `u128` register at that width, and fail the run if the value does not
fit. Kind 9 copies a `bytes` register as is, with no length prefix. A seed may not exceed 32 bytes.

### Pubkeys and blob

After the data segments come the pubkey table (32 bytes per key, used by account constraints and
`CONST_PUBKEY`), then the blob. The payload must end exactly where the blob ends.

## Run data

The `Run` instruction's data is the byte `5` followed by the run data, which may be at most 1,024
bytes. Its first account is the template account, followed by the runtime accounts.

```text
group lengths [u8; account group count] | fixed input values | row input values × iterations
```

Values are encoded by type: `bool` as one byte (0 or 1), `u64` and `i64` as eight little-endian
bytes, `u128` as sixteen, `pubkey` as thirty-two, and `bytes` as a little-endian `u16` length
followed by the bytes. Runtime accounts come in this order: fixed accounts, batch rows, then
account group members, group by group in declaration order. The number of rows is
`(accounts − fixed − Σ group lengths) / stride`. The division must be exact, and the result must lie
between the batch's minimum and maximum.

## Error codes

A custom error code packs a 16-bit kind and a 16-bit context:

```text
code = kind | (context << 16)
```

Runtime kinds start at 6000 and verifier kinds at 6100. The name tables live in
`fixtures/runtime-error-names.txt` and `fixtures/verifier-error-names.txt`. See
[Errors and events](/guide/errors-and-events) for what each context means.

## Run event

When the header sets `PROGRAM_FLAG_EMIT_EVENT`, a successful run logs this 47-byte record with
`sol_log_data`. It appears in the transaction logs as a `Program data:` line.

```text
"BEV1" | version u8 | iterations u8 | expanded u8 | executed u64 | template address [u8; 32]
```

| Field | Meaning |
| --- | --- |
| `version` | Bytecode version, `1` |
| `iterations` | Batch rows processed |
| `expanded` | `INVOKE` instructions reached, counting each row separately |
| `executed` | Bitmask, one bit per invoke reached, in order and counting from bit 0. A bit is set when that invoke ran, and clear when its guard skipped it |
| `template address` | The template account that ran |

## Why this is zero-copy

Parsing a payload copies nothing: the parser returns slices that point into the template account's
memory for every record table. Running a template allocates its buffers once per run: the decoded
inputs, the register file, and one set of buffers for building CPIs, reused by every CPI in the
run. PDA seeds are assembled on the stack. Heap use therefore does not grow with the number of
CPIs a run performs. "Zero-copy" describes how the stored payload is read, not an execution engine
that never allocates.

The example payloads in `fixtures/` are produced by the TypeScript compiler. The Rust tests verify
them and run them against the program, and check that the Rust builder reproduces
`system-transfer.hex` byte for byte.
