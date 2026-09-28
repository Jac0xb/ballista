# Token sweep (`tokenSweepIntoSwap`) against the real programs

Tests: [`tests/token_sweep.rs`](../tests/token_sweep.rs). Snapshot: slot 451,100,151
(2026-09-27 20:09:42 UTC).

Every measured figure here comes from those tests at that slot. The checks print the passing
sales, and three ignored measurements print the rest:

- `measure_compute_units`;
- `measure_how_far_above_the_quote_a_balance_may_go`;
- `measure_the_smallest_balance_the_route_can_sell`.

Three statements come from elsewhere:

- **The route.** Its description below comes from `routes.json`, except the ticks, which are a
  one-off decoded by hand.
- **`SlippageToleranceExceeded`.** The name of Jupiter's 6001 comes from Jupiter's IDL: Jupiter
  logs only the code. Raydium logs its errors' names, and the tests print them.
- **`pnpm fixtures`.** That it leaves the payload as it was comes from one run.

After refreshing the snapshot, rerun the tests and update this file:

```bash
cargo test --manifest-path tests/protocols/Cargo.toml --test token_sweep -- --include-ignored --nocapture
```

## The route

`usdcToSol`: 150 USDC for SOL, quoted for 1,218,153,385 lamports with a 50 bps slippage and no
platform fee.

- **Jupiter.** It is a v6 `route` instruction with one step, `RaydiumClmm` at 100%.
- **The pool.** It is Raydium CLMM's SOL/USDC pool, `3ucNos4NbumPLZNWztqGHNFFgkHeRMBQAVemeeomsUxv`.
- **Accounts.** `route` takes 24 accounts. The template passes the first four itself, so the
  account group is the other 20.
- **Jupiter's own transaction.** It is the compute budget (a limit of 1,400,000 CU), the setup that
  creates the wrapped SOL account, `route`, and the cleanup that closes the account. As a v0
  transaction with one lookup table, it is 648 bytes.
- **Ticks (one-off, decoded by hand from the snapshot's pool and tick-array accounts).** The pool
  has a tick spacing of 1 and stood at tick -20,950, about 123.09 USDC per SOL. The route passes
  three tick arrays, starting at ticks -21,000, -20,940 and -20,880, together with the pool's
  bitmap extension.

The test runs Jupiter's own transaction with the Ballista run in place of `route`. The seller's
USDC balance is written directly, under write rule 1.

## Passed as written

The template ran unchanged against Jupiter v6, Raydium CLMM and mainnet's Token program. Nothing
in its logic needed fixing. Its doc comment did: it said only the pools and tick arrays bound the
drift, and it now names both bounds. The docs page embeds that comment. No compiled bytes changed,
and `pnpm fixtures` leaves the payload as it was.

| Balance (USDC units) | Against the quote | Proceeds (lamports) | Least allowed | CU | Bytes |
| --- | --- | ---: | ---: | ---: | ---: |
| 154,500,000 | 3% over | 1,254,697,894 | 1,248,424,496 | 71,200 | 700 |
| 145,500,000 | 3% under | 1,181,608,871 | 1,175,700,739 | 71,191 | 700 |
| 1,500,000,000 | ten times | 12,181,263,810 | 12,120,626,180 | 71,484 | 700 |

In every case the source ends at zero, because the whole balance was `in_amount`. "Least allowed"
is the quote rescaled to the balance, less 50 bps. Price impact was negligible at these sizes. The
proceeds against the rescaled quote itself:

- 92 lamports under it at 3% over;
- 88 over it at 3% under;
- 0.002% under it at ten times.

The failure paths fail as expected:

- **A balance at the dust floor (10,000 units)** fails with Ballista's `RequirementFailed` at
  `worthSelling`, before Jupiter is called. The whole transaction reverts, including the setup's
  wrapped SOL account, and the seller pays only the fee.
- **A balance 10,000 times the quote (1.5 million USDC)** fails inside Raydium CLMM with
  `NotEnoughTickArrayAccount` (6023). It reverts the same way, and the seller keeps the balance.

## Compute units and size

At 3% over the quote, split by the `consumed` lines that `measure_compute_units` prints:

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

Two rows are derived rather than read:

- **Compute budget.** A builtin logs no `consumed` line, so its 150 CU is what the other lines
  leave of the total.
- **Ballista's own share.** It is the run's CU less Jupiter's.

At the quoted size, Jupiter's own transaction takes 64,188 CU and 648 bytes. With the run in
place of `route`, it takes 71,194 CU and 700 bytes. The template therefore adds about 7,000 CU and
52 bytes, leaving 532 of the 1,232 bytes free.

## How far the balance may drift from the quote

There are two upper bounds, and whichever the balance reaches first applies.

- **The quote's slippage.** The rescaled quote is linear, so the price impact of the extra size
  has to fit within `slippageBps`.
- **The route's coverage.** The swap has to stay within the tick arrays the route's accounts
  include.

For this deep pool the slippage binds first. `measure_how_far_above_the_quote_a_balance_may_go`
searched each bound to 1 USDC:

| Slippage | Largest balance that lands | Times the quote | CU | One step further |
| --- | ---: | ---: | ---: | --- |
| 50 bps (the quote's) | 268,013 USDC | 1,786.8 | 932,160 | Jupiter `SlippageToleranceExceeded` (6001) |
| 500 bps (relaxed) | 295,166 USDC | 1,967.8 | 1,051,868 | Raydium `NotEnoughTickArrayAccount` (6023) |

At the slippage edge, Raydium's swap completes and Jupiter's check on its output fails.

**The largest drift that works is about 1,787 times the quote.**

- **Tick coverage (one-off, from the decoded ticks above).** Selling USDC raises the pool's tick.
  The route's arrays end at tick -20,821, 129 ticks or about 1.3% of price above where the pool
  stood.
- **Compute.** Compute grows with the ticks crossed:
  - 71,194 CU at the quoted size;
  - 91,505 CU at 100 times the quote;
  - 461,727 CU at 1,000 times;
  - 1,051,868 CU at the tick-array edge.

  The route's limit is 1,400,000 CU, so compute never bound first, but at the edge it came within
  25%.
- **Below the quote.** The one lower bound is rounding, which the dust floor covers. With a dust
  floor of 0, `measure_the_smallest_balance_the_route_can_sell` finds:
  - 1 unit fails in Raydium with `TooSmallInputOrOutputAmount` (6022);
  - from 2 to 194 units, most sizes fail Jupiter's slippage check (6001), but 173, 179 to 181 and
    187 to 190 land;
  - from 195 units, every size lands, up to 1,000, where the scan stops;
  - the sizes it samples beyond the scan also land: 10,001 units, just above the tests' dust
    floor, then 100,000, 1,000,000, a tenth of the quote and half of it.

These numbers belong to this pool at this slot. On a shallower pool the extra size's price
impact reaches the slippage sooner, so less drift works. That follows from the first bound;
nothing here measured it.

## `saleMetTheQuote` never failed on its own

It repeats Jupiter's slippage check against the same rescaled quote:
`quote × (10,000 − slippageBps) / 10,000`, rounded down. Every sale the measurements saw refused
was refused by Jupiter or Raydium. The drift search prints a count of each, and the small-balance
scan names the refuser for every size. The requirement is a second check, made on the balances.

## Notes for the docs session

- **"Not yet run against Jupiter."** `docs/examples/protocols/token-sweep.md` says this, and
  "Running against the protocols" in `docs/examples/protocols/index.md` says no template has been
  run. This one now has been, against Jupiter v6 and Raydium CLMM at the slot above.
- **The drift paragraph.** "Limited by the pools the route's accounts cover" should name both
  bounds and say that slippage comes first on a deep pool. The practical advice is to quote for
  roughly the balance you expect. The measurements above give a scale: about 1,787 times the quote
  on this pool at 50 bps.
- **Failures outside the template.** A balance too large or too small for the route fails inside
  the route, with Jupiter's 6001 or the AMM's own error, rather than at a template label.
- **Choosing the dust floor.** Set it at the rounding floor or above: 194 units at 50 bps on this
  pool. Every balance above it that the scan tried then lands, and a smaller one fails at
  `worthSelling` instead of inside the route.
- **Cost.** At the quoted size the whole transaction took 71,194 CU and 700 bytes, about 7,000 CU
  and 52 bytes more than Jupiter's own 64,188 CU and 648 bytes. Here Jupiter's compute budget set
  the limit to 1,400,000 CU. A client that sets a tighter limit from a simulation of Jupiter's own
  transaction needs to add the run's cost.
- **A worked run for the Rust tab.** The tab shows the Jupiter deposit's run. `Sweep::sell_with`
  in `tests/protocols/tests/token_sweep.rs` builds this template's own run by name:
  - the accounts `jupiter`, `tokenProgram`, `seller`, `sourceAta` and `destinationAta`;
  - the inputs `routePlan`, `quotedInAmount`, `quotedOutAmount`, `slippageBps`, `platformFeeBps`
    and `dustFloor`;
  - the group, which is `route`'s accounts after the fourth.

  It then swaps the run into Jupiter's own transaction with `with_swap`. `routePlan` is the Borsh
  vector, including its `u32` count.
