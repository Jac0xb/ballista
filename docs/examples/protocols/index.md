# Live protocols

Twelve templates against real programs. Each is a source file in
`clients/js/examples/protocols/`, compiled in CI and checked against the same verifier the chain
runs before a template can be stored.

| Pattern | Protocol | What the chain decides |
| --- | --- | --- |
| [Deposit a swap's output](/examples/protocols/jupiter-deposit) | Jupiter → Kamino | How much the route produced |
| [Swap checked against an oracle](/examples/protocols/jupiter-oracle-swap) | Jupiter + Pyth | Whether the fill beat an independent price |
| [Sell a whole balance](/examples/protocols/token-sweep) | SPL Token → Jupiter | How much there is to sell |
| [A tip you only pay when you earned it](/examples/protocols/jito-tip) | Jito | Whether the arbitrage covered the bid |
| [Gate on a fresh price](/examples/protocols/pyth-gate) | Pyth | Whether the oracle is fresh, agreed and in band |
| [Compound the fees you collected](/examples/protocols/orca-compound) | Orca | What the position had earned |
| [Harvest only what earned](/examples/protocols/orca-harvest) | Orca | Which positions are worth touching |
| [Repay what the swap produced](/examples/protocols/kamino-repay) | Jupiter → Kamino | The debt, and what the swap returned |
| [Liquidate and prove it paid](/examples/protocols/kamino-liquidate) | Kamino | What the liquidator actually netted |
| [Empty a position, insist it was worth it](/examples/protocols/marginfi-withdraw) | marginfi | How much came out |
| [Rebalance between venues](/examples/protocols/drift-rebalance) | marginfi → Drift | What one released, to deposit in the other |
| [Settle, then withdraw what settled](/examples/protocols/drift-settle) | Drift | Whether settling moved anything |

## Reading offsets from an account

These templates read protocol state at fixed byte offsets, which is the one thing about them
that cannot be checked at build time. A wrong offset is not an error — it is a plausible number
from the wrong field.

`pnpm test:devnet` reads live accounts and checks the offsets decode to something a human would
recognise: a Unix timestamp near now, an exponent between −18 and 0, tick bounds in the right
order. It needs no keypair and no SOL.

That test has already caught one bug. Pyth's `PriceUpdateV2` has a `VerificationLevel` enum near
the front whose `Full` variant serializes to one byte and whose `Partial` variant serializes to
two, so **every field after it sits one byte earlier in a `Full` account**. Deriving offsets from
the struct's `LEN` gives the `Partial` layout, which on devnet is the minority: of 4,000 accounts
sampled, 3,323 were `Full`. The templates now read the verification level first and require
`Full`, which pins the layout and is the stronger guarantee anyway.

| Layout | Field | Offset |
| --- | --- | ---: |
| Pyth `PriceUpdateV2`, `Full` | `verification_level` `u8` | 40 |
| | `price` `i64` | 73 |
| | `conf` `u64` | 81 |
| | `exponent` `i32` | 89 |
| | `publish_time` `i64` | 93 |
| Orca `Position` | `liquidity` `u128` | 72 |
| | `fee_owed_a` `u64` | 112 |
| | `fee_owed_b` `u64` | 136 |
| SPL Token account | `amount` `u64` | 64 |

Anchor discriminators are computed rather than copied: `sha256("global:<handler>")[..8]` for
instructions, `sha256("account:<Name>")[..8]` for accounts. Anchor accounts carry an eight-byte
discriminator, so every offset above includes it.

::: warning Re-derive before you upload
CI checks the Ballista side. It cannot check that a protocol has not upgraded since. Pin each
account's owner and minimum data length, re-derive offsets from the current IDL, and prefer
fields the protocol treats as public API.
:::

## Running them

Templates are authored once in TypeScript and uploaded once. After that a run is one
instruction, usually built by a service in Rust. A runner never sees the bytecode; it needs the
order of the fixed accounts, the order of the inputs, and the row shape if the template batches.

There are only three run shapes across all twelve, all in
`clients/rust/examples/protocol_runs.rs`, and each page below tabs the one it uses.

## What they cost

These callees cannot be benchmarked, but every pattern is one of three shapes the cookbook
measures: [read then call](/examples/runtime-values#repay-exactly-what-is-owed),
[snapshot then check](/examples/token-accounts#exact-token-debit), and
[call only if](/examples/conditional#liquidate-only-when-unhealthy). Budget about 1,000 compute
units for the run, roughly 1,700 per invocation on top of the protocol's own cost, and 100 to 200
per field read. The [compute profile](/cu-profile) has the breakdown.

## Where devnet stops

| Protocol | Devnet | Note |
| --- | --- | --- |
| Orca, Drift, Pyth, Kamino | Deployed | Offsets verified against live accounts |
| Jupiter | Address not executable | Routes need mainnet liquidity anyway |
| marginfi | Absent | Mainnet only |
