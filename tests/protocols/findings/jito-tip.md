# `jitoProfitGuardedTip` against the real programs

- **Snapshot:** slot 451,100,151.
- **Tests:** [`tests/jito_tip.rs`](../tests/jito_tip.rs).
- **Template:** [`jito-profit-guarded-tip.ts`](../../../clients/js/examples/protocols/jito-profit-guarded-tip.ts).
- **Route:** `solToUsdcToSol`, quoted by Jupiter as two legs:
  - Leg 0: 1 SOL for 123.106283 USDC on Meteora DLMM (`HTvjzsfX…`).
  - Leg 1: that USDC back to 0.999749340 SOL on Raydium CLMM (`3ucNos4N…`, SOL/USDC, tick spacing 1).
  - Slippage is 50 bps on each leg.

## Verdict

As written, the template could not pay a tip out of a Jupiter strategy. It measured the searcher's
lamports around `route`, and `route` never moves them. Fixed: it now measures the searcher's
wrapped-SOL account, and the strategy is a round trip that starts and ends there. With that fix,
all three required runs behave as specified against the real programs.

## What the real programs showed

1. **The searcher's lamports do not move inside `route`.** `route` moves only token accounts. The
   Swap API wraps SOL and unwraps it in separate instructions before and after it, so a template
   that reads the searcher's lamports around `route` sees no profit even when the route makes one.
   After the whale's sale below, the round trip really makes 1,764,764 lamports, but the old
   template refused a 1-lamport tip, and Jito's 1,000-lamport floor, at `profitCoversTheTip`,
   paying only a zero tip. Pinned by `a_route_moves_wrapped_sol_and_no_lamports`: sent to Jupiter
   directly, a profitable round trip grows the wrapped SOL and changes the lamports by exactly the
   fee.

2. **The loss path.** The research predicted that a loss would underflow at `measureProfit` with
   ArithmeticOverflow. With lamports as the measure, a Jupiter strategy could not even produce the
   loss, since the delta was always 0. On wrapped SOL it would have: at the snapshot's prices the
   round trip loses 250,660 lamports. The template now requires `after >= before + tip + edge`, so
   the loss fails at `profitCoversTheTip` (`a_losing_round_trip_fails_at_the_requirement_rather_than_underflowing`).

3. **Two legs join into one `route`.** The joined route has:
   - a two-step plan: leg 0's step, then leg 1's step with its input and output indices changed
     from `00 01` to `01 02`;
   - leg 1's nine fixed accounts, with leg 0's source in place of leg 1's source, so the source and
     the destination are both the wrapped-SOL account;
   - then leg 0's step accounts, then leg 1's.

   Jupiter runs it. Step 2 spends exactly what step 1 produced, and the searcher's USDC ends where
   it started. Alone (compute budget and `route`), it costs 88,457 CU and 710 bytes, measured at
   slot 451,100,151 by the ignored `measure_the_findings_numbers`.

   Jupiter's slippage check applies to the last step's gross output (999,749,340), not to the net
   change of the account. With source equal to destination, that net change is −250,660.
   `quoted_out_amount = in_amount` with 0 bps fails with 6001 `SlippageToleranceExceeded`: Jupiter's
   own check could therefore enforce "no loss", but its failure carries Jupiter's code, not the
   template's label. Pinned by `jupiters_own_slippage_check_can_reject_a_loss`.

4. **Jupiter does not tie `route`'s source and destination accounts to its steps.** It checks the
   two positions like this:
   - **The source (position 2)** must hold at least `in_amount`, of any mint and any owner. One
     unit short fails with 6024. Exactly `in_amount` lands, including another wallet's USDC account.
   - **The destination (position 3)** must hold `destination_mint`. Another mint fails with 6019.
     So does a `destination_mint` that differs from the destination account's.
   - Neither code is in Jupiter's published IDL (`jup-ag/jupiter-cpi` v6.0.0), whose 18 errors stop
     at 6017, `ExactOutAmountNotMatched`.
   - Accounts that pass are left untouched. With another wallet's wrapped-SOL account in both
     positions, the route runs and moves the searcher's own accounts, the ones in its steps.

   So positions 2 and 3 prove nothing about what moved. Pinned by
   `jupiter_does_not_tie_the_route_s_source_and_destination_to_its_steps`. The template relies
   only on the balance it reads itself.

5. **Tip accounts.** The snapshot's tip account is owned by the Tip Payment program (`T1pyy…`) and
   has 8 bytes of data. A plain System transfer to it lands. The template now pins
   `jitoTip.owner` to that program, so a wallet passed as the tip account fails with
   `AccountConstraintFailed`.

## The design chosen

This change uses **(a), one joined `route`**, together with **measuring the wrapped-SOL account
that is its source and destination**.

- **(a)** It works against the deployed Jupiter. It is one invoke with one group, and Jupiter
  chains the amounts exactly: step 2 takes 100% of what step 1 produced.
- **(b), two invokes with two groups.** This would fix leg 1's `in_amount` at signing. The amount
  would have to sit at or under leg 0's worst-case output, stranding the difference in USDC, or
  fail when leg 0 fills short. It would also send two 9-account `route` headers where (a) sends
  one.
- **(c) on its own, one leg measured at its destination.** This counts gross proceeds, not profit.
  It is only right when the source is the destination, which is what (a) provides.

The profit is measured on wrapped SOL because that is where a SOL-to-SOL route pays out, and
because wrapped SOL is counted in lamports, the tip's unit. The template also requires the account
to be the searcher's, since the searcher pays the tip.

## Where the profit comes from

No route in the snapshot makes money: the round trip loses 250,660 lamports (0.025%) to the pools'
fees. The profitable runs therefore stage a backrun, the way such an opportunity arises on mainnet:

1. A whale sells SOL into the Raydium pool with Raydium CLMM's own `swap`, using the accounts from
   Jupiter's Raydium step with the input and output sides exchanged. This lowers that pool's SOL
   price.
2. The searcher's round trip sells 1 SOL on Meteora and buys it back cheaper on Raydium.

The only state written directly is the whale's wallet, under write rule 1: its SOL for fees, its
wrapped SOL, and an empty USDC account. Every other change goes through Raydium, Meteora, Jupiter
and Ballista.

| Whale sells | Round trip on 1 SOL |
| --- | --- |
| — | −250,660 |
| 100 SOL | +153,077 |
| 300 SOL | +958,406 |
| 500 SOL (the tests) | **+1,764,764** |

The "—" and 500 SOL rows are pinned by
`a_losing_round_trip_fails_at_the_requirement_rather_than_underflowing` and
`a_backrun_pays_the_tip_out_of_its_profit`; the 100 and 300 SOL rows are measured at slot
451,100,151 by the ignored `measure_the_findings_numbers`.

A 500 SOL sale costs 165,219 CU, measured the same way, so the whale's transaction asks for
`WHALE_SALE_COMPUTE_UNIT_LIMIT` (400,000) CU. The sale stays inside the tick array
[−21000, −20941], which holds the current tick and is the only array below the price that the
snapshot has.

## Runs

| Run | Result | CU, whole transaction | CU, Ballista's run | Bytes |
| --- | --- | --- | --- | --- |
| **pass:** tip 1,664,764 + edge 100,000 = the profit | tip paid in full; the searcher keeps the edge less the fee | 119,653 | 96,633 | 934 |
| **fail:** tip 1,664,765, one lamport over | `RequirementFailed` at `profitCoversTheTip`; nothing paid | — | 95,208 | same |
| **fail:** loss of 250,660, tip 1,000 | `RequirementFailed` at `profitCoversTheTip` | — | 95,295 | same |

Also checked: a USDC account as `wsolAccount` fails at `wsolAccountHoldsWrappedSol`. Another
wallet's wrapped SOL fails at `searcherOwnsTheWsolAccount`.

The template payload grew from 424 to 628 bytes. The run passes 46 runtime accounts: 6 fixed and a
40-account group. The CPI to Jupiter carries 44 accounts, within Ballista's limit of 64.

## Limits and open items

- **The docs page is stale.** `docs/examples/protocols/jito-tip.md` still describes lamport
  profit and says the template has not run against Jupiter. `docs/` was out of bounds for this
  task.
- **Another template (inference, not run here).** `jupiterOracleCheckedSwap` says "the accounts
  Jupiter moves are the ones this template measures". Item 4 shows that is not so. A route whose
  steps move other token accounts than `sourceAta` and `destinationAta` would measure 0 sold and
  0 received, and pass `fillBeatTheOracle` without checking anything. That matters for the threat
  its header names, a route built by someone other than the signer. `tokenSweepIntoSwap` fails
  safe in the same case: `saleMetTheQuote` and `nothingMeaningfulLeftBehind` both need the
  measured accounts to move.
- **Only single-step legs can be joined.** The test's `round_trip` needs each leg to be one step.
  A step ends in its two index bytes, but Jupiter's `Swap` enum before them has variants of
  different lengths, so a longer plan cannot be renumbered without decoding it. The SDK has no
  helper that builds the round trip.
- **The template measures one account.** Value that leaves other accounts is invisible to it.
  Jupiter's plan draws only on `in_amount` and on outputs of earlier steps, so a round trip built
  as above cannot draw on the searcher's other balances. The template does not enforce how the
  route was built, and the route is the searcher's own.
- **Refreshing the snapshot.** The whale's sale must stay inside the tick array the snapshot holds.
  After a refresh the current tick may sit nearer that array's edge. The sale then fails in
  Raydium, visibly, and `WHALE_SALE` needs resizing. A refresh can also move the second leg to
  another AMM, such as Whirlpool or Meteora. `whale_sells_sol` asserts that the leg's program is
  Raydium CLMM and says a refresh moved it, since the sale is built for Raydium's `swap`.
- **The auction.** Jito's block engine is not in LiteSVM. The tests show the tip is transferred,
  not how the auction scores it.
