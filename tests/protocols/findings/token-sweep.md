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
- **Failure points and the platform-fee runs.** The program counter and compute units at each
  failure, and the runs in [the quote and the fee are the builder's](#open-the-quote-and-the-fee-are-the-builders),
  are one-off readings from a scratch probe at the same slot.

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

## Sales

The template's pricing ran unchanged against Jupiter v6, Raydium CLMM and mainnet's Token program.
Its one fix, the owner checks in [Findings](#findings), came from a hostile route rather than from
these sales. It changes none of their proceeds, only their cost: 578 CU more each.

| Balance (USDC units) | Against the quote | Proceeds (lamports) | Least allowed | CU | Bytes |
| --- | --- | ---: | ---: | ---: | ---: |
| 154,500,000 | 3% over | 1,254,697,894 | 1,248,424,496 | 71,778 | 700 |
| 145,500,000 | 3% under | 1,181,608,871 | 1,175,700,739 | 71,769 | 700 |
| 1,500,000,000 | ten times | 12,181,263,810 | 12,120,626,180 | 72,062 | 700 |

In every case the source ends at zero, because the whole balance was `in_amount`. "Least allowed"
is the quote rescaled to the balance, less 50 bps. Price impact was negligible at these sizes. The
proceeds against the rescaled quote itself:

- 92 lamports under it at 3% over;
- 88 over it at 3% under;
- 0.002% under it at ten times.

The failure paths fail as expected:

- **A balance at the dust floor (10,000 units)** fails with Ballista's `RequirementFailed` at
  `worthSelling` (pc 18, after 2,767 CU), before Jupiter is called. The whole transaction reverts,
  including the setup's wrapped SOL account, and the seller pays only the fee.
- **A balance 10,000 times the quote (1.5 million USDC)** fails inside Raydium CLMM with
  `NotEnoughTickArrayAccount` (6023). It reverts the same way, and the seller keeps the balance.
- **Another wallet's USDC account at `sourceAta`** fails at `sweepsTheSellersOwnBalance` (pc 11,
  after 2,311 CU), before Jupiter is called.
- **An attacker's wrapped SOL account at `destinationAta` and as the step's output** fails at
  `proceedsGoToTheSeller` (pc 15, after 2,583 CU), before Jupiter is called.
- **The attacker's account as the step's output only**, with the seller's own at `destinationAta`,
  fails at `saleMetTheQuote` (pc 36), after the route ran.

## Compute units and size

At 3% over the quote, split by the `consumed` lines that `measure_compute_units` prints:

| Instruction | CU |
| --- | ---: |
| Compute budget | 150 |
| Setup: create the wrapped SOL account | 14,913 |
| Ballista run | 56,597 |
| · Jupiter `route` | 49,013 |
| · · Raydium CLMM `swap` | 41,361 |
| · Ballista's own share | 7,584 |
| Cleanup: close the wrapped SOL account | 118 |
| **Transaction** | **71,778** |

Two rows are derived rather than read:

- **Compute budget.** A builtin logs no `consumed` line, so its 150 CU is what the other lines
  leave of the total.
- **Ballista's own share.** It is the run's CU less Jupiter's.

At the quoted size, Jupiter's own transaction takes 64,188 CU and 648 bytes. With the run in
place of `route`, it takes 71,772 CU and 700 bytes. The template therefore adds about 7,600 CU and
52 bytes, leaving 532 of the 1,232 bytes free.

**The owner checks' cost.** They add 578 CU to every sale: 71,194 CU at the quoted size before
them. They add 128 bytes to the template, 756 before and 884 now, and none to the transaction,
which already passes the seller's key.

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
| 50 bps (the quote's) | 268,013 USDC | 1,786.8 | 932,738 | Jupiter `SlippageToleranceExceeded` (6001) |
| 500 bps (relaxed) | 295,166 USDC | 1,967.8 | 1,052,446 | Raydium `NotEnoughTickArrayAccount` (6023) |

At the slippage edge, Raydium's swap completes and Jupiter's check on its output fails.

**The largest drift that works is about 1,787 times the quote.**

- **Tick coverage (one-off, from the decoded ticks above).** Selling USDC raises the pool's tick.
  The route's arrays end at tick -20,821, 129 ticks or about 1.3% of price above where the pool
  stood.
- **Compute.** Compute grows with the ticks crossed:
  - 71,772 CU at the quoted size;
  - 92,083 CU at 100 times the quote;
  - 462,305 CU at 1,000 times;
  - 1,052,446 CU at the tick-array edge.

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

## `saleMetTheQuote` fails on its own only when the proceeds go elsewhere

It repeats Jupiter's slippage check against the same rescaled quote:
`quote × (10,000 − slippageBps) / 10,000`, rounded down. Every sale the measurements saw refused
was refused by Jupiter or Raydium first. The drift search prints a count of each, and the
small-balance scan names the refuser for every size.

It fails on its own when the step pays another account while the seller's is measured. Jupiter
checks the step's output, which met the quote, so it lets the sale through. The template, which
measures the seller's account, sees no proceeds and refuses it.

## Findings

### P1, fixed: a hostile route could pay the proceeds to another wallet

**The problem.** The template checked that both token accounts belong to the SPL Token program and
are its size, but never whose they were. Jupiter checks `route`'s destination position by its mint
alone, and a step pays whichever account it names. So a route could pay the proceeds into another
wallet's wrapped SOL account and put that account at `destinationAta`, where `saleMetTheQuote`
would find them. The oracle-checked swap had the same hole; a reviewer showed it there first.

**What it allowed.** `an_attackers_destination_fails_at_proceeds_go_to_the_seller` writes an
attacker's empty wrapped SOL account under write rule 1. It puts the account at `destinationAta`,
and in the seller's place as the Raydium step's `output_token_account`, position 14 of `route`'s
accounts. Against the template before this fix (756 bytes), the seller's sale of its 150 USDC
**landed** (71,194 CU, 732 bytes):
- the attacker's wrapped SOL account went from 0 to 1,218,153,385 lamports, the whole fill;
- the seller's USDC account went from 150,000,000 to 0;
- the seller's own wrapped SOL account, which the setup had created, was closed empty;
- the seller's lamports changed by −5,000, the fee: the setup's rent came back when the account
  closed.

Every check passed. The whole balance was sold, and the proceeds, measured in the attacker's
account, met the rescaled quote.

**The fix.** Two requirements, the template's first steps:
- `sweepsTheSellersOwnBalance` requires `accountData(sourceAta, 32, 'pubkey')` to equal the
  seller's key;
- `proceedsGoToTheSeller` requires the same of `destinationAta`.

Offset 32 is the SPL Token account's `owner` (`TOKEN_ACCOUNT_OWNER_OFFSET` in `shared.ts`). The
source's check stops no attack found here, and states that the balance swept is the seller's.
Before it, another wallet's account at `sourceAta` failed at `nothingMeaningfulLeftBehind` (pc 31),
after the route had sold the same amount from the seller's own account.

**Now.** The attack fails at `proceedsGoToTheSeller` before Jupiter is called. The seller keeps its
USDC and pays only the fee, and the attacker's account still holds 0.

### Open: the seller's other token accounts

The seller signs `route`, and Jupiter passes that authority to every step. A step can debit any
token account the seller owns and pay any account, and neither measured balance shows it. The
oracle-checked swap's findings (`findings/oracle-swap.md`) give an example, and a delegate approved
only on the source would close it. It is a design question, left open.

### Open: the quote and the fee are the builder's

The sweep has no price of its own: `saleMetTheQuote` holds the proceeds to `quotedOutAmount` and
`slippageBps`, which the run's builder supplies with the route. `route`'s `platform_fee_account`
(position 6) is in the group, and nothing checks who owns it. One-off probe runs of the fixed
template sold the quoted 150 USDC with an attacker's wrapped SOL account as the platform fee
account:
- **A 100 bps fee at the quote's 50 bps of slippage:** Jupiter refused it (6001).
- **100 bps with `slippageBps` 200:** it **landed**. The attacker took 12,181,533 lamports, 1% of
  the fill, and the seller 1,205,971,852.
- **250 bps with `slippageBps` 300:** it **landed**. The attacker took 30,453,834 lamports.

So `slippageBps` is also what a hostile builder may take, up to the 2.55% a `u8` fee allows. And a
builder who writes the quote can lower it. The seller should read `quotedOutAmount` and
`slippageBps` in what it signs as the least it accepts. The owner checks ensure only that the
proceeds they are measured against reach the seller.

## Notes for the docs session

These claims in `docs/` are no longer true.

`docs/examples/protocols/token-sweep.md`:
- **12–18.** The step list lacks the owner checks, now the first two steps:
  `sweepsTheSellersOwnBalance` and `proceedsGoToTheSeller`.
- **20–22.** "Limited by the pools the route's accounts cover" should name both bounds and say that
  slippage comes first on a deep pool. The practical advice is to quote for roughly the balance you
  expect. The measurements above give a scale: about 1,787 times the quote on this pool at 50 bps.
- **24–26.** It says the template passes `route`'s first four accounts itself. It should add that
  the template requires both token accounts to be the signer's, since Jupiter checks the
  destination by its mint alone and a step pays whichever account it names.
- **42–44.** "A Token-2022 account fails the template's owner check." The template now has two
  checks that could be called its owner check. The account constraint pins the account's program,
  SPL Token. The requirements pin the token account's `owner` field, at offset 32, to the seller.
- **46–48.** The Rust tab shows the Jupiter deposit's run. `Sweep::sell_routed` in
  `tests/protocols/tests/token_sweep.rs` builds this template's own run by name:
  - the accounts `jupiter`, `tokenProgram`, `seller`, `sourceAta` and `destinationAta`;
  - the inputs `routePlan`, `quotedInAmount`, `quotedOutAmount`, `slippageBps`, `platformFeeBps`
    and `dustFloor`;
  - the group, which is `route`'s accounts after the fourth.

  It then swaps the run into Jupiter's own transaction with `with_swap`. `routePlan` is the Borsh
  vector, including its `u32` count.
- **50–52.** "Not yet run against Jupiter", and "No test calls Jupiter". This template has been
  run, against Jupiter v6 and Raydium CLMM at the slot above.

`docs/examples/protocols/index.md`:
- **8–9, 31–33, 45–46 and 117.** Each says no template has been run against the real protocols, or
  has measured costs.

What the pages could add:
- **Failures outside the template.** A balance too large or too small for the route fails inside
  the route, with Jupiter's 6001 or the AMM's own error, rather than at a template label.
- **Choosing the dust floor.** Set it at the rounding floor or above: 194 units at 50 bps on this
  pool. Every balance above it that the scan tried then lands, and a smaller one fails at
  `worthSelling` instead of inside the route.
- **Cost.** At the quoted size the whole transaction took 71,772 CU and 700 bytes, about 7,600 CU
  and 52 bytes more than Jupiter's own 64,188 CU and 648 bytes. Here Jupiter's compute budget set
  the limit to 1,400,000 CU. A client that sets a tighter limit from a simulation of Jupiter's own
  transaction needs to add the run's cost.
- **The builder's share.** `quotedOutAmount` and `slippageBps` are the least the seller accepts,
  whoever builds the run: see [the quote and the fee are the builder's](#open-the-quote-and-the-fee-are-the-builders).
