# Limits

Every maximum on a template and on a run, in one place. The values come from
`common/src/template/wire.rs`. Terms such as CPI, row and account group are in the
[Glossary](/reference/glossary), and the rules behind each limit in the
[language reference](/reference/language).

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
| Programs per group filter | 2 |
| Matches per group filter | 4 (minimum 1) |
| Except keys per group filter | 4 |
| Group filter match offset | 65,535 |

One transaction holds fewer: about 61 runtime accounts. See
[accounts per transaction](#accounts-per-transaction).

### CPIs

| Limit | Maximum |
| --- | ---: |
| CPIs per run, counting every time a loop body runs and 3 for each registry open | 64 |
| Instruction data per CPI | 4,096 bytes |
| Readable CPI return data | 1,024 bytes |

This limit counts only Ballista's own calls. Solana's [instruction trace](#instruction-trace),
which also counts the run itself and every nested call, binds first.

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

An `emit` tag is at least 4 bytes. The tag rule is under [Output](/reference/language#output).

### Registries

| Limit | Maximum |
| --- | ---: |
| Registries per template | 8 |
| Entries a template opens | 8 |
| Field bytes per registry | 512 (minimum 1) |
| Entry account: 72-byte header plus fields | 584 bytes |
| CPIs each open counts toward the 64 per run | 3 |

Fields take `bool` 1 byte, `u64` and `i64` 8, `u128` 16 and `pubkey` 32. An entry's rent on
mainnet is (128 + the entry's bytes) × 5,080 lamports: 1,097,280 for 16 bytes of fields, 88 bytes
in all. See [what it costs](/guide/why-ballista#cost). The rules are under
[Registries](/reference/language#registries).

### Sizes and bytecode

| Limit | Maximum |
| --- | ---: |
| Compiled template | 10,160 bytes (a CPI can allocate 10,240, less the 80-byte account header) |
| [Registers](#registers) | 64 |
| VM instructions | 128 (minimum 1) |
| [PDA](/reference/glossary#pda) seeds per derivation, not counting the bump | 15 |
| Bytes per PDA seed | 32 |

### Registers {#registers}

A template has 64 [registers](/reference/glossary#register), each holding one value during a run.
Both SDK compilers give each value its own register:

- each fixed input the steps read, and each distinct constant, loaded once before the first step;
- each value an expression reads or computes: an account read, the clock, a row input, a sum, a
  comparison, a cast.

A `let` takes none of its own: it names its value's register. Writing the same read or sum twice
computes it twice, so give a value used more than once a `let`. A loop body compiles once, so its
registers count once, however many passes it makes.

Past 64, the compiler reuses a register once its value has been read for the last time. Inputs
and constants hold theirs from the start of the run, and a value read inside a loop holds its
register for the whole loop. Compilation fails only when more than 64 values are in use at once:
`Template uses more than 64 registers: N values are in use at once at steps[k] (label)`.

`compileTemplate(template).stats.registers` (`compiled.stats.registers` in Rust) gives the count
after reuse, and `stats.instructions` the VM instructions, at most 128: each value above takes one,
and so do most other steps, such as a `require` or a call.

## SDK limits

Both SDK compilers add their own maximums. A template built by hand or with other tools is bound
only by the program limits above.

| Limit | Maximum |
| --- | ---: |
| Batch rows (`maxIterations`) | 60 |
| Steps in a loop body | 64 |
| Data parts per CPI | 64 |
| Data parts per `emit` or `setReturnData` | 64 |
| Step label length | 64 characters |

Without the SDK's 60-row cap, the number of rows is still bounded: fixed accounts plus the row width
times the maximum rows must fit in 120 runtime accounts, and the worst-case CPI count, with every
loop at its maximum, must fit in 64.

## Transaction limits {#transaction-ceilings}

Solana's own limits often bind before Ballista's.

### Transaction size {#transaction-size}

A legacy or version 0 transaction holds at most **1,232 bytes**, and each upload instruction goes
in a transaction of its own. With the creator as fee payer, these are the largest that fit:

| Transaction | Largest upload chunk | Largest template uploaded in one shot |
| --- | ---: | ---: |
| Legacy | 1,023 bytes | 960 bytes |
| Version 0 | 1,021 bytes | 958 bytes |

One byte more makes 1,233. Larger templates upload in chunks. In one shot, the version 0
transaction would be 1,288 bytes for the [daily cap](/examples/protocols/daily-cap), 1,636 for the
[signed quote](/examples/protocols/signed-quote) and 1,866 for the
[oracle swap](/examples/protocols/jupiter-oracle-swap). In TypeScript, give
`buildKitTemplateUploadPlan` the `transactionMessage` you send, and it fits each instruction to it
([Lifecycle](/reference/typescript#lifecycle)).

### Accounts per transaction

A transaction uses at most **64 accounts**, whatever its version. Accounts loaded from address
lookup tables count too: a table makes a transaction smaller, not wider. Solana's feature to raise
the limit to 128, `increase_tx_account_lock_limit`, is not active on mainnet.

So a run alone in its transaction fits about **61 runtime accounts**:

- the Ballista program and the template account take two of the 64;
- the fee payer takes a third, unless it is also one of the run's accounts;
- every other instruction takes the accounts it adds, such as the Compute Budget program.

So Ballista's limit of 120 runtime accounts is reached only by passing some addresses in more than
one slot, such as an account group that lists one account several times.

To carry 61 accounts in bytes, use a version 1 transaction, up to 4,096 bytes, or a version 0
transaction, up to 1,232 bytes, with an address lookup table. A version 1 transaction needs its
compute-unit and loaded-data limits set in the message: without them the run fails with
`MaxLoadedAccountsDataSizeExceeded` or gets no compute units. `createComputeUnitProvider` sets
both.

### Instruction trace {#instruction-trace}

A transaction runs at most **64 instructions in total**: its own instructions plus every CPI at
every depth, including the calls a called program makes itself. The run is one of them, so even
alone in its transaction a run can make at most 63 CPIs. A 65th call fails with
`MaxInstructionTraceLengthExceeded`; a transaction of 65 instructions of its own is refused before
it runs. Ballista's count can't see nested calls,
so size a loop to the trace, not only to its `max`.

In a local test against copies of the mainnet programs, a template sells SOL through Jupiter in
slices, one per pass of a count loop, then pays out rows. The transaction spends 15 entries before
the first slice (compute budget, Jupiter's setup and its CPIs, and the run), 7 per slice (Jupiter,
the pool, their event calls and two token transfers) and 1 per payout. Seven slices fill the trace
exactly, 15 + 7 × 7 = 64, so the first payout would be entry 65 and fails with
`MaxInstructionTraceLengthExceeded`, although the loop allows 8.

### Call depth {#call-depth}

Solana nests calls at most **5 frames** deep. The transaction's own instruction is frame 1, and
each CPI runs one frame below its caller. SIMD-0268 would raise this to 9; it is not active on
mainnet.

- A run called by the transaction is frame 1, so the programs a template calls run at frame 2,
  and their own calls at frames 3 to 5.
- A template that [runs another template](/examples/composition#run-another-template) puts the
  inner run at frame 2, and the inner run's calls at frame 3.
- A call into frame 6 fails the transaction with Solana's `CallDepth` error, not a Ballista code.

In a local test, a run that runs a Jupiter sale reaches exactly frame 5: the outer run, the inner
run, Jupiter, Meteora DLMM, then the Token program. A deeper venue, a transfer hook or one more
wrapper doesn't fit.

### Compute

Compute cost depends on the template: PDA bump searches, account reads, loop passes, logs, CPIs and
the programs they call all add to it. So does creating a registry entry, which derives the entry's
address and calls the System program. Solana's log call alone charges an `emit` 200
[compute units](/reference/glossary#compute-units) plus 1 per byte. Simulate the exact transaction
and set the compute-unit limit from the measurement plus a margin. The TypeScript SDK's
`createComputeUnitProvider` does this.

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
