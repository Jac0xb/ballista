# Token sweep (`tokenSweepIntoSwap`) against the real programs

Tests: [`tests/token_sweep.rs`](../tests/token_sweep.rs). Snapshot: slot 451,100,151
(2026-09-27 20:09:42 UTC).

## The route

`usdcToSol`: 150 USDC for SOL, quoted for 1,218,153,385 lamports with a 50 bps slippage and no
platform fee.

- **Jupiter.** It is a v6 `route` instruction with one step, `RaydiumClmm` at 100%.
- **The pool.** Raydium CLMM's SOL/USDC pool is `3ucNos4NbumPLZNWztqGHNFFgkHeRMBQAVemeeomsUxv`.
  It has a tick spacing of 1 and stood at tick -20,950, about 123.09 USDC per SOL.
- **Tick arrays.** The route passes three tick arrays, starting at ticks -21,000, -20,940 and
  -20,880, together with the pool's bitmap extension.
- **Accounts.** `route` takes 24 accounts. The template passes the first four itself, so the
  account group is the other 20.
- **Jupiter's own transaction.** It is the compute budget (`SetComputeUnitLimit` 1,400,000), the
  setup that creates the wrapped SOL account, `route`, and the cleanup that closes the account.
  As a v0 transaction with one lookup table, it is 648 bytes.

The test runs Jupiter's own transaction with the Ballista run in place of `route`. The seller's
USDC balance is written directly, under write rule 1.

## Passed as written

The template ran unchanged against Jupiter v6, Raydium CLMM and mainnet's Token program. Nothing
in its logic needed fixing, and `pnpm fixtures` leaves its payload as it was.

| Balance (USDC units) | Against the quote | Proceeds (lamports) | Least allowed | CU | Bytes |
| --- | --- | ---: | ---: | ---: | ---: |
| 154,500,000 | 3% over | 1,254,697,894 | 1,248,424,496 | 71,200 | 700 |
| 145,500,000 | 3% under | 1,181,608,871 | 1,175,700,739 | 71,191 | 700 |
| 1,500,000,000 | ten times | 12,181,263,810 | 12,120,626,180 | 71,484 | 700 |

In every case the source ends at zero, because the whole balance was `in_amount`. "Least allowed"
is the quote rescaled to the balance, less 50 bps. Price impact was negligible at these sizes: at
3% over, the proceeds were only 92 lamports under the rescaled quote itself.

The failure paths fail as expected:

- **A balance at the dust floor (10,000 units)** fails with Ballista's `RequirementFailed` at
  `worthSelling`, before Jupiter is called. The whole transaction reverts, including the setup's
  wrapped SOL account, and the seller pays only the fee.
- **A balance 10,000 times the quote (1.5 million USDC)** fails inside Raydium CLMM with
  `NotEnoughTickArrayAccount` (6023). It reverts the same way, and the seller keeps the balance.

## Compute units and size

At 3% over the quote:

| Instruction | CU |
| --- | ---: |
| Compute budget | 150 |
| Setup: create the wrapped SOL account | 14,913 |
| Ballista run | 56,019 |
| · Jupiter `route` | 49,013 |
| · · Raydium CLMM `swap` | 41,361 |
| · Ballista's own share | 7,006 |
| Cleanup: close the wrapped SOL account | 118 |
| **Transaction** | **71,200** |

At the quoted size, Jupiter's own transaction takes 64,188 CU and 648 bytes. With the run in
place of `route`, it takes 71,194 CU and 700 bytes. The template therefore adds about 7,000 CU and
52 bytes, leaving 532 of the 1,232 bytes free.

## How far the balance may drift from the quote

There are two upper bounds, and whichever the balance reaches first applies.

- **The quote's slippage.** The rescaled quote is linear, so the price impact of the extra size
  has to fit within `slippageBps`.
- **The route's coverage.** The swap has to stay within the tick arrays the route's accounts
  include.

For this deep pool the slippage binds first. The ignored test
`measure_how_far_above_the_quote_a_balance_may_go` searched each bound to 1 USDC:

| Slippage | Largest balance that lands | Times the quote | CU | One step further |
| --- | ---: | ---: | ---: | --- |
| 50 bps (the quote's) | 268,013 USDC | 1,786.8 | 932,160 | Jupiter `SlippageToleranceExceeded` (6001) |
| 500 bps (relaxed) | 295,166 USDC | 1,967.8 | 1,051,868 | Raydium `NotEnoughTickArrayAccount` (6023) |

At the slippage edge, Raydium's swap completes and Jupiter's check on its output fails.

**The largest drift that works is about 1,787 times the quote.**

- **Tick coverage.** Selling USDC raises the pool's tick. The route's arrays end at tick -20,821,
  129 ticks or about 1.3% of price above where the pool stood.
- **Compute.** Compute grows with the ticks crossed: 71,194 CU at the quoted size, 91,505 CU at
  100 times the quote, 461,727 CU at 1,000 times and 1,051,868 CU at the tick-array edge. The
  route's limit is 1,400,000 CU, so compute never bound first, but at the edge it came within 25%.
- **Below the quote.** Every size tried landed, down to just above the 0.01 USDC dust floor. The
  one lower bound is rounding, which the dust floor already covers. With a dust floor of 0:
  - 1 unit fails in Raydium with `TooSmallInputOrOutputAmount` (6022);
  - every size tried from 2 to 100 units fails Jupiter's slippage check (6001);
  - 200 units (0.0002 USDC) lands.

These numbers belong to this pool at this slot. On a shallow pool the slippage bound can be a few
percent. Rerun the measurement after refreshing the snapshot:

```bash
cargo test --manifest-path tests/protocols/Cargo.toml --test token_sweep -- --ignored --nocapture
```

## Other observations

- **`saleMetTheQuote` never failed on its own.** It repeats Jupiter's slippage check against the
  same rescaled quote: `quote × (10,000 − slippageBps) / 10,000`, rounded down. In every short
  sale measured, Jupiter's check failed first (6001). The requirement is a second check, made on
  the balances.
- **A failed sale sells nothing.** Every failure, whether too small, too large or at the floor,
  reverts the whole transaction. The seller loses only the fee.

## What changed in the template

- **Its logic.** Nothing.
- **Its doc comment.** The comment said only the pools and tick arrays bound the drift. It now
  names both bounds. The comment is part of the source that the docs page embeds, and no compiled
  bytes changed.

## Notes for the docs session

- **"Not yet run against Jupiter."** `docs/examples/protocols/token-sweep.md` says this, and
  "Running against the protocols" in `docs/examples/protocols/index.md` says no template has been
  run. This one now has been, against Jupiter v6 and Raydium CLMM at the slot above.
- **The drift paragraph.** "Limited by the pools the route's accounts cover" should name both
  bounds and say that slippage comes first on a deep pool. The practical advice is to quote for
  roughly the balance you expect. The measurements above give a scale: about 1,787 times the quote
  on this pool at 50 bps.
- **Failures outside the template.** A balance too large or too small for the route fails inside
  the route, with Jupiter's 6001 or the AMM's own error, rather than at a template label. Nothing
  is sold.
- **Choosing the dust floor.** Set it above the rounding floor, which is about 200 units at 50 bps
  on this pool. A smaller balance then fails at `worthSelling` instead of inside the route.
- **Cost.** The whole transaction took 71,200 CU and 700 bytes, about 7,000 CU and 52 bytes more
  than Jupiter's own. Here Jupiter's compute budget set the limit to 1,400,000 CU. A client that
  sets a tighter limit from a simulation of Jupiter's own transaction needs to add the run's cost.
- **A worked run for the Rust tab.** The tab shows the Jupiter deposit's run. `Sweep::sell` in
  `tests/protocols/tests/token_sweep.rs` builds this template's own run by name:
  - the accounts `jupiter`, `tokenProgram`, `seller`, `sourceAta` and `destinationAta`;
  - the inputs `routePlan`, `quotedInAmount`, `quotedOutAmount`, `slippageBps`, `platformFeeBps`
    and `dustFloor`;
  - the group, which is `route`'s accounts after the fourth.

  It then swaps the run into Jupiter's own transaction with `with_swap`. `routePlan` is the Borsh
  vector, including its `u32` count.
