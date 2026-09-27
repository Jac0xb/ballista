# `pythFreshPriceGate` against the real programs

Tests: [`tests/pyth_fresh_price_gate.rs`](../tests/pyth_fresh_price_gate.rs).

The snapshot and route are the same as in [the oracle swap's findings](oracle-swap.md):
- slot 451,100,151;
- route `solToUsdc`, 1 SOL for USDC through Meteora DLMM;
- Pyth SOL/USD at $123.088, with $0.012 of confidence, published 19 s before the clock.

The gate's action is Jupiter's `route`, in place of the one in Jupiter's own transaction. Its terms
are the ones a trader selling SOL might set:

| Term | Value |
| --- | --- |
| `feedId` | SOL/USD |
| `maximumAge` | 60 s |
| `maximumConfidence` | $0.10 |
| `floorPrice` to `ceilingPrice` | $100 to $150 |

## Runs

| Run | Result |
| --- | --- |
| Fresh (19 s) and in band | Lands. The route fills 123,106,283 USDC units, as in Jupiter's own transaction |
| Clock moved on 41 s (write rule 3), so the price is exactly 60 s old | Lands |
| One second later, at 61 s | Fails at `priceIsFresh` (pc 17) after 2,761 CU |
| Floor one raw unit above the price | Fails at `priceAboveFloor` (pc 23) after 3,117 CU |
| Ceiling one raw unit below the price | Fails at `priceBelowCeiling` (pc 26) after 3,295 CU |
| Floor and ceiling both equal to the price | Lands |
| SOL/USD's account copied as USDC/USD's, with the same price, and `feedId` SOL/USD | Fails at `priceIsTheExpectedFeed` (pc 12) after 2,340 CU |
| The same account with `feedId` USDC/USD | Lands (a control) |
| The unfixed gate (the payload committed before the feed pin) with that account | Lands. This was a scratch run and is not committed |

Every requirement comes before the route, so a refused run never reaches Jupiter.

## Compute units and size

|  | The gate's run | Jupiter's own transaction |
| --- | --- | --- |
| Transaction | 79,407 CU | 73,084 CU |
| Jupiter's `route` | 40,985 CU | 40,985 CU |
| Ballista's own work (its run less `route`) | 6,323 CU | none |
| Wire size | 857 bytes | 698 bytes |

- **The feed pin's cost.** As in the swap, it costs 318 CU and 32 bytes: the unfixed gate took
  79,089 CU and 825 bytes.
- **The template's size.** It is 664 bytes, up from 596.
- **Why this is larger than the oracle swap's run.** It is 22 bytes larger. Its four 8-byte terms
  carry 24 bytes more than the swap's one tolerance, and it passes two fewer accounts.

## Findings

### P1, fixed: the gate took any feed's price

The gate pinned its price account's owner, length and verification level, but not the feed. The
unfixed gate acted on an account carrying USDC/USD's feed id and SOL's price, in band. So a gate
set for SOL could act on any other feed whose price happened to fall inside its band, even while
SOL itself was out of it.

An asset priced near SOL is the easy case, such as a staked-SOL token's feed. That example is
inferred, not tested.

The gate now takes a `feedId` and checks it right after the verification level, as the oracle
swap does. For the fix, what it still allows, and Pyth's own check, see
[the oracle swap's findings](oracle-swap.md#p1-fixed-the-price-accounts-feed-was-never-checked).

### Freshness matches Pyth's rule to the second

The gate requires `clock − publish_time ≤ maximumAge`. Pyth's `get_price_no_older_than` requires
`publish_time + maximum_age ≥ clock.unix_timestamp`, the same bound. Against LiteSVM's Clock
sysvar, the price passes at exactly `maximumAge` seconds old and fails one second later.

### The band is inclusive

`floorPrice ≤ price ≤ ceilingPrice`. A band of exactly the price lets the route run. One raw unit
past either end fails at that end's own requirement.

### Observation: the band is in raw units, and the exponent is never read

`floorPrice`, `ceilingPrice` and `maximumConfidence` are compared with the feed's raw integers, as
its docs say. The gate does not read the exponent at 89, which is −8 for SOL/USD. With the feed
pinned, the exponent is whatever Pyth publishes for that feed. If Pyth ever changed it, every bound
would be off by a power of ten.

Pinning the exponent the way the old oracle swap did, as an input compared at 89, would make that
a clean failure. These runs do not depend on it.

### Not exercised here

These runs never fail `publishersAgree` (confidence) or `priceIsFullyVerified`. Both are single
reads of the price account, like the requirements above.
