# Example cookbook

These examples are finite orchestration patterns, not protocol promises. Each assumes the named
downstream program exposes the relevant instruction and that the caller supplies every required
signer. TypeScript tabs author templates; Rust tabs demonstrate how a Rust service binds inputs and
accounts to the same compiled artifact.

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

::: tip Why the Rust examples look different
The Rust SDK intentionally focuses on PDA, lifecycle, account decoding, and run instruction codecs.
The deterministic authoring compiler currently lives in TypeScript. Export its bytes as a build
artifact, then upload or invoke that same artifact from Rust.
:::
