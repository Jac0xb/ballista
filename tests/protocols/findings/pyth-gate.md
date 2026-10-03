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
| `exponent` | −8, SOL/USD's, which the three bounds below are in units of |
| `maximumAge` | 60 s |
| `maximumConfidence` | $0.10 |
| `floorPrice` to `ceilingPrice` | $100 to $150 |

## Runs

| Run | Result |
| --- | --- |
| Fresh (19 s) and in band | Lands. The route fills 123,106,283 USDC units, as in Jupiter's own transaction |
| Clock moved on 41 s (write rule 3), so the price is exactly 60 s old | Lands |
| One second later, at 61 s | Fails at `priceIsFresh` (pc 26) after 3,043 CU |
| Floor one raw unit above the price | Fails at `priceAboveFloor` (pc 32) after 3,399 CU |
| Ceiling one raw unit below the price | Fails at `priceBelowCeiling` (pc 35) after 3,577 CU |
| Floor and ceiling both equal to the price | Lands |
| SOL/USD's account copied as USDC/USD's, with the same price, and `feedId` SOL/USD | Fails at `priceIsTheExpectedFeed` (pc 18) after 2,429 CU |
| The same account with `feedId` USDC/USD | Lands (a control) |
| The unfixed gate (the payload committed before the feed pin) with that account | Lands. This was a scratch run and is not committed |
| SOL/USD's exponent moved from −8 to −7 (write rule 2), its raw price and confidence kept | Fails at `priceExponentIsExpected` (pc 21) after 2,629 CU. It landed before the fix |
| The same account with `exponent` −7 | Lands (a control). No other check reads the exponent |
| In band, with a 100 bps platform fee paid to an attacker's USDC account at `slippageBps` 200 | Fails at `platformFeeWithinCap` after 3,777 CU. It landed before the fix (82,569 CU, 897 bytes) |

Every requirement comes before the route, so a refused run never reaches Jupiter. The program
counters are the current template's. The compute units at each failure predate the platform-fee
cap: the four route numbers it now loads as inputs add about 120 CU before the first requirement.

## Compute units and size

|  | The gate's run | Jupiter's own transaction |
| --- | --- | --- |
| Transaction | 79,689 CU | 73,084 CU |
| Jupiter's `route` | 40,985 CU | 40,985 CU |
| Ballista's own work (its run less `route`) | 6,605 CU | none |
| Wire size | 865 bytes | 698 bytes |

- **The exponent pin's cost.** It costs 282 CU and 8 bytes, the `exponent` input: the run took
  79,407 CU (6,323 of them Ballista's own) and 857 bytes before it. A run refused at any later
  requirement pays the same 282 CU more; one refused at the feed pin pays 89, for the input alone.
- **The feed pin's cost.** As in the swap, it cost 318 CU and 32 bytes: the gate took 79,089 CU and
  825 bytes before it.
- **The platform-fee cap's cost.** It costs 601 CU and 13 bytes: the run took 79,461 CU (47,362
  in Ballista's run) and 865 bytes before it, and 80,062 (47,963) and 878 after. Most of it is
  taking the route in parts: four more inputs to load, and the route's four numbers sent as 32
  bytes of inputs where `actionData` packed them in 19. The table above predates it.
- **The template's size.** It is 892 bytes: 732 before the platform-fee cap, 664 before the
  exponent pin, and 596 before the feed pin.
- **Why this is larger than the oracle swap's run.** At 878 bytes it is 30 larger than the swap's
  848. Its five 8-byte terms carry 32 bytes more than the swap's one tolerance, and it passes two
  fewer accounts.

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

### P2, fixed: the exponent was never read

**The problem.** `floorPrice`, `ceilingPrice` and `maximumConfidence` are compared with the feed's
raw integers, and the gate never read the exponent, an i32 at 89. The bounds mean dollars only at
the exponent the caller set them for, −8 for SOL/USD. If Pyth changed the feed's exponent, every
bound would be off by a power of ten, and the gate would still act.

**What it allowed.** `a_price_at_another_exponent_fails_at_the_exponent_pin` moves SOL/USD's
exponent from −8 to −7 under write rule 2, keeping its raw price, confidence and publish time:
- at −7 the price is $1,230.88, far above the $150 ceiling, and the confidence is $0.12, over the
  $0.10 limit;
- against the unfixed gate, with the usual terms, the run **landed** (79,407 CU, 857 bytes), and
  the route sold 1 SOL for 123,106,283 USDC units, as it does in band. The raw integers passed
  every check.

**Why P2, not P1.** No one else can bring this about. The owner, the verification level and the
feed id hold the account to a Wormhole-verified update for the feed the signer names, so its
exponent is the one Pyth published. It takes Pyth changing the feed's exponent, or a signer setting
bounds at the wrong one.

**The fix.** The gate takes an `exponent` input, an i64 because inputs have no i32 type. It
requires the account's exponent, read as an i64, to equal it (`priceExponentIsExpected`), right
after `priceIsTheExpectedFeed`. The moved account now fails there. With `exponent` −7 it lands,
which shows that nothing else reads the exponent. The oracle swap reads the exponent and scales by
it instead; the gate's bounds are the caller's own numbers, so it pins the exponent they assume.

### P1, fixed: the route's platform fee was the builder's

**The problem.** The gate's action is a Jupiter `route`, which pays `platform_fee_bps` of its
output to the platform fee account at position 6 of its list. Both arrived in what the run's
builder supplied, `actionData` and the group, and nothing checked either.

**What it allowed.** In band, with a 100 bps fee paid to an attacker's USDC account at
`slippageBps` 200, the unfixed gate **landed** (82,569 CU, 897 bytes): the red run of
`a_hostile_platform_fee_fails_at_platform_fee_within_cap`.

**The fix.** The gate declares `MAX_PLATFORM_FEE_BPS`, 0, and requires `platformFeeBps` to be at
most it (`platformFeeWithinCap`) right before the route. No expression reads a byte out of a
`bytes` input, so the gate could not read the fee out of `actionData`; it now takes the route in
parts, `routePlan`, `inAmount`, `quotedOutAmount`, `slippageBps` and `platformFeeBps`, as
`splitJupiterRoute` splits it. The same run now fails there after 3,777 CU, before Jupiter runs.
See [`platform-fee.md`](platform-fee.md).

### Not exercised here

These runs never fail `publishersAgree` (confidence) or `priceIsFullyVerified`. Both are single
reads of the price account, like the requirements above.

## Stale docs

`docs/` was out of bounds for this change. On `docs/examples/protocols/pyth-gate.md`:
- It says the gate is "not yet run against Jupiter" and that "no test calls Jupiter". These tests
  run it against the real Jupiter, Meteora and Pyth receiver programs.
  `docs/examples/protocols/index.md` says the same of every template, under "Running against the
  protocols" and "Jupiter calls".
- Its list of what the gate requires lacks the feed pin and the exponent pin. It never mentions the
  `feedId` and `exponent` inputs a run must supply, or that the bounds' raw units are at `exponent`.

The page's Rust tab is included from `protocol_runs.rs#plain`, so it shows the `exponent` input
without a docs change.
