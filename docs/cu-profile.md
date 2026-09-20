# Compute profile

What each part of a template costs, measured one feature at a time. Every figure below is a
difference between two templates that differ only in how many times they do one thing, so the fixed
cost of a run cancels and what is left is the price of that feature.

Reproduce with `UPDATE_BENCHMARKS=1 cargo test --manifest-path tests/ballista/Cargo.toml
profile_compute_units -- --nocapture`. The harness lives in `tests/ballista/src/profile.rs`.

<!-- profile:table -->

| Measurement | Compute units | What it covers |
| --- | ---: | --- |
| **Run** | | |
| Fixed cost of any run | 1,012 | parse, validate, allocate, one assertion |
| **Instructions** | | |
| PDA derivation | 4,846 | one literal seed, bump search |
| Clock timestamp | 206 | sysvar read |
| Clock slot | 205 | sysvar read |
| Account data read, dynamic offset | 185 | offset from a register |
| Account data read | 173 | u64 at a fixed offset |
| Checked add | 159 | u64 + u64 |
| Comparison | 155 | u64 < u64 |
| Cast | 125 | u64 to u128 |
| Boolean and | 118 | bool && bool |
| Account key read | 115 | 32 bytes into a register |
| Move | 106 | copy a register |
| Account emptiness read | 105 | bool header field |
| Account lamports read | 102 | u64 header field |
| Account length read | 102 | u64 header field |
| Constant | 86 | const u64 |
| **Invocations** | | |
| Cross-program invocation | 1,706 | System transfer, 2 accounts, 12 bytes |
| Account passed to an invocation | 140 | resolved and given a meta |
| Byte of invocation data | under 0.01 | measured across 1,000 extra bytes |
| **Batches** | | |
| Iteration with one invocation | 1,855 | the shape a payroll run repeats |
| Iteration, three instructions | 474 | one account read, compare, assert |
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

**Address derivation is the one big cost.** A single `derivePda` or `assertAta` costs more than
three cross-program invocations. The reason is the bump search: deriving a canonical address means
hashing with bump 255, then 254, and so on until the result is off-curve, and the runtime charges
1,500 units for every attempt. Three attempts is typical.

**Cross-program invocation is mostly not ours.** Of the units an invocation costs, the runtime
charges a flat 1,000 for the call itself and the callee charges its own. What Ballista adds is the
remainder, spent resolving accounts and building the instruction.

**Everything else is small and linear.** An instruction costs between 85 and 210 units, an account
between 45 and 70 to validate, an input under 80 to decode, and a loop iteration adds almost
nothing beyond the instructions inside it.

## Where the golf is

These are the measured opportunities, in the order a reader should care about them. None of them
are implemented.

**Take the bump as an input.** A template that derives a PDA pays the full search every run, but
the caller already knows the canonical bump: it is public and stable. If a derivation could accept
the bump and use one address computation instead of a search, it would cost about 1,500 units
instead of about 4,800. For the batched ATA pattern, which derives once per row, that is a saving
of roughly 3,300 units per row. The check stays exactly as strong, because a wrong bump produces a
different address and the comparison still fails.

**Fuse compare with assert.** A requirement is almost always a comparison feeding an assertion, two
dispatches where one would do. A fused form would save on the order of 150 units per guard. Most
templates have a handful, so this is worth tens of units per run, not hundreds.

**Resolve invocation accounts once.** An account passed to an invocation costs about 140 units to
resolve and turn into a meta, paid again on every iteration of a batch even when the account is
fixed. Hoisting the fixed part of an invocation's account list out of the loop would save that
repeat.

The costs not worth chasing are the fixed cost of a run, account validation, and input decoding.
Together they are under two thousand units for a typical template, and they buy the checks that
make a stored template worth running.
