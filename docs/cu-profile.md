# Where the compute goes

Solana counts the work a program does in **compute units**, and every transaction has a limit on
how many it can use. This page shows where a Ballista run spends them. For what each example costs
against sending the same calls yourself, see [What a run costs](/benchmarks).

In brief: every run starts with about 1,000 units of fixed work. After that, calls to other
programs cost the most, and most of what a call costs is charged by Solana, not by Ballista. The
other large cost is deriving a PDA, an account address computed from a program's ID instead of
from a key pair. Everything else is small.

All figures come from running the compiled program in [Mollusk](https://github.com/anza-xyz/mollusk),
Anza's harness for testing Solana programs, on the Agave 4.1 runtime.

## Cost of each feature

Each figure below is the difference between two templates that are identical except for how many
times they do one thing. The fixed cost of a run cancels out, and what is left is the cost of one
more of that thing. Two words in the table are Ballista's own: a **register** is one of the
numbered slots that hold a template's working values, and an **invocation** is a call to another
program.

<!-- profile:table -->

| Measurement | Compute units | What it covers |
| --- | ---: | --- |
| **Run** | | |
| Fixed cost of any run | 1,014 | parse, validate, allocate, one assertion |
| **Instructions** | | |
| PDA derivation | 4,852 | one literal seed, bump search |
| PDA derivation, supplied bump | 1,898 | one literal seed, no search |
| Clock timestamp | 207 | sysvar read |
| Clock slot | 204 | sysvar read |
| Account data read, dynamic offset | 186 | offset from a register |
| Account data read | 174 | u64 at a fixed offset |
| Checked add | 164 | u64 + u64 |
| Comparison | 158 | u64 < u64 |
| Cast | 126 | u64 to u128 |
| Boolean and | 119 | bool && bool |
| Account key read | 115 | 32 bytes into a register |
| Move | 105 | copy a register |
| Account emptiness read | 104 | bool header field |
| Account lamports read | 103 | u64 header field |
| Account length read | 103 | u64 header field |
| Constant | 86 | const u64 |
| Assertion | 80 | reads a bool register and continues |
| **Invocations** | | |
| Cross-program invocation | 1,730 | System transfer, 2 accounts, 12 bytes |
| Account passed to an invocation | 136 | resolved and given a meta |
| Byte of invocation data | under 0.01 | measured across 1,000 extra bytes |
| **Batches** | | |
| Iteration with one invocation | 1,671 | the shape a payroll run repeats |
| Iteration, three instructions | 478 | one account read, compare, assert |
| Fixed invocation account, per iteration | 57 | resolved again every row |
| Declared register, per iteration | under 0.01 | restored from the pre-loop snapshot whether or not the body writes it |
| **Accounts** | | |
| Account pinned to an address | 67 | 32-byte comparison |
| Account pinned to an owner | 67 | 32-byte comparison |
| Account with a minimum length | 46 | length comparison |
| Unconstrained account | 46 | declared and supplied, nothing checked |
| **Inputs** | | |
| 32-byte bytes input | 78 | decoded before execution |
| pubkey input | 78 | decoded before execution |
| u64 input | 63 | decoded before execution |

<!-- /profile -->

**Fixed cost.** The smallest possible template checks a constant and does nothing else. It costs
1,014 units. Every run pays at least this much to load the template, check the accounts passed to
it and read its inputs.

**Calls.** A call from one program to another is a cross-program invocation, or CPI. A SOL
transfer costs 1,730 units when a template makes it. Most of that is not Ballista's: the Solana
runtime charges a flat 946 units for every CPI, and the System Program spends 150 on the transfer,
the same as it would for a plain instruction. The other 630 or so is Ballista reading the call's
description from the template, looking up its accounts, assembling its data and handing it over.
Each extra account passed to a call adds 136 units, to look it up and add it to the call's account
list. The size of the call's data barely matters: 1,000 extra bytes added less than 10 units.

**PDAs.** A PDA (program derived address) is an account address computed from a program's ID and a
few chosen values, called seeds, instead of from a key pair. To find it, the runtime adds a
one-byte **bump** to the seeds, trying 255 first and counting down until the result is a valid
PDA, and it charges 1,500 units for every attempt. That search makes deriving a PDA the most
expensive single step: 4,852 units in the table, for one seed written into the template as a fixed
value (a literal). Given the bump, the template makes one attempt: 1,898 units, whatever the bump
is. The check then proves less: the address comes from those seeds and that bump, which need not be
the canonical one. [PDA and ATA assertions](/guide/pda-assertions) shows how to pass the bump and
when that matters.

**Everything else is small.** A step between calls costs 80 to 210 units: a comparison, a sum,
reading the clock, or reading a field from an account, such as its balance in lamports (the
smallest unit of SOL). Each account the template declares costs 46 to 67 units to check, and each
input 63 to 78 to decode.

**Batches.** A batch repeats the same steps for each row of accounts. A row that makes one SOL
transfer costs 1,671 units, and a row that reads an account and checks it, without a call, costs
478. When a batch makes the same call on every row, Ballista keeps the call's account list, and its
data if that does not change, from one row to the next, and swaps in only the row's own accounts.
An account that is the same on every row still costs 57 units per row, because it is passed to the
call on every row. At the start of each row, Ballista resets the template's working values to where
they stood before the loop, apart from any the template keeps from row to row, such as a running
total. That reset costs under 0.01 units per value.

## Cost of each phase of a run

The table above prices one feature at a time. The tables below follow four runs from start to
finish: the smallest possible template, a payroll batch of one row and of 30 rows (one SOL
transfer per row), and a batch that adds up the balances of 30 accounts without making any calls.
Each table starts with the run's total and its number of calls, which the tables call invokes. The
stages are:

- **Entrypoint and account deserialization**: Solana starts the program, which reads the list of
  accounts passed to the instruction. This grows with the number of accounts.
- **Instruction parse and dispatch**: working out which Ballista instruction was sent.
- **Template account load and header parse**: checking the template account and reading the header
  at the start of the template.
- **Runtime account validation**: checking each account passed to the run against what the
  template declares for it, such as its address, owner or size.
- **Run input parse**: decoding the inputs passed to this run.
- **Register file allocation**: reserving memory for the template's working values, which Ballista
  calls registers.
- **Invocation scratch allocation**: reserving the buffers that every call reuses.
- **Interpreter**: running the template's steps. In runs that make calls, this is split in three.
  **Interpreter, other instructions** is every step except the calls, including the loop.
  **Invoke build: accounts and data** is looking up each call's accounts and assembling its data.
  **Invoke itself: runtime and callee** is the call: Solana's CPI fee, the called program's work,
  and handing the call over.
- **Return and unattributed remainder**: whatever the stages above do not account for.

<!-- profile:phases -->

#### Empty run

1,014 compute units in total, with no invoke.

| Phase | Compute units | Share |
| --- | ---: | ---: |
| Entrypoint and account deserialization | 8 | 0.8% |
| Instruction parse and dispatch | 18 | 1.8% |
| Template account load and header parse | 239 | 23.6% |
| Runtime account validation | 100 | 9.9% |
| Run input parse | 221 | 21.8% |
| Register file allocation | 116 | 11.4% |
| Invocation scratch allocation | 56 | 5.5% |
| Interpreter, including invokes | 233 | 23.0% |
| Return and unattributed remainder | 23 | 2.3% |

#### Payroll, 1 row

3,290 compute units in total, across 1 invoke.

| Phase | Compute units | Share |
| --- | ---: | ---: |
| Entrypoint and account deserialization | 63 | 1.9% |
| Instruction parse and dispatch | 18 | 0.5% |
| Template account load and header parse | 239 | 7.3% |
| Runtime account validation | 239 | 7.3% |
| Run input parse | 303 | 9.2% |
| Register file allocation | 115 | 3.5% |
| Invocation scratch allocation | 66 | 2.0% |
| Interpreter, other instructions | 640 | 19.5% |
| Invoke build: accounts and data | 398 | 12.1% |
| Invoke itself: runtime and callee | 1,210 | 36.8% |
| Return and unattributed remainder | 0 | 0.0% |

#### Payroll, 30 rows

51,741 compute units in total, across 30 invokes.

| Phase | Compute units | Share |
| --- | ---: | ---: |
| Entrypoint and account deserialization | 490 | 0.9% |
| Instruction parse and dispatch | 18 | 0.0% |
| Template account load and header parse | 239 | 0.5% |
| Runtime account validation | 1,399 | 2.7% |
| Run input parse | 303 | 0.6% |
| Register file allocation | 115 | 0.2% |
| Invocation scratch allocation | 66 | 0.1% |
| Interpreter, other instructions | 7,803 | 15.1% |
| Invoke build: accounts and data | 5,270 | 10.2% |
| Invoke itself: runtime and callee | 36,300 | 70.2% |
| Return and unattributed remainder | 0 | 0.0% |

#### Sum 30 rows, no invoke

18,208 compute units in total, with no invoke.

| Phase | Compute units | Share |
| --- | ---: | ---: |
| Entrypoint and account deserialization | 462 | 2.5% |
| Instruction parse and dispatch | 18 | 0.1% |
| Template account load and header parse | 239 | 1.3% |
| Runtime account validation | 1,249 | 6.9% |
| Run input parse | 221 | 1.2% |
| Register file allocation | 168 | 0.9% |
| Invocation scratch allocation | 56 | 0.3% |
| Interpreter, including invokes | 15,805 | 86.8% |
| Return and unattributed remainder | 0 | 0.0% |

<!-- /phases -->

What the tables show:

- **The call itself is the largest stage** in every run that makes calls: 36.8% of the one-row
  payroll and 70.2% of the 30-row payroll. Each SOL transfer costs 1,210 units here, and 946 of
  them are Solana's CPI fee and 150 the System Program's work. A program written by hand to make
  the same calls would pay both. (The 1,730 in the feature table also covers building the call.)
- **Building calls is the largest part Ballista controls.** It takes 398 units for the single call
  in the one-row payroll, and 5,270 for the thirty calls in the 30-row payroll, about 176 each,
  because every row after the first reuses the account list and data.
- **Start-up work is small and does not grow with the batch.** Loading the template, decoding the
  inputs and the two allocations come to 723 units in both payrolls. What grows with the batch is
  the work per account: reading the accounts at the start, and checking them.
- **Without calls, running the steps is most of the run**: 86.8% of the 30-row sum.

### How the phases are measured

The program has an optional `cu-profile` build feature. With it on, the program reads how much of
its compute budget is left (the `sol_remaining_compute_units` system call) at each stage boundary
and around each call, and returns the readings as the instruction's return data. Each reading
costs 115 units, measured in the same run, and that is subtracted from every stage. The test in
`tests/ballista/src/phases.rs` runs each case on both the profiling build and the normal build.
The stages come from the profiling build; the totals and percentages come from the normal build,
and anything the stages leave over is shown as the remainder. With the feature off, none of the
readings are compiled into the program.

## Whole runs, tracked over time

`pnpm cu:bench` runs [mollusk-svm-bencher](https://docs.rs/mollusk-svm-bencher), the compute-unit
bencher that comes with Mollusk, over nine fixed runs. It adds the results to
`benches/compute_units.md`, with the change since the previous results. That file is committed, so
any change to the program's compute cost shows up in review. The runs are defined in
`tests/ballista/src/cases.rs`. The latest results:

| Run | Compute units |
| --- | ---: |
| Smallest possible template: one check, no calls | 1,014 |
| One SOL transfer | 2,938 |
| 8 SOL transfers in a batch | 14,989 |
| 30 SOL transfers in a batch | 51,741 |
| Add up the balances of 30 accounts, no calls | 18,208 |
| Compare a value in an account with two inputs, no calls | 1,932 |
| Derive one PDA, searching for the bump | 6,015 |
| Derive one PDA, bump supplied | 3,147 |
| Upload a 30-row payroll template | 6,817 |

## Regression checks

The integration tests (`pnpm test:integration`) fail when a run uses more compute than the limit
recorded for it. `fixtures/cu-ceilings.json` holds a limit for each tracked run above, and
`fixtures/example-ceilings.json` one for every example. Updating the recorded numbers
(`pnpm cu:ceilings` for the tracked runs, `pnpm benchmarks` for the examples) lowers a limit when a
change makes a run cheaper, but never raises one. An increase has to be edited into the file by
hand, where a reviewer sees it.

The tracked runs and the examples use fixed account addresses. Every bump search therefore takes
the same number of attempts, and costs the same, on every run and every machine.

## Reproducing the numbers

| Numbers | Command |
| --- | --- |
| Cost of each feature, and every example's cost table | `pnpm benchmarks` |
| Cost of each phase | `pnpm cu:phases` |
| Tracked whole runs | `pnpm cu:bench` |
| Check that nothing got more expensive | `pnpm test:integration` |

`pnpm cu:phases` builds the two versions of the program it needs. The others measure the program
in `target/deploy/`, so build it first with `pnpm build:program`. `pnpm benchmarks` and
`pnpm cu:phases` finish by rewriting every generated table in the docs.
