# Compute profile

Three measurements of the same program, each answering a different question.

| Tool | Question it answers | How to run it |
| --- | --- | --- |
| Marginal profile | What does one more of *this* cost? | `pnpm benchmarks` |
| Phase profile | Where does a run's budget actually go? | `pnpm cu:phases` |
| [Mollusk bencher](https://docs.rs/mollusk-svm-bencher) | Did this change move any whole instruction? | `pnpm cu:bench` |

The marginal profile is the table below. The phase profile splits one run into the stages it
passes through. The bencher is Anza's own compute-unit bencher, the one most Solana programs use;
it writes `benches/compute_units.md` with a delta column against the last committed run, so a
regression shows up as a diff.

## What one more of something costs

Every figure below is a difference between two templates that differ only in how many times they
do one thing, so the fixed cost of a run cancels and what is left is the price of that feature.
The harness lives in `tests/ballista/src/profile.rs`.

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

## Reading it

A run starts at about a thousand compute units and then pays for what the template actually does.
Nothing in the interpreter is expensive on its own. Three things dominate any real template:

**Address derivation is the one big cost, unless you supply the bump.** A canonical derivation
hashes with bump 255, then 254, and so on until the result is off-curve, and the runtime charges
1,500 units for every attempt. Pass the bump instead and there is one attempt: about 1,900 units
against about 4,850, and flat however deep the canonical bump happens to be. See
[PDA assertions](/guide/pda-assertions).

**Cross-program invocation is mostly not ours.** Of the units an invocation costs, the runtime
charges a flat 1,000 for the call itself and the callee charges its own. What Ballista adds is the
remainder, spent resolving accounts and building the instruction.

**Everything else is small and linear.** An instruction costs between 85 and 210 units, an account
between 45 and 70 to validate, an input under 80 to decode, and a loop iteration adds almost
nothing beyond the instructions inside it.

## Where a run's budget goes

The program can be built with a `cu-profile` feature that reads `sol_remaining_compute_units` at
each phase boundary and returns the samples as the instruction's return data. The harness in
`tests/ballista/src/phases.rs` runs every case twice, once on that build and once on the ordinary
one, and reports the phases against the ordinary total, so the instrumentation is excluded rather
than smeared across the table.

Reading the counter is itself a syscall, measured at 115 units in the run that uses it, and every
interval has that subtracted. The feature compiles to nothing when it is off: against a build with
the hooks deleted from the source, the ordinary binary has a byte-identical `.text` and differs
only in 31 bytes of panic-location metadata, where the added lines moved some line numbers.

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

Three things stand out. The invoke dominates anything that invokes, and most of it belongs to the
runtime, not to Ballista. Building each invocation — resolving its accounts and encoding its data
— is the largest piece Ballista actually controls. And the fixed prologue is genuinely small: a
template's header parse, input parse and allocation together cost under 700 units no matter how
big the batch is.

## Where the golf is

Every claim here is a measurement, and the ones that did not survive measurement are listed too.

### Landed

**Take the bump as an input.** `expression.pda(program, seeds, bump)` and the `bump` option on
`assertPda` and `assertAta` compile to `CREATE_PDA`, which derives once instead of searching down
from 255. Measured at 1,898 units against 4,852, and the saving grows with the bump's depth: for
an associated token account whose canonical bump is 250, the search costs 11,325 units and the
supplied bump 4,069, flat. The check stays exactly as strong, because a wrong bump either fails
to be a program address or produces a different one, and the comparison rejects it either way.

**Build each invocation once per batch, not once per row.** A batch rebuilt the whole invocation
on every row: resolving every account in its list, and encoding every byte of its data, though
usually only the row account differs. At loop entry the executor now works out what can be kept.
It keeps the account list and re-resolves only the row slots; it keeps the program account; and
when no segment of the data reads a register the body writes, it keeps the encoded bytes too. A
body that invokes more than one descriptor would overwrite the shared buffers, so it opts out and
pays nothing.

That last part needs the compiler's help, which is the next entry.

**Load fixed inputs before the loop, once.** The SDK compiled `expression.input('amount')` where
it was used, so a payroll loaded the same unchanging amount on every row — and because the loop
restores every register its body writes, that load also told the executor the invocation data
might have changed. Fixed inputs now load before the first step and are shared between uses, so
the data cache above actually engages. It also removes a duplicate load when an input is read
twice.

Together, across the cookbook, **28,054 compute units, 5.0%**:

| Template | Before | After | Change |
| --- | ---: | ---: | ---: |
| Existing-account token payroll, 32 rows | 64,497 | 55,183 | **−14.4%** |
| Bounded SOL payroll, 30 rows | 59,368 | 51,741 | **−12.8%** |
| Claim then distribute, 16 rows | 34,844 | 30,432 | −12.7% |
| Close empty token accounts, 16 rows | 37,143 | 35,095 | −5.5% |
| Index-weighted rewards, 30 rows | 73,700 | 70,339 | −4.6% |
| Bounded keeper crank, 24 rows | 51,052 | 50,303 | −1.5% |
| Swap then deposit, no batch | 5,734 | 5,783 | +0.9% |

Deciding what a loop can keep costs a little on every run, which is the +0.9% on templates with
no batch to amortise it. The worst case across all twenty examples is 49 compute units.

**Materialize constants before the loop too, and share them.** A literal inside a batch body was
rebuilt on every row at about 86 units each, and two uses of the same value took two registers.
Constants now compile once, ahead of the first step, keyed by value. No example got worse and
four got materially better: bounded keeper crank **−9.8%**, close empty token accounts −2.9%,
index-weighted rewards −2.8%, basis-point revenue split −1.5%, for another 8,007 units.

Across everything measured here, the cookbook went from 555,835 compute units to 519,774: **6.5%
less for the same twenty templates**, with no change to what any of them guarantee.

**Derive the template address once per upload.** `create_template` computed the template PDA in
its account check and again for the bump. Only 30 units, because the optimizer was already
merging the two syscalls, but the code no longer depends on it noticing.

### Measured and rejected

**Skip the input decoder and the invocation buffers when a template has neither.** Allocation is
a real part of the fixed cost — the register file is 116 units and the three invocation buffers
56, together 17% of the 1,014-unit floor — and a template with no inputs or no invocations could
skip them. Measured on the micro-benchmarks it looked like a 13% cut to the floor. Measured on
the cookbook it was **677 units worse**, because every real template has both inputs and an
invocation, so the fast paths never fire and only the extra branches remain. Reverted. The
lesson is in the method, not the code: a micro-benchmark of a template nobody writes will happily
recommend a regression.

**Restore only the registers a loop body writes.** The loop copies the register file back from a
snapshot on every iteration, which looked like a `memcpy` per row. Replacing it with a mask of
the registers the body writes made a 30-row batch with no invoke **3,004 units worse**: iterating
set bits costs more per register than the copy costs for the whole file. Reverted. The marginal
cost of a declared register per iteration measures 0.00 units, so there is nothing here.

### Still open

**Share repeated account reads within a step.** A template that reads the same account field
twice in one expression pays for both: the oracle band reads its price field twice, 174 units.
Reads cannot be shared across a step boundary, because an invocation in between can change the
account, but within one step they can.

Note that hoisting account reads out of a loop would be wrong for the same reason: a batch that
transfers from one treasury must see the balance the previous row left behind.

**Cache more than one invocation per body.** A body that invokes two different programs — assert
the ATA, create it, then transfer — opts out of all of the above, because the account list and
the data buffer are shared. Giving each descriptor its own slot would extend the win to the
cookbook's most expensive example.

**Fuse compare with assert.** A requirement is almost always a comparison feeding an assertion,
two dispatches where one would do, so roughly 90 units per guard. That is 0.2% of a payroll but
up to 6% of a small guardrail template, which is where the fixed costs dominate.

**Take the template bump on upload.** An upload's largest single cost is the canonical search for
the template's own address, 1,500 units per rejected bump. Accepting it in the instruction would
make uploads cheaper and, more usefully, constant. One-time per template.

**Trim the fixed floor.** Of the 1,005-unit floor, 221 units go to parsing run inputs for a
template that has none and 169 to allocating the register file and scratch. Stack buffers instead
of `Vec` would plausibly recover a few hundred units: 0.5% of a payroll, 10% of a guardrail.

The costs not worth chasing are account validation and input decoding, and above all the invoke
itself: 1,000 of its ~1,210 units are the runtime's flat charge for any cross-program invocation,
which a hand-written program making the same calls would pay too.

### Keeping the wins

`fixtures/cu-ceilings.json` records the best figure every benchmark case has reached, and
`cargo test` fails when one of them costs more. `pnpm cu:ceilings` lowers a ceiling an improvement
has beaten but never raises one, so a regression has to be accepted explicitly, in the same commit,
where a reviewer sees it. That ratchet is what caught the register-restore change above. It also caught a case whose
compute depended on `Pubkey::new_unique`, a counter shared with every other test in the binary,
which made the case's template address — and so its bump search — depend on what else had run.

The ratchet covers both halves. `fixtures/cu-ceilings.json` holds the hand-built cases, which
measure the executor, and `fixtures/example-ceilings.json` holds every cookbook example, which
measures the compiler as well — the two largest wins above were compiler changes that the first
file could not see.

Both sets are reproducible now. They were not at first: the harnesses drew addresses from
`Pubkey::new_unique`, whose counter is shared with every other test in the binary, so a template's
own address — and the depth of the bump search that derives it — depended on which tests happened
to run alongside. Two examples moved by thousands of units between runs. They now draw from a
fixed sequence.

Raising a ceiling is deliberate. The batch work above costs every run a few units to decide what
it can keep, so six ceilings went up by 9 to 39 units while the batched examples fell by
thousands; those six were edited by hand, which is the point.

## Whole instructions, tracked over time

`pnpm cu:bench` runs [`mollusk-svm-bencher`](https://docs.rs/mollusk-svm-bencher) over a fixed set
of instructions and writes `benches/compute_units.md`. The file is committed, so the next run
prints a delta against it and any change to the program shows its compute cost in the diff. The
cases are in `tests/ballista/benches/compute_units.rs`.

::: tip Why not a VM trace
`agave-ledger-tool program run --trace` would give a per-opcode trace, but `program run` in Agave
4.1.0 exits with `The argument 'accounts_index_limit' wasn't found` before it executes anything.
Sampling the compute meter from inside the program gets the same attribution without depending on
that path.
:::
