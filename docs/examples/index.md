# Example cookbook

Batching is not the reason to use Ballista. Thirty transfers already fit in a 1,240-byte
transaction, well inside the 4,096-byte limit, and a plain transaction sends them without a
template, without rent, and without the compute a run costs. If a pattern is just "do these N
things", send N instructions.

The reason is everything a transaction cannot say. Instruction data is fixed at signing, so a
transaction cannot move *what is there*, repay *what is owed*, skip a call that would fail, or
let one row's payment depend on the last. A template runs on chain, reads state as it goes, and
decides.

The cookbook is ordered that way. The patterns at the top are the ones with no plain equivalent:

- [Amounts nobody knows at signing](/examples/runtime-values) — read the balance, then spend it
- [Work that should not always happen](/examples/conditional) — skip a call instead of reverting
- [Loops that read as they go](/examples/loops) — each row decides from what it finds
- [Safety guardrails](/examples/guardrails) — prove something after the call returns
- [Protocol composition](/examples/composition) — chain protocols on runtime values

Below those come the patterns a transaction can already express, kept because they are common and
because the measurements are the honest answer to "should I use this here": [payments](/examples/payments),
[token accounts](/examples/token-accounts).

New to the SDK? [Author and run in both languages](/examples/end-to-end) shows one template four
ways: authored in TypeScript, run from TypeScript, authored in Rust with the shared builder, and
run from Rust with typed inputs.

## Cost and alternatives

Every pattern is measured: what one run costs in compute units, what the transaction weighs, and
what the same work costs as plain instructions. The last column is the question worth asking
first, and the table is sorted by it.

**No, needs a program** means no instruction sequence expresses it, because the decision depends
on state read during execution. **Yes, weaker guarantees** means the instructions can be sent,
but a check Ballista performs on chain falls to whoever builds the transaction. **Yes, same
guarantees** means a plain transaction already does this, and Ballista buys one instruction and a
stored, verified shape — read those rows as a cost, not a pitch.

<!-- benchmark:summary -->

| Pattern | CU per run | Plain CU | Bytes per run | Plain bytes | Template rent | Without a program? |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| [Sweep above a reserve](/examples/runtime-values#sweep-above-a-reserve) | 3,437 | 150 | 283 | 220 | 0.00215 SOL | No, needs a program |
| [Forward the whole token balance](/examples/runtime-values#forward-the-whole-token-balance) | 3,350 | 76 | 308 | 250 | 0.00209 SOL | No, needs a program |
| [Repay exactly what is owed](/examples/runtime-values#repay-exactly-what-is-owed) | 3,252 | 150 | 308 | 220 | 0.00201 SOL | No, needs a program |
| [Split what arrived](/examples/runtime-values#split-what-arrived) | 5,754 | 300 | 324 | 270 | 0.00272 SOL | No, needs a program |
| [Claim only when there is something](/examples/conditional#claim-only-when-there-is-something) | 3,336 | 150 | 308 | 220 | 0.00209 SOL | No, needs a program |
| [Liquidate only when unhealthy](/examples/conditional#liquidate-only-when-unhealthy) | 3,435 | 150 | 316 | 220 | 0.00211 SOL | No, needs a program |
| [Top up only when low](/examples/conditional#top-up-only-when-low) | 3,376 | 150 | 291 | 220 | 0.00209 SOL | No, needs a program |
| [Initialize only if missing](/examples/conditional#initialize-only-if-missing) | 2,956 | 150 | 275 | 220 | 0.00189 SOL | No, needs a program |
| [Waterfall until the money runs out](/examples/loops#waterfall-until-the-money-runs-out) | 23,452 | 1,200 | 578 | 570 | 0.00258 SOL | No, needs a program |
| [Consolidate only the funded accounts](/examples/loops#consolidate-only-the-funded-accounts) | 18,611 | 608 | 539 | 586 | 0.00209 SOL | No, needs a program |
| [Crank only the ripe entries](/examples/loops#crank-only-the-ripe-entries) | 19,209 | 1,200 | 506 | 570 | 0.00213 SOL | No, needs a program |
| [Distribute a runtime pot pro rata](/examples/loops#distribute-a-runtime-pot-pro-rata) | 21,022 | 1,200 | 578 | 570 | 0.00242 SOL | No, needs a program |
| [Oracle price band](/examples/guardrails#oracle-price-band) | 4,081 | 150 | 324 | 220 | 0.00254 SOL | No, needs a program |
| [Maximum lamport spend](/examples/guardrails#maximum-lamport-spend) | 3,624 | 150 | 283 | 220 | 0.00232 SOL | No, needs a program |
| [Canonical position account](/examples/guardrails#canonical-position-account) | 5,630 | 150 | 285 | 220 | 0.00256 SOL | No, needs a program |
| [Time-gated governance execution](/examples/composition#time-gated-governance-execution) | 3,828 | 150 | 342 | 240 | 0.0023 SOL | No, needs a program |
| [Basis-point revenue split](/examples/payments#basis-point-revenue-split) | 5,729 | 300 | 324 | 270 | 0.00272 SOL | Yes, weaker guarantees |
| [Index-weighted rewards](/examples/payments#index-weighted-rewards) | 68,356 | 4,500 | 1,240 | 1,670 | 0.00224 SOL | Yes, weaker guarantees |
| [Deadline refund](/examples/payments#deadline-refund) | 3,480 | 150 | 291 | 220 | 0.00209 SOL | Yes, weaker guarantees |
| [Reserve-preserving sweep](/examples/payments#reserve-preserving-sweep) | 4,097 | 150 | 291 | 220 | 0.00258 SOL | Yes, weaker guarantees |
| [Close empty token accounts](/examples/token-accounts#close-empty-token-accounts) | 34,064 | 1,888 | 803 | 842 | 0.00205 SOL | Yes, weaker guarantees |
| [Exact token debit](/examples/token-accounts#exact-token-debit) | 3,780 | 76 | 316 | 250 | 0.00227 SOL | Yes, weaker guarantees |
| [Deadline and minimum output](/examples/guardrails#deadline-and-minimum-output) | 4,064 | 150 | 333 | 240 | 0.00248 SOL | Yes, weaker guarantees |
| [Pinned program and owner](/examples/guardrails#pinned-program-and-owner) | 2,959 | 150 | 283 | 220 | 0.00183 SOL | Yes, weaker guarantees |
| [Swap then deposit](/examples/composition#swap-then-deposit) | 5,783 | 300 | 385 | 278 | 0.00282 SOL | Yes, weaker guarantees |
| [Primary or fallback route](/examples/composition#primary-or-fallback-route) | 3,512 | 150 | 345 | 240 | 0.0023 SOL | Yes, weaker guarantees |
| [Bounded SOL payroll](/examples/payments#bounded-sol-payroll) | 51,741 | 4,500 | 1,240 | 1,670 | 0.00191 SOL | Yes, same guarantees |
| [Assert, create, then transfer](/examples/token-accounts#assert-create-then-transfer) | 191,260 | 123,752 | 1,007 | 1,122 | 0.00345 SOL | Yes, same guarantees |
| [Existing-account token payroll](/examples/token-accounts#existing-account-token-payroll) | 55,183 | 2,432 | 1,339 | 1,738 | 0.00195 SOL | Yes, same guarantees |
| [Conditional ATA setup](/examples/token-accounts#conditional-ata-setup) | 16,658 | 13,518 | 407 | 341 | 0.00224 SOL | Yes, same guarantees |
| [Claim then distribute](/examples/composition#claim-then-distribute) | 30,432 | 1,366 | 974 | 1,148 | 0.00258 SOL | Yes, same guarantees |
| [Bounded keeper crank](/examples/composition#bounded-keeper-crank) | 45,396 | 3,600 | 1,826 | 2,162 | 0.00194 SOL | Yes, same guarantees |

<!-- /benchmark -->

[How these are measured](/benchmarks#example-cost-tables). Ballista is never cheaper in compute. A
run starts at about 1,300 units and each call it makes costs about 1,757 against 150 for the same
call sent plainly, most of that gap being the flat 1,000 units Solana charges any program for a
cross-program invocation. See [where the compute goes](/benchmarks#where-the-compute-goes). It is
often cheaper in bytes for batches, and the guards travel with the template rather than the client.

## Payments

| Pattern | Core primitives |
| --- | --- |
| [Bounded SOL payroll](/examples/payments#bounded-sol-payroll) | account rows, loop, System CPI |
| [Basis-point revenue split](/examples/payments#basis-point-revenue-split) | checked math, multiple CPIs |
| [Index-weighted rewards](/examples/payments#index-weighted-rewards) | `loopIndex`, checked multiply |
| [Deadline refund](/examples/payments#deadline-refund) | clock, guard |
| [Reserve-preserving sweep](/examples/payments#reserve-preserving-sweep) | snapshot, require, `min` |

## Token accounts

| Pattern | Core primitives |
| --- | --- |
| [Assert, create, then transfer](/examples/token-accounts#assert-create-then-transfer) | PDA assertion, emptiness guard, two CPIs |
| [Existing-account token payroll](/examples/token-accounts#existing-account-token-payroll) | owner constraints, batch transfer |
| [Conditional ATA setup](/examples/token-accounts#conditional-ata-setup) | stride-two rows, guarded Create |
| [Close empty token accounts](/examples/token-accounts#close-empty-token-accounts) | fixed data read, guarded CPI |
| [Exact token debit](/examples/token-accounts#exact-token-debit) | snapshot, post-CPI invariant |

## Safety guardrails

| Pattern | Core primitives |
| --- | --- |
| [Deadline and minimum output](/examples/guardrails#deadline-and-minimum-output) | `AND`, clock, comparisons |
| [Pinned program and owner](/examples/guardrails#pinned-program-and-owner) | account schema constraints |
| [Oracle price band](/examples/guardrails#oracle-price-band) | fixed-width account read |
| [Maximum lamport spend](/examples/guardrails#maximum-lamport-spend) | pre/post snapshot |
| [Canonical position account](/examples/guardrails#canonical-position-account) | generic PDA assertion |

## Protocol composition

| Pattern | Core primitives |
| --- | --- |
| [Swap then deposit](/examples/composition#swap-then-deposit) | client bytes, two generic CPIs |
| [Claim then distribute](/examples/composition#claim-then-distribute) | root CPI plus batch loop |
| [Primary or fallback route](/examples/composition#primary-or-fallback-route) | complementary guards |
| [Time-gated governance execution](/examples/composition#time-gated-governance-execution) | clock and state reads |
| [Bounded keeper crank](/examples/composition#bounded-keeper-crank) | repeated generic CPI |

::: tip Two authoring surfaces, one bytecode
The TypeScript compiler and the Rust `ProgramBuilder` emit identical bytes for the same template;
the repository checks this against shared fixtures. The compiler adds pin lints, inferred data
lengths, and a source map; the builder is lower level and leaves those decisions to you. Either
artifact can be uploaded and run from either language.
:::
