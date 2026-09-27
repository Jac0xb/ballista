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
| CPIs per run, counting every loop row | 64 |
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

### Sizes and bytecode

| Limit | Maximum |
| --- | ---: |
| Compiled template | 10,240 bytes |
| Registers | 64 |
| VM instructions | 128 (minimum 1) |
| Loops per template | 1, not nested |
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
| Step label length | 64 characters |

Without the SDK's 60-row cap, the number of rows is still bounded: fixed accounts plus the row width
times the maximum rows must fit in 120 runtime accounts, and the invokes in the loop times the
maximum rows must fit in 64 CPIs.

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
- Compute cost depends on the template: PDA bump searches, account reads, CPIs and the programs
  they call all add to it. Simulate the exact transaction and set the compute-unit limit from the
  measurement plus a margin. The TypeScript SDK's `createComputeUnitProvider` does this.

## When each limit is checked {#static-versus-runtime}

**Before any run**, the compiler and the program's verifier (at create or finalize) reject a
template that could exceed a limit in its worst case: register and instruction counts, inputs and
account groups, runtime accounts at the maximum rows, CPIs with the loop at its maximum, accounts
listed per CPI, PDA seeds, the largest instruction data each CPI could build, and fixed-offset reads
past an account's declared minimum length.

**On every run**, `Run` checks what the caller supplies: the run data and each input value, the
number of runtime accounts, the row count (it must divide evenly and fall between the minimum and
maximum), account group sizes, each fixed and row account against its declaration, and each CPI's
accounts plus its forwarded group against 64.

## Heap

A run allocates its inputs, its registers and one set of CPI buffers, and reuses those buffers for
every CPI. Heap use does not grow with the number of CPIs, and stays within Solana's default 32 KiB
even for 64 CPIs of 4 KiB each.

If a template nears several limits at once, split it. Smaller templates are easier to review.
