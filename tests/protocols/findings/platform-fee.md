# The route's platform fee, capped in every Jupiter template

Tests: `a_hostile_platform_fee_fails_at_platform_fee_within_cap` in each of
[`oracle_checked_swap.rs`](../tests/oracle_checked_swap.rs), [`token_sweep.rs`](../tests/token_sweep.rs),
[`jupiter_daily_cap.rs`](../tests/jupiter_daily_cap.rs), [`jito_tip.rs`](../tests/jito_tip.rs),
[`jupiter_deposit_exact_output.rs`](../tests/jupiter_deposit_exact_output.rs),
[`kamino_repay_swap_output.rs`](../tests/kamino_repay_swap_output.rs),
[`pyth_fresh_price_gate.rs`](../tests/pyth_fresh_price_gate.rs) and
[`runtime_scenarios.rs`](../tests/runtime_scenarios.rs); and
`a_raised_cap_takes_a_fee_within_it_and_refuses_one_above` in `oracle_checked_swap.rs`. Snapshot:
slot 451,100,151. Every figure below was printed by those tests, or by their red runs against the
templates before the fix.

## The problem

Jupiter v6's `route(route_plan, in_amount, quoted_out_amount, slippage_bps, platform_fee_bps: u8)`
pays `platform_fee_bps` of its output to the platform fee account, position 6 of its account list.
Every template forwards that position in its account group. Whoever builds the run chooses both the
rate and the account, and nothing checked either: a hostile frontend could take up to 2.55% of
every swap through these templates. Jupiter's own slippage check is no defense, since the builder
also writes `slippageBps` and the quote it checks against.

## The fix

Each template that forwards a Jupiter `route` declares a constant,

```ts
/** The route's platform fee account and rate are chosen by whoever builds the run: cap the rate. */
export const MAX_PLATFORM_FEE_BPS = 0n;
```

and requires `platformFeeBps ≤ MAX_PLATFORM_FEE_BPS`, labelled `platformFeeWithinCap`, right
before the call. At 0 the fee account is irrelevant. An author running their own frontend can raise
the constant. The Rust mirrors use one helper, `require_platform_fee_within_cap`.

| Template | How the fee is read |
| --- | --- |
| `jupiterOracleCheckedSwap` | Already split: the `platformFeeBps` input |
| `tokenSweepIntoSwap` | Already split: the `platformFeeBps` input |
| `jupiterDailyCapSwap` | Already split: the `platformFeeBps` input |
| `jitoProfitGuardedTip` | Was `strategyData`, the joined round trip as bytes; now split |
| `jupiterDepositExactOutput` | Was `routeArgs` as bytes; now split |
| `kaminoRepaySwapOutput` | Was `routeArgs` as bytes; now split |
| `pythFreshPriceGate` | Was `actionData` as bytes; now split |
| `splitSellPayout`, `splitSellInner` (test-only scenarios) | Already split; checked once, before the loop |

**Why split, not the last byte.** No expression reads a byte out of a `bytes` input:
`accountDataBytes` and `instructionDataBytes` read accounts and instructions, and `bytesLength`
only measures. So the four templates now take `routePlan`, `inAmount`, `quotedOutAmount`,
`slippageBps` and `platformFeeBps`, as `splitJupiterRoute` (TypeScript) and `RouteQuote::split`
(Rust) split the Swap API's data, and write `route`'s data back from them, as the three split
templates already did. The Jito runners split the joined round trip the same way.

**Registers.** `jupiterOracleCheckedSwap` was at the runtime's 64 registers, and the cap needs two
(the constant and the comparison). It now reads the trader's key once, into `traderKey`, for both
owner checks, and computes `10,000 − toleranceBps` in u128, so that one 10,000 serves as both the
whole and the divisor. Both are the same checks. It is still at 64.

## Evidence

Each test sends the template's usual run with the route's fee raised and its platform fee account
replaced by a fresh attacker's token account of the output mint (`Leg::with_platform_fee`), at a
slippage loose enough for Jupiter's own check.

| Template | Fee, slippage | Before the cap | After |
| --- | --- | --- | --- |
| Oracle swap | 1, 100, 150, 255 bps at 200 | 1 bps landed (86,087 CU, 880 bytes); 100 bps landed, the attacker taking 1,231,062 of 123,106,283 USDC units; 150 bps failed at `fillBeatTheOracle` | Each fails at `platformFeeWithinCap` after 5,013 CU of Ballista's run |
| Token sweep | 100 at 200; 250 at 300 | Both landed: the attacker took 12,181,533 and 30,453,834 lamports (74,570 CU, 732 bytes) | Both fail there after 3,451 CU |
| Daily cap | 100 at 200 | Landed (85,728 CU, 839 bytes) | Fails there (pc 39) after 6,228 CU; no entry is left |
| Jito tip | 10 at 200, after the whale's sale, tip 1,000, no edge | Landed (122,616 CU, 966 bytes) | Fails there after 2,943 CU; no tip is paid |
| Deposit | 100 at 200 | Landed (154,000 CU, 1,104 bytes) | Fails there after 2,679 CU; nothing is deposited |
| Repay | 100 at 200 | Landed (144,166 CU, 1,025 bytes) | Fails there after 2,789 CU; the debt stands |
| Pyth gate | 100 at 200, in band | Landed (82,569 CU, 897 bytes) | Fails there after 3,777 CU |
| Split sell, 2 slices | 100 at 200 | Landed (137,471 CU, 920 bytes) | Fails there, before the first slice |

In every run after the fix, Jupiter is never invoked and the attacker's account stays at 0.

**Raising the cap.** The test-only scenario `jupiterOracleCheckedSwapFeeCap100`
(`clients/js/examples/scenarios/raised-fee-cap.ts`) is the oracle swap with
`MAX_PLATFORM_FEE_BPS` at 100. A 100 bps fee lands and pays the fee account exactly 1,231,062
units, 1% of the unfeed fill (86,132 CU, 880 bytes); 101 bps fails at `platformFeeWithinCap`
before Jupiter runs. `protocol-semantics.test.ts` checks that it differs from the oracle swap only
in that constant, and that every Jupiter template caps the fee it writes last into `route`'s data,
before the call, at 0.

## Cost

On each template's passing run, before and after the cap.

| Template | Ballista's run, CU | Transaction, bytes | Template, bytes |
| --- | --- | --- | --- |
| Oracle swap | 50,880 → 50,925 (+45) | 848 → 848 | 1,504 → 1,520 |
| Token sweep (transaction, ten times the quote) | 71,745 → 71,853 (+108) | 700 → 700 | 884 → 932 |
| Daily cap, creating the entry | 50,521 → 50,629 (+108) | 807 → 807 | 966 → 1,014 |
| Jito tip | 96,481 → 97,082 (+601) | 934 → 947 | 628 → 788 |
| Deposit (transaction) | 150,771 → 151,372 (+601) | 1,072 → 1,085 | 532 → 692 |
| Repay (transaction) | 141,179 → 141,830 (+651) | 993 → 1,006 | 562 → 722 |
| Pyth gate | 47,362 → 47,963 (+601) | 865 → 878 | 732 → 892 |
| Split sell, 1 slice | 56,875 → 56,969 (+94) | 929 → 929 | 1,743 → 1,775 |

The requirement alone costs about 100 CU; the oracle swap's register savings pay for half of it.
The four templates that now take the route in parts pay about 500 CU more to load four more inputs,
13 bytes more on the wire (the four numbers as 32 bytes of inputs, where the bytes input packed
them in 19), and 160 bytes more of template. No CU ceiling moved: `tests/ballista` passes
unchanged.

## What it does not cover

- **Venues' own fee accounts**, such as Meteora's `host_fee_in`, sit in the route's accounts too.
  The cap does not reach them. Where a template measures the output (the oracle swap's fill
  check, the sweep's quote check, the Jito tip's profit check), that bounds them; where it does
  not (the daily cap, the Pyth gate), nothing does.
- **The quote.** `quotedOutAmount` and `slippageBps` are still the builder's; see
  [the token sweep's findings](token-sweep.md#open-the-quote-is-the-builders).
- **The signer's other token accounts**, which a route's steps can spend; see
  [the oracle swap's findings](oracle-swap.md#open-the-signers-authority-over-its-other-token-accounts).

## Docs pages that need a line

`docs/` was out of bounds for this change.

- `docs/examples/protocols/jupiter-oracle-swap.md`: "Fees within the tolerance" (lines 36–39) is no
  longer true; the step list should add `platformFeeWithinCap` and `MAX_PLATFORM_FEE_BPS`.
- `docs/examples/protocols/token-sweep.md`: "A bad quote or fee" (lines 29–31) should drop the
  fee; the step list should add the cap.
- `docs/examples/protocols/daily-cap.md`: the step list should add the cap.
- `docs/examples/protocols/jito-tip.md`: lines 62–63 name the input `strategyData`; it is now
  `routePlan`, `inAmount`, `quotedOutAmount`, `slippageBps` and `platformFeeBps`, split from the
  joined data. Add the cap.
- `docs/examples/protocols/jupiter-deposit.md`: line 29 says the route arrives whole as
  `routeArgs`, and lines 64–65 name `routeArgs`; both now the five split inputs. Add the cap.
- `docs/examples/protocols/kamino-repay.md`: lines 60–61 name `routeArgs`; now the five split
  inputs. Add the cap.
- `docs/examples/protocols/pyth-gate.md`: lines 58–59 name `actionData`; now the five split
  inputs. Add the cap.
- `docs/examples/protocols/index.md`: "Jupiter calls" (lines 48–59) could say that every Jupiter
  template caps the route's platform fee at a constant, 0 unless its author raises it.
- `docs/guide/guardrails.md`: "Deadline and minimum output" could add capping a route's platform
  fee to what to check before a swap.

No page includes `clients/rust/examples/protocol_runs.rs`, whose Pyth-gate and deposit runs now pass
the route's parts through `route_inputs`. The pages' code tabs include the templates and runs
changed here, so they show the cap and the split inputs without an edit.
