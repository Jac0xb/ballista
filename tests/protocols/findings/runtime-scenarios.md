# Runtime scenarios against the real programs

Tests: [`tests/runtime_scenarios.rs`](../tests/runtime_scenarios.rs). Templates:
[`clients/js/examples/scenarios`](../../../clients/js/examples/scenarios), compiled into
`fixtures/protocol-scenarios.json`. They are test-only: no docs pages, no Rust mirrors, and not in
the protocol-example count.

They exercise the runtime's newer features on real mainnet programs: count loops beside row loops,
`EMIT`, `SET_RETURN_DATA`, and a nested Ballista run whose return data the outer run reads. Every
run uses route `solToUsdc`: 1 SOL for USDC through Meteora DLMM, valued at the snapshot's Pyth
SOL/USD price of $123.088. Each run takes `route`'s place after Jupiter's compute budget and setup.
Jupiter's cleanup is left out so that the run is the transaction's last instruction (see
[Issues](#issues-found)).

## What each scenario proves

**Scenario A, `splitSellPayout`: two loops, events and return data in one template.**

- **Checks before any CPI.** The seller must own both token accounts, which must hold wrapped SOL
  and USDC. The Pyth account must be a fully verified update of the named feed, at most 60 s old,
  with a positive price and an exponent of −8.
- **The split sell.** `step.repeat(slices, …, { max: 8 })` sells equal slices of the balance, one
  `route` per pass. Each pass writes the slice into `in_amount`, rescales the quote, requires
  `sold == slice` and a fill at or above the Pyth floor less 1%, emits `SLCE` (pass index, sold,
  received), and carries the running total.
- **The payout.** A `forEach` pays each row's USDC with SPL Token `transfer`, and requires the
  running sum of payouts to stay within the total received.
- **The return data.** `setReturnData` is set last, after every invoke, to the total received.

| Run | Result |
| --- | --- |
| 1 SOL in 4 slices, 2 rows | Lands. Each `SLCE` event decodes to exactly the fill Jupiter alone gets for that slice (30,776,570 each; the slices never leave one DLMM bin). The return data is 123,106,280: the seller's USDC plus the rows' payouts |
| 40 SOL in 4 slices, oracle at 12,424,958,630 (write rules 1 and 2) | Fails at `sliceBeatTheOracle` on the fourth pass. The fills fall slice by slice (1,230,983,000 down to 1,230,070,903) as the sale walks DLMM bins. The oracle sits between the edges of the third fill and the fourth. The failed transaction still logs the three passes that cleared |
| `slices` 9, above `max` | Fails with `LoopCountExceeded` at `sellSlices`, before Jupiter runs |
| Rows asking the total + 1 | Fails at `payoutsWithinProceeds`. Rows asking exactly the total land and leave the seller 0 |
| 7 slices in Jupiter's transaction | Fails with `MaxInstructionTraceLengthExceeded`, not for lack of compute (see [the instruction trace](#the-instruction-trace)) |
| 8 slices with the token accounts already in place (write rule 1) | Lands: 61 trace entries |

Splitting costs almost nothing here. One slice fetched 123,106,283, two slices 123,106,282, and
four slices 123,106,280: the difference is rounding on each swap.

**Scenario B, `nestedSplitSellPayout` over `splitSellInner`: output across a nested run.**

The outer run calls Ballista's `run` on the inner split sell. It forwards the inner run's fixed
accounts in the inner run's own order, with the seller as the one signer, then the route as the
inner run's group. It reads the returned `u64` straight after the call, pays the rows within it,
and emits `PAID` (inner total, paid).

| Run | Result |
| --- | --- |
| 4 slices, rows paying exactly the inner total | Lands. The inner run's `SLCE` events are logged at stack height 2 and match Jupiter alone. `PAID` is logged at height 1 and reads (total, total). The seller keeps 0 |
| Rows asking the inner total + 1 | Fails at the outer run's `payoutsWithinInnerTotal`, after the inner run sold everything |
| `returnClaim` (returns any number) in the inner template's place | Fails with `AccountConstraintFailed` on `innerTemplate`, before any step. The same claim run through the unpinned `ballistaRelay` is passed up untouched |
| `ballistaRelay` over `fixtures/return-data.hex`, which sets no return data but leaves the Token program's 165 | Fails with `ReturnDataMismatch` at the relay's `readNextResult` |
| A return-data read placed before the call | Refused at upload with the verifier's `InvalidReturnData` (instruction 0). The same template with the read after the call uploads |
| The oracle 5% above the market | Fails at the inner template's own `sliceBeatTheOracle`. Its label comes through the outer run |

- **The program check.** `OP_RETURN_DATA` does not expose the program that set the data. It
  enforces it instead: the setter must be the program the preceding invoke called, here the pinned
  Ballista program.
- **The template check.** That proves a Ballista run set the data, not which template ran. So the
  outer run pins the inner template's address. A finalized template can be neither rewritten nor
  closed, so the address fixes its bytes.

## Call depth

**Observed limit: 5 frames.** That is the transaction's instruction plus four nested calls. Agave
4.3's `MAX_INSTRUCTION_STACK_DEPTH` is 5. LiteSVM runs mainnet's feature set, in which SIMD-0268's
raise to 9 frames is not active.

- **The probe.** `ballistaRelay` adds one frame each. Four relays over `returnClaim` land and pass
  the claim up through every frame. A fifth relay fails with `CallDepth` as it calls frame 6.
- **Scenario B sits exactly at the limit, and lands.** The outer run is frame 1, the inner run 2,
  Jupiter 3, DLMM 4. At frame 5 are the Token program's two transfers and DLMM's two calls to itself
  for its events. So no shallower inner CPI was needed.
- **No headroom.** Any extra frame fails: a venue that nests one level deeper, a transfer hook, or a
  relay in front of the outer run.

## The instruction trace

The binding limit on the split sell is the instruction trace, not compute: a transaction runs at
most 64 instructions, its own and every CPI's together.

| Where the entries come from | Entries |
| --- | --- |
| Jupiter's transaction before the first slice: 2 compute-budget, 4 setup instructions and their 8 CPIs, the run | 15 |
| Each slice: Jupiter's `route` and its event call, DLMM's swap and two event calls, two Token transfers | 7 |
| Each payout row | 1 |
| The nested run's inner Ballista frame | 1 |

- **In Jupiter's transaction:** 6 slices is the most (59 entries with two rows). The seventh slice
  overflows, and the runtime refuses the CPI that would be entry 65. The error is blamed on Ballista,
  with no Ballista code.
- **Without the setup:** 8 slices take 61 entries.

The formula is asserted for 1, 2, 4 and 6 slices, and so is 64 at the failure.

## Compute units and size

All figures are measured: printed by the tests, not asserted. The tests assert only that each
transaction fits the 1,232-byte packet.

| Run | Transaction | Ballista's outermost run | Wire size | Deepest frame |
| --- | --- | --- | --- | --- |
| A, 1 slice, 2 rows | 88,856 CU | 56,875 CU | 929 bytes | 4 |
| A, 2 slices | 133,231 CU | 101,250 CU | 929 bytes | 4 |
| A, 4 slices | 221,993 CU | 190,012 CU | 929 bytes | 4 |
| A, 6 slices | 310,760 CU | 278,779 CU | 929 bytes | 4 |
| A, 8 slices, no setup | 367,800 CU | 367,500 CU | 792 bytes | 4 |
| B, 4 slices, 2 rows | 227,225 CU | 195,244 CU | 966 bytes | 5 (asserted) |
| Four relays over `returnClaim` | 12,562 CU | — | 265 bytes | 5 (asserted) |

- **Per slice:** about 44,400 CU, of which Jupiter's `route` is 40,990. Ballista's own share of a
  pass is about 3,400 CU: the reads, the checks, the event and the CPI itself.
- **Nesting:** 5,232 CU and 37 bytes more than scenario A: the second Ballista frame, its
  32-account CPI, and the return-data read.
- **Compute budget:** the tests reuse the route's compute-budget instruction (a 1.4M limit).
  Without one, the default limit grows with the instruction count:
  - the run alone gets 200,000 CU and fits four slices (188,040 CU). A fifth runs out inside DLMM,
    which the logs then blame;
  - Jupiter's transaction, with its four setup instructions, still fits six slices (308,488 CU).

  These are one-off readings.

| Template | Payload | Registers (of 64) | Instructions (of 128) |
| --- | --- | --- | --- |
| `splitSellPayout` | 1,743 bytes | 61 | 82 |
| `splitSellInner` | 1,552 bytes | 56 | 73 |
| `nestedSplitSellPayout` | 578 bytes | 7 | 13 |
| `ballistaRelay` | 179 bytes | 2 | 4 |
| `returnClaim` | 68 bytes | 1 | 2 |

## Issues found

No runtime bug turned up. Every check behaved as `schema.ts` and the verifier describe it, so no
test is `#[ignore]`d. What these scenarios did turn up:

- **SDK: registers run out.** The compiler gives every expression node a fresh register and never
  reuses one, and `jupiterOracleCheckedSwap` already uses all 64. Its floor reads both mints'
  decimals and the exponent, and has two power-of-ten branches: 28 registers. With a
  count loop, an event and a payout loop added, compilation failed with "Template uses more than 64
  registers". So the split sell is SOL-for-USDC only: the mints are pinned, the exponent must be
  −8, and the floor is two `multiplyDivide`s on `u64`s (they keep the product exact, so no casts are
  needed).
- **SDK and verifier: a loop's `max` is not what fits.** The verifier bounds Ballista's own CPIs
  (`MAX_EXPANDED_CPIS` 64; this template's worst case is 12). It cannot know that each Jupiter CPI
  costs 7 trace entries, so `max: 8` passes and cannot land in Jupiter's own transaction. Size a
  loop to the trace. The `repeat` docs ("the worst-case CPI count assumes all of them") could say
  so.
- **SDK: no event decoder.** Neither SDK decodes `EMIT`'s `Program data:` lines. The harness does it
  itself in `tx::program_data`, tracking the `invoke` and `success` lines. That tracking is needed:
  a nested run's events and its caller's are interleaved in one log.
- **Events are logged before they are final.** A failed transaction's logs still hold the events
  of the passes before the failure (the oracle case above). A reader must check that the
  transaction landed.
- **Return data is the last instruction's.** The runtime clears it as each instruction starts. With
  Jupiter's cleanup after the run, the transaction ends with the Token program's empty return data
  (`a_later_instruction_clears_the_runs_return_data`), and the total survives only in the run's
  `Program return:` log line. Put the run last, simulate it, or read that line.
- **Return data names the program, not the template.** See scenario B: pin the inner template.
- **Harness:** `LoopCountExceeded` carries the `REPEAT`'s program counter but was missing from
  `tx.rs`'s labelled failures; it is added. `Outcome` now keeps the transaction's return data.
