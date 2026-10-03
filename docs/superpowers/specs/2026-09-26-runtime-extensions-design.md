# Runtime extensions: math, loops, output, introspection

Status: proposed · 2026-09-26 · builds on `cu/integrated` (the three compute-unit branches merged)

## Goal

Every template worth writing has one shape: read state during the run, compute a number or a
decision, act on it, check the result. The runtime reads and checks well. It computes poorly
(no wide math, no bit operations, no signed 32-bit reads) and it only ever acts through CPIs and
one fixed run event. These four additions widen the middle of that shape without changing what
Ballista is:

1. **Math** that protocol numbers need: multiply-then-divide with a 256-bit intermediate,
   remainder, shifts, bitwise operations, powers of ten, and an `i32` read.
2. **Loops** driven by a count, and more than one loop per template.
3. **Output**: log values (`emit`) and set Solana return data (`setReturnData`).
4. **Introspection**: read the other instructions in the transaction, and read bounded byte
   ranges from instruction data and from read-only accounts.

## What must stay true

- Finalization still proves termination, that every register is written before it is read,
  that every account reference is in the schema, that CPI privileges never exceed the schema,
  at most 64 CPIs per run, and at most 4,096 bytes per CPI.
- Ballista stays stateless and non-custodial. It never signs as a PDA. Unattended execution
  (delegate signing, persistent state) is out of scope.
- New behaviour arrives only as new opcodes. A run never re-verifies a stored template, so
  meaning must not be attached to operands or flags that the current verifier ignores.
- The header, account records, input records, run-data layout and bytecode version (1) do not
  change.
- `RuntimeValue` gains no variant. A `bytes` value stays a borrowed slice; there is no arena.
- The TypeScript compiler and the Rust `ProgramBuilder` keep emitting identical bytes.

## 1. Math

| Opcode | Operands | Types | Result | Fails when |
| --- | --- | --- | --- | --- |
| `MUL_DIV` | `a × b ÷ c`, rounded down | `u64` or `u128`, all three alike | same | `c` is zero; the quotient does not fit |
| `MUL_DIV_CEIL` | `a × b ÷ c`, rounded up | as above | same | as above |
| `REM` | `a mod b` | `u64`, `i64` or `u128`, alike | same | `b` is zero; `i64::MIN mod −1` |
| `SHL` | `a << b` | `a`: `u64` or `u128`; `b`: `u64` | type of `a` | a set bit would be shifted out |
| `SHR` | `a >> b` | as above | type of `a` | never (a shift of the width or more gives 0) |
| `BIT_AND`, `BIT_OR`, `BIT_XOR` | `a op b` | `u64` or `u128`, alike | same | never |
| `POW10` | `10^a` | `a`: `u64` | `u128` | `a > 38` |
| `READ_I32` | read width 4, sign-extended | like the other reads | `i64` | like the other reads |

- The product in `MUL_DIV` is exact. For `u64` it is computed in `u128`. For `u128` it is
  computed as 256 bits: a fast path when the high half is zero, otherwise a division of four
  64-bit limbs by two (Knuth D). It lives in an `#[inline(never)]` helper and is property-tested
  against a slow bit-by-bit reference.
- `REM` follows Rust: the result takes the sign of the dividend.
- Shifting is checked the way the rest of the language is: `SHL` never drops a set bit
  silently. `SHR` is floor division by a power of two, the same rounding `DIV` already has.
- Rounding gets its own opcode (`MUL_DIV_CEIL`), because the verifier allows the flags byte
  only on read opcodes.
- `READ_I32` is a read opcode, so it also works as the width selector for `RETURN_DATA` and for
  the instruction-data read below. The verifier's result-type tables for reads and
  `RETURN_DATA` map it to `i64`.
- TypeScript: `expression.multiplyDivide(a, b, c, rounding?: 'down' | 'up')`,
  `expression.remainder`, `expression.shiftLeft`, `expression.shiftRight`, `expression.bitAnd`,
  `expression.bitOr`, `expression.bitXor`, `expression.powerOfTen`, and read type `'i32'`. The
  bitwise names avoid the existing boolean `and`/`or`, which the compiler also flattens as
  conjunctions.

## 2. Loops

**Count loops.** `REPEAT` runs its body `count` times, where `count` is a `u64` register read
once, at loop entry.

| Field | Meaning |
| --- | --- |
| `a` | body length, 1 to 255 records |
| `b` | the `u64` count register |
| `c` | static maximum iterations, 1 to 255 |
| immediate | carry mask, as `FOREACH` |

A count above `c` fails the run with `LoopCountExceeded`. The body has the same rules as a
`FOREACH` body: forward only, no nested loop, carried registers keep their type, and
`loopIndex` is available. It has no rows, so row accounts and row inputs are rejected inside it.

**More than one loop.** A template may hold up to eight top-level loops, run one after another
and never nested. Any `FOREACH` iterates the same batch rows. A template with a batch has at
least one `FOREACH`, and one without a batch has none.

The worst case becomes the sum over loops of (invocations in the body × that loop's maximum),
plus the invocations outside loops. It must stay at or below 64.

**Verifier.** The single `in_loop` flag splits into "inside a loop" (`loopIndex` allowed) and
"inside a row loop" (row accounts and row inputs allowed). Today one flag means both, and in a
template with both kinds of loop that would let a count-loop body name a row.

**Executor.** The batch state from `cu/interpreter` (`enter_batch`, `next_iteration`,
`finish_batch`) generalises to a loop with a kind, rows or count. Every loop entry reuses one
register-snapshot buffer rather than allocating another, because the bump heap never frees.

**Unchanged.** The run event's `iterations` byte still reports batch rows.

**TypeScript.** `step.repeat(count, steps, { max, carry?, label? })`. `forEach` may appear more
than once. `inspectTemplate` recognises `REPEAT` and multiple loops when it recomputes the
worst-case CPI count.

## 3. Output

| Opcode | Operands | Effect |
| --- | --- | --- |
| `EMIT` | immediate: data-segment range | Encodes the segments and logs them with `sol_log_data`, as one field |
| `SET_RETURN_DATA` | immediate: data-segment range | Encodes the segments and sets them as the run's return data |

- Parts are built exactly like CPI data: literal bytes plus register encodings, and the same
  width-sum rule proves the worst-case length. Both lengths are capped at 1,024 bytes, the
  return-data limit, which also keeps a log line bounded.
- `SET_RETURN_DATA` may appear at most once, never inside a loop, and no `INVOKE` may follow it.
  Solana clears return data at the start of every program invocation (Agave
  `invoke_context.rs`), so a later CPI would silently erase it. `EMIT` may appear anywhere,
  loops included.
- Both encode into a dedicated output buffer. That buffer is reserved once per run, on the heap,
  at the largest output length. They must not use the CPI scratch buffer: inside a loop it
  can hold a cached, loop-invariant payload that a later CPI reuses without re-encoding.
- `cu-profile` builds overwrite return data after the run with the profile record. That is
  acceptable for a measurement-only feature and is documented in the profile code.
- TypeScript: `step.emit(parts, label?)` and `step.setReturnData(parts, label?)`, with parts
  from `data.literal` and `data.encode`.

## 4. Introspection and bytes

The template declares the Instructions sysvar as an account pinned to
`Sysvar1nstructions1111111111111111111111111`. Every introspection opcode names that account in
`a`, and the verifier rejects one whose constraint does not pin that address
(`InvalidIntrospection`). The sysvar is read-only and constant for the instruction, so reads
borrow it without copying.

| Opcode | Operands | Result |
| --- | --- | --- |
| `INSTRUCTION_COUNT` | `a` | `u64` |
| `INSTRUCTION_INDEX` | `a` | `u64`, the index of the instruction running Ballista |
| `INSTRUCTION_PROGRAM` | `a`, `b` index | `pubkey` |
| `INSTRUCTION_ACCOUNT_COUNT` | `a`, `b` index | `u64` |
| `INSTRUCTION_ACCOUNT` | `a`, `b` index, `c` position | `pubkey` |
| `INSTRUCTION_ACCOUNT_FLAGS` | `a`, `b` index, `c` position | `u64`: bit 0 signer, bit 1 writable |
| `INSTRUCTION_DATA_LEN` | `a`, `b` index | `u64` |
| `READ_INSTRUCTION_DATA` | `a`, `b` index, `c` offset; immediate: a read opcode as the width selector | that read's type |
| `READ_INSTRUCTION_BYTES` | `a`, `b` index, `c` offset; immediate: length, 1 to 1,024 | `bytes` of exactly that length |
| `READ_ACCOUNT_BYTES` | `a` account, `b` offset; immediate: length, 1 to 1,024 | `bytes` of exactly that length |
| `BYTES_LEN` | `a` bytes register | `u64` |

- `b`, `c` and offsets are `u64` registers. An index, position or byte range outside what exists
  fails the run with `InstructionOutOfRange`.
- The parsing uses `pinocchio::sysvars::instructions`, which checks the instruction index and
  the account position. Data ranges are checked in Ballista.
- `READ_ACCOUNT_BYTES` requires the account to be read-only in the transaction
  (`WritableAccountBytesRead` otherwise). A read-only account cannot change or be reallocated
  during the transaction, so the slice stays valid across CPIs without a copy. A writable
  account's data can change under a CPI, and holding a borrow would make that CPI fail.
- `BYTES_LEN` exists so a runtime-length `bytes` value can be forwarded with an explicit Borsh
  length prefix.
- TypeScript, where `sysvar` is the declared account:
  - Opcodes: `expression.instructionCount(sysvar)`, `currentInstructionIndex(sysvar)`,
    `instructionProgram(sysvar, index)`, `instructionAccountCount(sysvar, index)`,
    `instructionAccount(sysvar, index, position)`, `instructionDataLength(sysvar, index)`,
    `instructionData(sysvar, index, offset, type)`,
    `instructionDataBytes(sysvar, index, offset, length)`,
    `accountDataBytes(account, offset, length)` and `bytesLength(value)`.
  - `instructionAccountIsSigner` and `instructionAccountIsWritable` are compile-time sugar over
    the flags and `bitAnd`.
  - `INSTRUCTIONS_SYSVAR_ADDRESS_BYTES` is exported.
- **Signed messages.** An SDK helper, `ed25519Signature({ sysvar, index, signer, messageLength })`,
  returns the requirement steps and a field reader. The steps require that:
  - instruction `index` is the Ed25519 program;
  - it holds exactly one signature;
  - all three of its offsets refer to that same instruction (`u16::MAX`);
  - the public key at the key offset equals `signer`;
  - the message is `messageLength` bytes.

  `field(offset, type)` then reads a value from the signed message. The runtime has already
  verified the signature before any program runs, so these checks bind that verified signature
  to this template's inputs. No new opcode is needed.

## New error kinds

- **Runtime:** `LoopCountExceeded`, `InstructionOutOfRange`, `WritableAccountBytesRead`. Each
  carries the program counter as context.
- **Verifier:**
  - `InvalidLoop`, for count-loop shape, the loop limit, or rows inside a count loop.
  - `InvalidOutput`, for output length, `SET_RETURN_DATA` placement or repetition.
  - `InvalidIntrospection`, for an unpinned sysvar account.
- **Codes:** each is appended after the last existing code.
- **Tables to update:** every error-name table (the Rust enums, `wire.rs`, `errors.ts`, the
  `.txt` fixtures), the TypeScript and Rust decoders, and the tests that assert the first unused
  code decodes to nothing.

## Opcode numbers

39 stays unassigned; tests use it as the unknown opcode.

| 51 `MUL_DIV` | 52 `MUL_DIV_CEIL` | 53 `REM` | 54 `SHL` | 55 `SHR` | 56 `BIT_AND` |
| --- | --- | --- | --- | --- | --- |
| **57** `BIT_OR` | **58** `BIT_XOR` | **59** `POW10` | **60** `READ_I32` | **61** `REPEAT` | **62** `EMIT` |
| **63** `SET_RETURN_DATA` | **64** `INSTRUCTION_COUNT` | **65** `INSTRUCTION_INDEX` | **66** `INSTRUCTION_PROGRAM` | **67** `INSTRUCTION_ACCOUNT_COUNT` | **68** `INSTRUCTION_ACCOUNT` |
| **69** `INSTRUCTION_ACCOUNT_FLAGS` | **70** `INSTRUCTION_DATA_LEN` | **71** `READ_INSTRUCTION_DATA` | **72** `READ_INSTRUCTION_BYTES` | **73** `READ_ACCOUNT_BYTES` | **74** `BYTES_LEN` |

## Testing

Each feature lands with:

- **Verifier table tests** for every new rule and rejection. This includes the updated unknown
  opcode and mistyped-operand sweep.
- **Executor unit tests** for the pure helpers (`mul_div`, `rem`, shifts, `pow10`, `i32`
  decoding). `mul_div` is property-tested against a bit-by-bit reference over the full `u128`
  range.
- **The program generator** (`generate.rs`) emits the pure math ops, count loops and multiple
  loops, so the "executor accepts everything the verifier accepts" property covers them.
- **TypeScript compiler tests** and a new **opcode parity test** that reads the `OP_*`
  constants from `wire.rs` and compares them with the compiler's table. Today nothing catches
  drift between them.
- **TypeScript fixtures** that exercise every new opcode, verified by the Rust suite and run end
  to end under Mollusk:
  - **Math:** the upgraded oracle check (below).
  - **Output:** the log is captured with Mollusk's `LogCollector`, and return data is read
    directly and by a nested template through `RETURN_DATA`.
  - **Introspection:** `process_transaction_instructions`, with Memo instructions as neighbours.
  - **Signed messages:** a real Ed25519 instruction. Mollusk's `precompiles` feature verifies
    it, so a tampered signature fails before Ballista runs, and a valid one reaches the helper's
    checks.
- **Certora:**
  - Extend `writes_destination` for `EMIT`, `SET_RETURN_DATA` and `REPEAT`.
  - Extend the verifier error-code rule's match and the `value_dependent` list.
  - Add `u64` rules for `mul_div`, `rem`, the shifts and the bitwise ops alongside the existing
    arithmetic rules.
  - Check that no new frame exceeds 4 KiB.
- **Compute-unit ceilings:** any regression from new dispatch arms is measured and, if real,
  raised by hand in the same commit with the numbers. Otherwise the ratchet only moves down.

## Examples and docs

- **Oracle example.** `jupiter-oracle-checked-swap` reads both mints' decimals and the feed's
  exponent (`i32`), checks each token account's mint, and computes the floor with `powerOfTen`
  and `multiplyDivide`. The caller no longer supplies a divisor or an exponent.
- **New protocol example.** A signed-quote settlement using `ed25519Signature`.
- **Reference docs:**
  - `language.md`: sources, operators, steps, loops and bounds.
  - `wire-format.md`: opcodes and record meanings.
  - `limits.md`, `typescript.md` and `rust.md`.
- **Guide pages:**
  - `errors-and-events.md`: codes, and events next to `emit`.
  - `formal-verification.md`, `mental-model.md` and `why-ballista.md`, which say "at most one
    loop".
  - `expressions.md` and `scope.md`.

These wait for the in-progress docs rewrite to finish.

## Out of scope

- Delegate signing, persistent state, try/catch around a CPI (impossible on Solana), an `i128`
  type, secp256k1 or secp256r1 helpers, variable-length byte reads, and building new `bytes`
  values at run time.

## Order of work

Each phase ends with every CI check green: fixtures unchanged after `pnpm fixtures`,
`pnpm check`, `pnpm test`, the SBF build and Mollusk suite, and the Certora typecheck.

1. **Math**, then the oracle example upgrade.
2. **Loops.**
3. **Output.**
4. **Introspection and bytes**, then the Ed25519 helper and the signed-quote example.
5. **Docs**, and a final compute-unit and verification pass.
