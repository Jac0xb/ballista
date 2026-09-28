# Wire format

The bytes of a compiled template, of the data a caller sends to run it, and of the registry entries
a template keeps between runs. This is bytecode version 1, the only version the program accepts.
The Rust definitions in `common/src/template/wire.rs` are the source of truth.

Terms used on this page:

- **Payload:** the compiled template. The program stores it in the template account, after the
  account's own header.
- **Record:** one fixed-size item in a table.
- **Register:** a numbered slot that holds one value during a run. A template uses at most 64.
- **Opcode:** the number that says what an instruction does.
- **Blob:** the literal bytes at the end of the payload, such as instruction discriminators (the
  leading bytes that tell a program which instruction to run) and `bytes` constants. Other records
  point into it by offset and length.
- **Verifier:** the part of the Ballista program that checks a payload, once, when the template is
  created in one step or finalized after a chunked upload.
- **CPI:** cross-program invocation, a call from the template to another program.
- **PDA:** program-derived address, an address computed from a program ID and a list of seeds,
  the byte strings it is derived from. The bump is one extra seed byte that makes the result a
  valid program address; the canonical bump is the highest value that does.
- **Runtime accounts:** the accounts passed to a run after the template account. Fixed accounts
  are passed once. Batch rows are sets of accounts, one set per row, that `FOREACH` loops run over.
  An account group is a list of accounts, sized by the caller, that a CPI forwards without the
  template reading them.
- **Loop:** a `FOREACH` or `REPEAT` instruction and the body of instructions that follows it. A
  pass is one run of the body.
- **Instructions sysvar:** the read-only account at `Sysvar1nstructions1111111111111111111111111`
  in which Solana lists the transaction's instructions. Opcodes 64 to 72 read it.
- **Registry entry:** an account Ballista owns that keeps one template's state between runs, one
  per template, registry, and key. Opcodes 75 to 77 open, read, and write it. See
  [Registry entries](#registry-entries).

## Template account {#template-account}

A template lives in its own account, at a PDA of the Ballista program derived from
`['template', creator, templateId]`. The account holds an 80-byte header, then the payload.

```text
┌─────────────── template account (PDA) ───────────────┐
│ account header │ payload: header │ tables … │ blob   │
└───────────────────────────────────────────────────────┘
```

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 1 | Discriminator, `1` |
| 1 | 1 | Account version, `2` |
| 2 | 1 | State: `0` uploading, `1` finalized |
| 3 | 1 | PDA bump |
| 4 | 32 | Creator |
| 36 | 2 | Template ID |
| 38 | 2 | Reserved, zero |
| 40 | 4 | Payload length |
| 44 | 4 | Bytes written so far |
| 48 | 32 | SHA-256 hash of the payload |

A run reads the payload in place, without copying it, and only once the state is finalized.

## Payload {#payload}

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

Registries add no section. A registry's index and size, and a field's offset and type, travel in
the immediates of the registry opcodes; see [Registries](#registries).

## Design rules

- Multi-byte integers are little-endian.
- Every record has one-byte alignment and no padding, so the program reads each table in place
  from account memory without copying it. In Rust the records derive the zerocopy crate's
  `FromBytes`, `IntoBytes`, `KnownLayout`, and `Unaligned` traits.
- The header's counts determine where every section starts and ends.
- Parsing rejects a payload that is truncated, has bytes left over, sets an unknown header flag,
  or uses the reserved header byte. The verifier rejects non-zero reserved bytes in records.
- Instructions refer to registers, accounts, and table records by index, and the verifier checks
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
at position `value & 0x7f` in the current batch row, and is valid only inside `FOREACH`, never
inside `REPEAT`. Account group members have no references; only a CPI descriptor can forward them.

## Input descriptors {#inputs-table-and-cpi-descriptors}

The inputs table holds one 4-byte descriptor per input: the fixed inputs first, then the row
inputs.

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 1 | Value type: 1 `bool`, 2 `u64`, 3 `i64`, 4 `u128`, 5 `pubkey`, 6 `bytes` |
| 1 | 1 | Reserved, zero |
| 2 | 2 | Maximum length: 1 to 1,024 for `bytes`, zero for every other type |

A `LOAD_INPUT` whose `a` operand has bit `0x80` set loads row input `a & 0x7f` of the current row,
and is valid only inside `FOREACH`, never inside `REPEAT`.

## Instruction record

Every instruction is 16 bytes:

| Offset | Bytes | Field | Meaning |
| ---: | ---: | --- | --- |
| 0 | 1 | opcode | The operation |
| 1 | 1 | destination | The register that receives the result, or `0xff` for none |
| 2 | 3 | a, b, c | Operands: registers, account references, table indices, or counts, depending on the opcode |
| 5 | 1 | flags | Bit 0 on read opcodes: the offset comes from register `b`. Zero for every other opcode |
| 6 | 8 | immediate | A constant, a packed range, a length, a read opcode, a loop's carry mask, or a packed registry open or field |
| 14 | 2 | reserved | Must be zero |

A packed range holds a start (a blob offset or a first table index) in its low 32 bits and a length
in its high 32 bits.

### Opcodes

`REQUIRE`, `INVOKE`, `FOREACH`, `REPEAT`, `EMIT`, `SET_RETURN_DATA`, `OPEN_REGISTRY`, and
`WRITE_REGISTRY` produce no value. All but the first three must set the destination to `0xff`.
Every other instruction writes its result to the destination register.

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
| 38 | `LOOP_INDEX` | none; inside a loop only | `u64`, the zero-based row or pass index |
| 40 | `REQUIRE` | `a`: `bool` register; the run fails if it is false | none |
| 41 | `INVOKE` | `a`: CPI descriptor index; `b`: a `bool` guard register, or `0xff` for none | none |
| 42 | `FOREACH` | `a`: body length in instructions; immediate: carry mask, one bit per register whose value survives each row | none |
| 43 | `READ_U8` | as `READ_U64` | `u64` |
| 44 | `READ_U16` | as `READ_U64` | `u64` |
| 45 | `READ_U32` | as `READ_U64` | `u64` |
| 46 | `READ_BOOL` | as `READ_U64`; fails unless the byte is 0 or 1 | `bool` |
| 47 | `DERIVE_PDA` | `a`: the executable program account; immediate: packed range of 1 to 15 data segments used as seeds | `pubkey`; searches for the canonical bump |
| 48 | `RETURN_DATA` | `a`: a read opcode (13 to 16, 43 to 46, or 60) that selects width and type; immediate: byte offset | The read's type; must directly follow an `INVOKE` with no guard |
| 49 | `MOVE` | `a`: source register | A copy of `a`; used to update carried registers |
| 50 | `CREATE_PDA` | as `DERIVE_PDA`, plus `b`: a `u64` register holding the bump | `pubkey`; derives once, and fails with `InvalidPdaDerivation` if the bump exceeds 255 or the result is on the ed25519 curve (not a valid program address) |
| 51 | `MUL_DIV` | `a`, `b`, `c`: three `u64` or three `u128` registers | That type: `a × b ÷ c` rounded down, with the product held exactly; fails on a zero `c` or a result that does not fit |
| 52 | `MUL_DIV_CEIL` | as `MUL_DIV` | As `MUL_DIV`, rounded up |
| 53 | `REM` | as `ADD` | That type: `a` mod `b`, with the sign of `a`; fails on a zero divisor or the `i64` minimum mod −1 |
| 54 | `SHL` | `a`: a `u64` or `u128` register; `b`: a `u64` register, the shift | The type of `a`; fails rather than shift out a set bit |
| 55 | `SHR` | as `SHL` | The type of `a`, rounded down; a shift of the full width or more gives 0 |
| 56 | `BIT_AND` | `a`, `b`: two `u64` or two `u128` registers | That type |
| 57 | `BIT_OR` | as `BIT_AND` | That type |
| 58 | `BIT_XOR` | as `BIT_AND` | That type |
| 59 | `POW10` | `a`: a `u64` register | `u128`, 10 to the power `a`; fails if `a` is above 38 |
| 60 | `READ_I32` | as `READ_U64` | `i64`, sign-extended from 4 bytes |
| 61 | `REPEAT` | `a`: body length in instructions; `b`: the `u64` count register, read once at the start; `c`: maximum passes, 1 to 255; immediate: carry mask, as `FOREACH` | none; fails with `LoopCountExceeded` if the count is above `c` |
| 62 | `EMIT` | immediate: packed range of data segments, the first a literal tag | none; logs the encoded bytes as one `Program data:` field |
| 63 | `SET_RETURN_DATA` | immediate: packed range of data segments | none; sets the encoded bytes as the run's return data |
| 64 | `INSTRUCTION_COUNT` | `a`: the Instructions sysvar account | `u64`, the number of instructions in the transaction |
| 65 | `INSTRUCTION_INDEX` | as `INSTRUCTION_COUNT` | `u64`, the index of the instruction running this template |
| 66 | `INSTRUCTION_PROGRAM` | `a`: the sysvar account; `b`: a `u64` register, the instruction index | `pubkey`, that instruction's program |
| 67 | `INSTRUCTION_ACCOUNT_COUNT` | as `INSTRUCTION_PROGRAM` | `u64`, how many accounts it names |
| 68 | `INSTRUCTION_ACCOUNT` | as `INSTRUCTION_PROGRAM`, plus `c`: a `u64` register, the account position | `pubkey`, that account's key |
| 69 | `INSTRUCTION_ACCOUNT_FLAGS` | as `INSTRUCTION_ACCOUNT` | `u64`: bit 0 signer, bit 1 writable |
| 70 | `INSTRUCTION_DATA_LEN` | as `INSTRUCTION_PROGRAM` | `u64`, the length of its data |
| 71 | `READ_INSTRUCTION_DATA` | as `INSTRUCTION_PROGRAM`, plus `c`: a `u64` register, the byte offset; immediate: a read opcode that selects width and type, as for `RETURN_DATA` | The read's type |
| 72 | `READ_INSTRUCTION_BYTES` | as `READ_INSTRUCTION_DATA`, but the immediate is a length, 1 to 1,024 | `bytes` of exactly that length |
| 73 | `READ_ACCOUNT_BYTES` | `a`: account reference; `b`: a `u64` register, the byte offset; immediate: a length, 1 to 1,024 | `bytes` of exactly that length; fails with `WritableAccountBytesRead` if the account is writable |
| 74 | `BYTES_LEN` | `a`: a `bytes` register | `u64`, its length |
| 75 | `OPEN_REGISTRY` | `a`: the entry account; `b`: a `pubkey` register holding the key, or `0xff` for the zero key; `c`: the payer account; immediate: a packed registry open | none; checks the entry or creates it, then keeps it open for the rest of the run |
| 76 | `READ_REGISTRY` | `a`: an entry account opened earlier; immediate: a packed field | The field, with the type its read opcode gives |
| 77 | `WRITE_REGISTRY` | `a`: the value register; `b`: an entry account opened earlier; immediate: a packed field | none; writes the value into the field |

Opcodes 0 and 39 are unassigned, as is every number above 77.

Read opcodes (13 to 16, 43 to 46, and 60) with the dynamic-offset flag take their offset from the
`u64` register in `b`, and must have a zero immediate. Without the flag, the immediate offset plus
the read width must fit inside the account's declared minimum data length.

#### Loops

- A template holds at most eight loops, `FOREACH` and `REPEAT` counted together. Each sits at the
  top level, and its body is the `a` instructions after it, which cannot include another loop.
- Every `FOREACH` runs over the same batch rows, from the first. A template with a batch has at
  least one `FOREACH`, and one without a batch has none.
- A `REPEAT` needs a body, a maximum of at least 1, and a destination of `0xff`, and its body
  cannot name a row account or row input. The verifier rejects a `REPEAT` that breaks these rules,
  or a ninth loop, with `InvalidLoop`.
- The worst-case CPI count adds, for each loop, the `INVOKE`s in its body times its maximum (the
  header's maximum batch rows for `FOREACH`, `c` for `REPEAT`), plus the `INVOKE`s outside loops,
  plus 3 for each `OPEN_REGISTRY`. It must be at most 64.

#### Outputs

- `EMIT` and `SET_RETURN_DATA` encode their data segments the way a CPI encodes its data. `dst`,
  `a`, `b`, and `c` must be `0xff`, the range must name at least one segment, and the worst-case
  length, counting a `bytes` register at its maximum, must be at most 1,024 bytes.
- An `EMIT`'s first segment must be a literal of at least 4 bytes that does not start with `BEV`,
  the run event's tag family.
- `SET_RETURN_DATA` may appear once, outside every loop, with no `INVOKE` at a later index, because
  invoking a program clears return data.
- The verifier rejects a template that breaks these rules with `InvalidOutput`.

#### Introspection and byte reads

- Opcodes 64 to 72 name, in `a`, a fixed account whose constraint pins its address to the
  Instructions sysvar. The verifier rejects any other account with `InvalidIntrospection`.
- Indexes, positions, and offsets are `u64` registers. An index, position, or byte range that the
  transaction or account does not hold fails the run with `InstructionOutOfRange`.
- `READ_ACCOUNT_BYTES` may name any declared account, including a row account inside `FOREACH`,
  but only one that is read-only in this instruction.
- Byte reads return slices of the sysvar's or the account's data rather than copies. Neither can
  change while the instruction runs.

#### Registries

An `OPEN_REGISTRY` immediate packs the registry it opens:

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 1 | Registry index, below 8 |
| 1 | 2 | Size of the registry's fields, 1 to 512 bytes |
| 3 | 1 | The fixed account pinned to the System program, which creating an entry calls |
| 4 | 4 | Zero |

A `READ_REGISTRY` or `WRITE_REGISTRY` immediate packs one field:

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 2 | Field offset, counted from the end of the entry's 72-byte header |
| 2 | 1 | A read opcode (13 to 16, 43 to 46, or 60) that sets the field's width and type |
| 3 | 5 | Zero |

The verifier checks:

- `OPEN_REGISTRY` sits at the top level, never in a loop body, and never after a
  `SET_RETURN_DATA`. A template holds at most 8, opens each entry account once, and gives every
  open of one registry index the same size.
- The entry account is a fixed account declared writable and nothing else: no signer or executable
  flag, no address or owner pin, and a minimum data length of 0. The payer is a fixed account
  declared signer and writable. The System program account is a fixed account pinned to the System
  program's address.
- `b` is a set `pubkey` register, or `0xff` for the zero key of 32 zero bytes.
- No CPI account record lists an entry account writable, whether its `INVOKE` comes before the
  open, after it, or never.
- `READ_REGISTRY` and `WRITE_REGISTRY` name an account opened by an `OPEN_REGISTRY` at a lower
  index, and a field that fits inside that registry's size. A read's `b` and `c`, and a write's
  destination and `c`, are `0xff`.
- A read takes any read opcode. A write takes only `READ_BOOL`, `READ_U64`, `READ_I64`,
  `READ_U128`, or `READ_PUBKEY`, whose width holds every value of its type, and its value register
  must have that type.
- No read opcode (13 to 16, 43 to 46, or 60) and no `READ_ACCOUNT_BYTES` names an entry account,
  wherever either sits: fields are read only with `READ_REGISTRY`. The entry's key, owner,
  lamports, data length, and emptiness stay readable.

The verifier rejects a template that breaks these rules with `InvalidRegistry` (6132). It also
counts each open as 3 CPIs toward the limit of 64, since creating an entry whose address already
holds lamports takes a transfer, an allocate, and an assign.

At run time:

- An entry account passed read-only fails before the first instruction, at account validation,
  with `AccountConstraintFailed` (6020).
- An open checks the entry, or creates it, as described under
  [Registry entries](#registry-entries). An account that is not the entry fails with
  `InvalidRegistryEntry` (6025).
- An open entry stays marked as borrowed for the rest of the run. A CPI that passes it writable,
  which only a batch-row account or group member can still do, fails with `RegistryReentry`
  (6026).

## CPI descriptors

Each CPI the template can make is described once by a 12-byte descriptor. An `INVOKE` names a
descriptor, and a loop can invoke the same descriptor on every pass.

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

Each segment is 8 bytes. CPI descriptors use segments to build instruction data, `EMIT` and
`SET_RETURN_DATA` use them to build their output, and `DERIVE_PDA` and `CREATE_PDA` use them as
seeds.

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

## Registry entries {#registry-entries}

A registry entry is an account Ballista owns: a 72-byte header, then the registry's fields.

| Offset | Bytes | Field |
| ---: | ---: | --- |
| 0 | 4 | Magic bytes `BREG` |
| 4 | 1 | Entry version, `1` |
| 5 | 1 | Registry index |
| 6 | 2 | Reserved, zero |
| 8 | 32 | Template address |
| 40 | 32 | Key |
| 72 | Registry size | Fields, zero when the entry is created |

The account is exactly 72 bytes plus the registry's size, so at most 584 bytes. A field offset in
`READ_REGISTRY` or `WRITE_REGISTRY` counts from byte 72. A written `bool` is one byte, 0 or 1.

The entry's address is a PDA of the Ballista program, with the canonical bump and these seeds:

```text
"registry" | template address (32 bytes) | registry index (1 byte) | key (32 bytes)
```

The key is the 32 bytes the template computes: an address such as the caller's, or all zeros for
a template-wide entry. `findRegistryEntryAddress` (TypeScript, from `@jac0xb/ballista/kit`) and
`find_registry_entry_address` (Rust) return the address and bump.

An open handles two cases:

- **The account is owned by Ballista.** Its size and header must match the running template, the
  open's registry index and size, and the key. The address is not derived again: only Ballista
  writes an entry's header, and only at the address that header derives.
- **The account has no data and is owned by the System program.** It must be at the derived
  address. With no lamports there, the System program's `CreateAccount` makes it, funded by the
  payer with the rent-exempt minimum for its size (the balance an account needs to stay on chain).
  If the address already holds lamports, the payer transfers only what is missing, then `Allocate`
  and `Assign` make the account Ballista's. Ballista then writes the header.

Anything else fails with `InvalidRegistryEntry`. Ballista signs `CreateAccount`, `Allocate`, and
`Assign` with the entry's seeds. No other call in a run carries a Ballista signature, and nothing
closes an entry, so its rent stays locked.

## Error codes

A custom error code is `kind | (context << 16)`. Runtime kinds start at 6000 (`0x1770`) and verifier
kinds at 6100 (`0x17D4`). The full table, with hex forms, is in
[Errors and events](/guide/errors-and-events#error-codes).

## Run event

When the header sets `PROGRAM_FLAG_EMIT_EVENT`, a successful run logs this 47-byte record with
`sol_log_data`. It appears in the transaction logs as a `Program data:` line.

```text
"BEV1" | version u8 | iterations u8 | expanded u8 | executed u64 | template address [u8; 32]
```

| Field | Meaning |
| --- | --- |
| `version` | Bytecode version, `1` |
| `iterations` | Batch rows in the run. `REPEAT` passes are not counted |
| `expanded` | `INVOKE` instructions reached, counting each loop pass separately |
| `executed` | Bitmask, one bit per invoke reached, in order and counting from bit 0. A bit is set when that invoke ran, and clear when its guard skipped it |
| `template address` | The template account that ran |

Each `EMIT` also logs a `Program data:` line. Its tag cannot start with `BEV`, so an `EMIT` line
cannot be mistaken for this event.

## Why this is zero-copy

Parsing a payload copies nothing: the parser returns slices that point into the template account's
memory for every record table. Running a template allocates its buffers once per run: the decoded
inputs, the register file, one set of buffers for building CPIs, reused by every CPI in the run,
one register snapshot shared by every loop, and, at the first `EMIT` or `SET_RETURN_DATA`, one
1,024-byte output buffer that every later output reuses. PDA seeds are assembled on the stack, and
byte reads borrow the sysvar's or the account's data. Heap use therefore does not grow with the
number of CPIs, loops, or outputs a run performs. "Zero-copy" describes how the stored payload is
read, not an execution engine that never allocates.

The example payloads in `fixtures/` are produced by the TypeScript compiler. The Rust tests verify
them and run them against the program, and check that the Rust builder reproduces
`system-transfer.hex` byte for byte. Both SDKs derive entry addresses against
`fixtures/registry-entry-addresses.txt`, vectors the program's own derivation produces.
