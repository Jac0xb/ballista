# Limits

Every maximum on a template and on a run, in one place. The values come from
`common/src/template/wire.rs`. Terms such as CPI, row and account group are in the
[Glossary](/reference/glossary).

## Program limits

The Ballista program enforces these, whichever SDK built the template.

### Accounts

| Limit | Maximum |
| --- | ---: |
| Runtime accounts per run (fixed + batch rows + group members) | 120 |
| Accounts per batch row | 8 (minimum 1) |
| Account groups per template | 8 |
| Members per account group | 255 |
| Accounts per CPI, including a forwarded account group | 64 |

### CPIs

| Limit | Maximum |
| --- | ---: |
| CPIs per run, counting every time a loop body runs and 3 for each registry open | 64 |
| Instruction data per CPI | 4,096 bytes |
| Readable CPI return data | 1,024 bytes |

### Inputs

| Limit | Maximum |
| --- | ---: |
| Inputs, fixed plus row | 32 |
| Row inputs per batch row | 8 |
| Input values per run, fixed plus every row | 256 |
| Run data (group lengths plus input values) | 1,024 bytes |
| One `bytes` input or literal | 1,024 bytes |

### Loops

| Limit | Maximum |
| --- | ---: |
| Loops per template (`forEach` and `repeat` together), not nested | 8 |
| Count-loop maximum (`max`) | 255 (minimum 1) |

### Output and byte reads

| Limit | Maximum |
| --- | ---: |
| One log (`emit`) or return data (`setReturnData`), worst case | 1,024 bytes |
| Return-data steps per template | 1 |
| One byte read from account or instruction data | 1,024 bytes (minimum 1) |

An `emit` must also start with a literal tag of at least 4 bytes that does not start with `BEV`.

### Registries

| Limit | Maximum |
| --- | ---: |
| Registries per template | 8 |
| Entries a template opens | 8 |
| Field bytes per registry | 512 (minimum 1) |
| Entry account: 72-byte header plus fields | 584 bytes |
| CPIs each open counts toward the 64 per run | 3 |

There is no separate limit on fields. Their widths (`bool` 1 byte, `u64` and `i64` 8, `u128` 16,
`pubkey` 32) must total 1 to 512 bytes. Entries open only at the top level, never in a loop, so a
template cannot open one per batch row.

Creating an entry costs rent: the lamports Solana requires an account to hold for its size. The
payer, a signing account the template names, pays it once, the first time a run opens the entry,
and pays only the shortfall if the address already holds lamports. Later runs pay nothing. Entries
are never closed, so the rent is never returned. An entry with 16 bytes of fields, 88 bytes in all,
needs 1,503,360 lamports.

### Sizes and bytecode

| Limit | Maximum |
| --- | ---: |
| Compiled template | 10,240 bytes |
| Registers | 64 |
| VM instructions | 128 (minimum 1) |
| PDA seeds per derivation, not counting the bump | 15 |
| Bytes per PDA seed | 32 |

## TypeScript SDK limits

The TypeScript SDK adds its own maximums. A template built with the Rust `ProgramBuilder` is bound
only by the program limits above.

| Limit | Maximum |
| --- | ---: |
| Batch rows (`maxIterations`) | 60 |
| Top-level steps | 128 |
| Steps in a loop body | 64 |
| Data parts per CPI | 64 |
| Data parts per `emit` or `setReturnData` | 64 |
| Step label length | 64 characters |

Without the SDK's 60-row cap, the number of rows is still bounded: fixed accounts plus the row width
times the maximum rows must fit in 120 runtime accounts, and the worst-case CPI count, with every
loop at its maximum, must fit in 64.

## Transaction limits {#transaction-ceilings}

Solana's own limits often bind before Ballista's.

### Accounts per transaction

- A version 1 transaction lists at most **64 account addresses** and cannot use address lookup
  tables. The template account and the Ballista program take two of them, and the fee payer a third
  unless it is also a runtime account. In practice a v1 run fits about **60 runtime accounts**.
- Runs larger than that, up to Ballista's 120, need a version 0 transaction with an address lookup
  table. A v0 transaction is limited to 1,232 bytes.
- Accounts used by other instructions in the same transaction count toward the same 64.

See [Transaction v1](/guide/transaction-v1) for how to build one.

### Size and compute

- A v1 transaction can be up to 4,096 bytes. The account and compute limits usually run out first.
- Compute cost depends on the template: PDA bump searches, account reads, loop passes, logs, CPIs
  and the programs they call all add to it. So does creating a registry entry, which derives the
  entry's address and calls the System program. Solana's log call alone charges an `emit` 200
  compute units plus 1 per byte. Simulate the exact transaction and set the compute-unit limit
  from the measurement plus a margin. The TypeScript SDK's `createComputeUnitProvider` does this.

## When each limit is checked {#static-versus-runtime}

**Before any run**, the compiler and the program's verifier (at create or finalize) reject a
template that could exceed a limit in its worst case: register, instruction, and loop counts,
inputs and account groups, runtime accounts at the maximum rows, CPIs with every loop at its
maximum, accounts listed per CPI, PDA seeds, the largest instruction data each CPI could build, the
largest log or return data each output could build, byte-read lengths, fixed-offset reads past an
account's declared minimum length, and registry indexes, sizes, opens, and field ranges.

**On every run**, `Run` checks what the caller supplies: the run data and each input value, the
number of runtime accounts, the row count (it must divide evenly and fall between the minimum and
maximum), account group sizes, each fixed and row account against its declaration, each count
loop's count against its maximum, each CPI's accounts plus its forwarded group against 64, and each
registry entry against its template, registry, and key.

## Heap

A run allocates its inputs, its registers and one set of CPI buffers, plus, once needed, one
register snapshot shared by its loops and one 1,024-byte buffer shared by its logs and return
data. It reuses each of them, so heap use does not grow with the number of CPIs, loops, or outputs,
and stays within Solana's default 32 KiB even for 64 CPIs of 4 KiB each.

If a template nears several limits at once, split it. Smaller templates are easier to review.
