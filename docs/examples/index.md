# Example cookbook

These examples are finite orchestration patterns, not protocol promises. Each assumes the named
downstream program exposes the relevant instruction and that the caller supplies every required
signer.

Start with [Author and run in both languages](/examples/end-to-end), which shows one template four
ways: authored in TypeScript, run from TypeScript, authored in Rust with the shared builder, and
run from Rust with typed inputs. The patterns below use the same four tabs where the Rust side
differs, and template plus run tabs where it does not.

## Cost and alternatives

Every pattern below is measured: what one run costs in compute units, what the transaction weighs,
and what the same work costs as plain instructions. The last column is the question worth asking
first. **Yes, same guarantees** means a plain transaction already does this and Ballista buys one
instruction and a stored, verified shape. **Yes, weaker guarantees** means the instructions can be
sent, but a check Ballista performs on chain falls to whoever builds the transaction. **No, needs a
program** means no instruction sequence expresses it, because the decision depends on state read
during execution.

<!-- benchmark:summary -->

| Pattern | CU per run | Plain CU | Bytes per run | Plain bytes | Template rent | Without a program? |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| [Bounded SOL payroll](/examples/payments#bounded-sol-payroll) | 16,712 | 1,200 | 514 | 570 | 0.00191 SOL | Yes, same guarantees |
| [Basis-point revenue split](/examples/payments#basis-point-revenue-split) | 5,948 | 300 | 324 | 270 | 0.00297 SOL | Yes, weaker guarantees |
| [Index-weighted rewards](/examples/payments#index-weighted-rewards) | 20,500 | 1,200 | 514 | 570 | 0.00224 SOL | Yes, weaker guarantees |
| [Deadline refund](/examples/payments#deadline-refund) | 3,450 | 150 | 291 | 220 | 0.00209 SOL | Yes, weaker guarantees |
| [Reserve-preserving sweep](/examples/payments#reserve-preserving-sweep) | 4,257 | 150 | 291 | 220 | 0.00274 SOL | Yes, weaker guarantees |
| [Assert, create, then transfer](/examples/token-accounts#assert-create-then-transfer) | 105,512 | 66,376 | 743 | 758 | 0.00345 SOL | Yes, same guarantees |
| [Existing-account token payroll](/examples/token-accounts#existing-account-token-payroll) | 17,074 | 608 | 547 | 586 | 0.00195 SOL | Yes, same guarantees |
| [Conditional ATA setup](/examples/token-accounts#conditional-ata-setup) | 16,648 | 13,518 | 407 | 341 | 0.00224 SOL | Yes, same guarantees |
| [Close empty token accounts](/examples/token-accounts#close-empty-token-accounts) | 10,160 | 472 | 407 | 362 | 0.00205 SOL | Yes, weaker guarantees |
| [Exact token debit](/examples/token-accounts#exact-token-debit) | 3,849 | 76 | 316 | 250 | 0.00235 SOL | Yes, weaker guarantees |
| [Deadline and minimum output](/examples/guardrails#deadline-and-minimum-output) | 4,069 | 150 | 333 | 240 | 0.00248 SOL | Yes, weaker guarantees |
| [Pinned program and owner](/examples/guardrails#pinned-program-and-owner) | 2,934 | 150 | 283 | 220 | 0.00183 SOL | Yes, weaker guarantees |
| [Oracle price band](/examples/guardrails#oracle-price-band) | 4,086 | 150 | 324 | 220 | 0.00254 SOL | No, needs a program |
| [Maximum lamport spend](/examples/guardrails#maximum-lamport-spend) | 3,589 | 150 | 283 | 220 | 0.00232 SOL | No, needs a program |
| [Canonical position account](/examples/guardrails#canonical-position-account) | 7,090 | 150 | 285 | 220 | 0.00256 SOL | No, needs a program |
| [Swap then deposit](/examples/composition#swap-then-deposit) | 5,721 | 300 | 385 | 278 | 0.00282 SOL | Yes, weaker guarantees |
| [Claim then distribute](/examples/composition#claim-then-distribute) | 19,030 | 758 | 710 | 764 | 0.00258 SOL | Yes, same guarantees |
| [Primary or fallback route](/examples/composition#primary-or-fallback-route) | 3,582 | 150 | 345 | 240 | 0.00238 SOL | Yes, weaker guarantees |
| [Time-gated governance execution](/examples/composition#time-gated-governance-execution) | 3,834 | 150 | 342 | 240 | 0.0023 SOL | No, needs a program |
| [Bounded keeper crank](/examples/composition#bounded-keeper-crank) | 9,032 | 600 | 506 | 370 | 0.00193 SOL | Yes, same guarantees |

<!-- /benchmark -->

[How these are measured](/benchmarks#example-cost-tables). Ballista is never cheaper in compute:
budget roughly 1,900 units per batch row on top of the calls themselves. It is often cheaper in
bytes for batches, and the guards travel with the template rather than with the client.

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
