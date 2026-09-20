# Measurements

Measurements are produced by the Agave 4.1-aligned Mollusk 0.14 suite from the compiled SBF
program. They are regression evidence, not cluster-wide fee or latency
promises. Run `cargo test --manifest-path tests/ballista/Cargo.toml -- --nocapture` and grep for
`compute units` to reproduce them.

| Scenario | Compute units |
| --- | ---: |
| 1 SOL transfer, plain template with the run event enabled | 2,886 |
| 1 SOL transfer with a guard input and a pre/post balance assertion | 3,505 |
| 8 SOL transfers in a batch | 15,939 |
| 30 SOL transfers in a batch | 56,783 |
| 58 CPIs each carrying 1,000 bytes of data | 104,320 |
| 118 PDA derivations of 15 seeds across 59 rows | 523,473 |
| Nested template: one template running another through CPI | 5,291 |
| Guarded ATA creation, repeat run that skips the CPI | about 1,400 |

The fixed cost of a run rose by several hundred compute units against version 2 while the per-CPI
cost fell slightly. The fixed cost covers checked table lookups, typed error propagation, and the
one-time scratch allocation; the per-CPI saving comes from reusing that scratch instead of
allocating per invocation.

A 30-recipient Ballista-only v1 message built and measured with Solana Kit 8.2 is **1,240 bytes**
against the v1 limit of 4,096 bytes. Its Run instruction contains 33 account metas (template,
System Program, source, and 30 recipients), and the transaction has 34 unique account keys after
including the Ballista program. Account locks and compute therefore become relevant well before
template bytes, which are stored on-chain and absent from `Run` data.

## Example cost tables

Every pattern in the [example cookbook](/examples/) carries a measured table: what the run costs in
compute units, what the transaction weighs, and what the same work costs as plain instructions.

The numbers come from one pipeline. `pnpm benchmarks` compiles each example and measures its
transaction with Solana Kit; the Mollusk benchmark in `tests/ballista` runs the template and the
plain sequence against the same accounts and records compute units;
`node scripts/benchmark-tables.mjs` writes the tables. The System Program is Mollusk's builtin, and
the Token and Associated Token programs are mainnet dumps, so their costs are the current on-chain
ones. Where an example names a third-party protocol, the callee is a System transfer with padded
data: the CPI is real, and the protocol's own work is excluded from both sides of the comparison.

The last column answers the question the tables exist for. A pattern marked **Yes, same
guarantees** is one a plain transaction already handles, where Ballista buys a single instruction
and a stored, verified shape. **Yes, weaker guarantees** means the instructions can be sent, but a
check that Ballista performs on chain is left to whoever builds the transaction. **No, needs a
program** means no instruction sequence expresses it, because the decision depends on state read
during execution.

<!-- benchmark:summary -->

| Pattern | Ballista CU | Plain CU | Ballista bytes | Plain bytes | Without a program? |
| --- | ---: | ---: | ---: | ---: | --- |
| [Bounded SOL payroll](/examples/payments#bounded-sol-payroll) | 16,712 | 1,200 | 514 | 570 | Yes, same guarantees |
| [Basis-point revenue split](/examples/payments#basis-point-revenue-split) | 5,948 | 300 | 324 | 270 | Yes, weaker guarantees |
| [Index-weighted rewards](/examples/payments#index-weighted-rewards) | 20,500 | 1,200 | 514 | 570 | Yes, weaker guarantees |
| [Deadline refund](/examples/payments#deadline-refund) | 3,450 | 150 | 291 | 220 | Yes, weaker guarantees |
| [Reserve-preserving sweep](/examples/payments#reserve-preserving-sweep) | 4,257 | 150 | 291 | 220 | Yes, weaker guarantees |
| [Assert, create, then transfer](/examples/token-accounts#assert-create-then-transfer) | 105,512 | 66,376 | 743 | 758 | Yes, same guarantees |
| [Existing-account token payroll](/examples/token-accounts#existing-account-token-payroll) | 17,074 | 608 | 547 | 586 | Yes, same guarantees |
| [Conditional ATA setup](/examples/token-accounts#conditional-ata-setup) | 16,648 | 13,518 | 407 | 341 | Yes, same guarantees |
| [Close empty token accounts](/examples/token-accounts#close-empty-token-accounts) | 10,160 | 472 | 407 | 362 | Yes, weaker guarantees |
| [Exact token debit](/examples/token-accounts#exact-token-debit) | 3,849 | 76 | 316 | 250 | Yes, weaker guarantees |
| [Deadline and minimum output](/examples/guardrails#deadline-and-minimum-output) | 4,069 | 150 | 333 | 240 | Yes, weaker guarantees |
| [Pinned program and owner](/examples/guardrails#pinned-program-and-owner) | 2,934 | 150 | 283 | 220 | Yes, weaker guarantees |
| [Oracle price band](/examples/guardrails#oracle-price-band) | 4,086 | 150 | 324 | 220 | No, needs a program |
| [Maximum lamport spend](/examples/guardrails#maximum-lamport-spend) | 3,589 | 150 | 283 | 220 | No, needs a program |
| [Canonical position account](/examples/guardrails#canonical-position-account) | 7,090 | 150 | 285 | 220 | No, needs a program |
| [Swap then deposit](/examples/composition#swap-then-deposit) | 5,721 | 300 | 385 | 278 | Yes, weaker guarantees |
| [Claim then distribute](/examples/composition#claim-then-distribute) | 19,030 | 758 | 710 | 764 | Yes, same guarantees |
| [Primary or fallback route](/examples/composition#primary-or-fallback-route) | 3,582 | 150 | 345 | 240 | Yes, weaker guarantees |
| [Time-gated governance execution](/examples/composition#time-gated-governance-execution) | 3,834 | 150 | 342 | 240 | No, needs a program |
| [Bounded keeper crank](/examples/composition#bounded-keeper-crank) | 9,032 | 600 | 506 | 370 | Yes, same guarantees |

<!-- /benchmark -->

Ballista's overhead is the difference column in each table: roughly 1,900 compute units per batch
row, a few thousand for a run's fixed cost, and more where a pattern derives a PDA. Against that,
one run replaces a transaction's worth of instructions and carries its guards with it.

## Heap and parsing boundary

The register file is sized to the template's declared register count, at most 64 slots of 40
bytes. A batch allocates one additional snapshot of the same size so root registers can be restored
around each iteration. One set of CPI scratch buffers is allocated per run and reused by every
invocation: 64 account metas, 64 account views, and a data buffer sized to the largest declared CPI
payload. PDA seeds are assembled in a 480-byte stack buffer.

The two heap-stress scenarios above previously failed with an access violation at the 32 KiB heap
boundary, because the bump allocator never frees and every CPI allocated fresh vectors. They now
run with constant heap.

Program and account record tables are borrowed directly from immutable template data. `Run` does
not allocate or deserialize an AST.

## Stack frames

The program is built for SBPF version 0, which gives every function a fixed 4 KiB stack frame.
The Certora platform tools report frames that exceed it; the regular toolchain does not. Two did:
the CPI path, whose 64-slot account array now lives in a frame of its own, and the return-data
read, which now copies through the syscall into one buffer. Both are under 4 KiB and every
function in the program compiles without a frame warning.
