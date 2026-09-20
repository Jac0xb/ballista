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

| Pattern | CU per run | Plain CU | Bytes per run | Plain bytes | Template rent | Without a program? |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| [Bounded SOL payroll](/examples/payments#bounded-sol-payroll) | 59,338 | 4,500 | 1,240 | 1,670 | 0.00191 SOL | Yes, same guarantees |
| [Basis-point revenue split](/examples/payments#basis-point-revenue-split) | 5,948 | 300 | 324 | 270 | 0.00297 SOL | Yes, weaker guarantees |
| [Index-weighted rewards](/examples/payments#index-weighted-rewards) | 73,400 | 4,500 | 1,240 | 1,670 | 0.00224 SOL | Yes, weaker guarantees |
| [Deadline refund](/examples/payments#deadline-refund) | 3,450 | 150 | 291 | 220 | 0.00209 SOL | Yes, weaker guarantees |
| [Reserve-preserving sweep](/examples/payments#reserve-preserving-sweep) | 4,257 | 150 | 291 | 220 | 0.00274 SOL | Yes, weaker guarantees |
| [Assert, create, then transfer](/examples/token-accounts#assert-create-then-transfer) | 167,396 | 111,752 | 1,007 | 1,122 | 0.00345 SOL | Yes, same guarantees |
| [Existing-account token payroll](/examples/token-accounts#existing-account-token-payroll) | 64,465 | 2,432 | 1,339 | 1,738 | 0.00195 SOL | Yes, same guarantees |
| [Conditional ATA setup](/examples/token-accounts#conditional-ata-setup) | 16,648 | 13,518 | 407 | 341 | 0.00224 SOL | Yes, same guarantees |
| [Close empty token accounts](/examples/token-accounts#close-empty-token-accounts) | 37,047 | 1,888 | 803 | 842 | 0.00205 SOL | Yes, weaker guarantees |
| [Exact token debit](/examples/token-accounts#exact-token-debit) | 3,849 | 76 | 316 | 250 | 0.00235 SOL | Yes, weaker guarantees |
| [Deadline and minimum output](/examples/guardrails#deadline-and-minimum-output) | 4,069 | 150 | 333 | 240 | 0.00248 SOL | Yes, weaker guarantees |
| [Pinned program and owner](/examples/guardrails#pinned-program-and-owner) | 2,934 | 150 | 283 | 220 | 0.00183 SOL | Yes, weaker guarantees |
| [Oracle price band](/examples/guardrails#oracle-price-band) | 4,086 | 150 | 324 | 220 | 0.00254 SOL | No, needs a program |
| [Maximum lamport spend](/examples/guardrails#maximum-lamport-spend) | 3,589 | 150 | 283 | 220 | 0.00232 SOL | No, needs a program |
| [Canonical position account](/examples/guardrails#canonical-position-account) | 5,590 | 150 | 285 | 220 | 0.00256 SOL | No, needs a program |
| [Swap then deposit](/examples/composition#swap-then-deposit) | 5,721 | 300 | 385 | 278 | 0.00282 SOL | Yes, weaker guarantees |
| [Claim then distribute](/examples/composition#claim-then-distribute) | 34,827 | 1,366 | 974 | 1,148 | 0.00258 SOL | Yes, same guarantees |
| [Primary or fallback route](/examples/composition#primary-or-fallback-route) | 3,582 | 150 | 345 | 240 | 0.00238 SOL | Yes, weaker guarantees |
| [Time-gated governance execution](/examples/composition#time-gated-governance-execution) | 3,834 | 150 | 342 | 240 | 0.0023 SOL | No, needs a program |
| [Bounded keeper crank](/examples/composition#bounded-keeper-crank) | 51,028 | 3,600 | 1,826 | 2,162 | 0.00194 SOL | Yes, same guarantees |

<!-- /benchmark -->

### Where the bytes go

A recipient's address has to appear in the transaction either way. What a plain transaction adds on
top, for every row, is another instruction envelope: the program index, the account index list, and
the instruction data. Ballista carries those once and repeats only the account index.

A run also pays a fixed 63 bytes for two account keys a plain transaction does not need, the
template account and the Ballista program, so small batches are larger and the lines cross a few
rows in.

<!-- benchmark:chart -->

<svg viewBox="0 0 720 320" role="img" aria-label="Transaction bytes saved against the number of batch rows" style="width:100%;height:auto;max-width:720px">
    <line x1="56" y1="241.4" x2="704" y2="241.4" stroke="currentColor" stroke-opacity="0.45" /><text x="48" y="245.4" text-anchor="end" font-size="12" fill="currentColor" fill-opacity="0.7">0</text>
    <line x1="56" y1="189.0" x2="704" y2="189.0" stroke="currentColor" stroke-opacity="0.12" /><text x="48" y="193.0" text-anchor="end" font-size="12" fill="currentColor" fill-opacity="0.7">100</text>
    <line x1="56" y1="136.6" x2="704" y2="136.6" stroke="currentColor" stroke-opacity="0.12" /><text x="48" y="140.6" text-anchor="end" font-size="12" fill="currentColor" fill-opacity="0.7">200</text>
    <line x1="56" y1="84.1" x2="704" y2="84.1" stroke="currentColor" stroke-opacity="0.12" /><text x="48" y="88.1" text-anchor="end" font-size="12" fill="currentColor" fill-opacity="0.7">300</text>
    <line x1="56" y1="31.7" x2="704" y2="31.7" stroke="currentColor" stroke-opacity="0.12" /><text x="48" y="35.7" text-anchor="end" font-size="12" fill="currentColor" fill-opacity="0.7">400</text>
    <line x1="56" y1="16" x2="56" y2="276" stroke="currentColor" stroke-opacity="0.35" />
    <path d="M 56.0 274.4 L 76.9 265.5 L 97.8 256.6 L 118.7 247.7 L 139.6 238.8 L 160.5 229.9 L 181.4 221.0 L 202.3 212.0 L 223.2 203.1 L 244.1 194.2 L 265.0 185.3 L 285.9 176.4 L 306.8 167.5 L 327.7 158.6 L 348.6 149.7 L 369.5 140.8 L 390.5 131.8 L 411.4 122.9 L 432.3 114.0 L 453.2 105.1 L 474.1 96.2 L 495.0 87.3 L 515.9 78.4 L 536.8 69.5 L 557.7 60.6 L 578.6 51.6 L 599.5 42.7 L 620.4 33.8 L 641.3 24.9 L 662.2 16.0" fill="none" stroke="#2f6f4f" stroke-width="2.5" />
    <path d="M 56.0 276.0 L 76.9 268.1 L 97.8 260.3 L 118.7 252.4 L 139.6 244.5 L 160.5 236.7 L 181.4 228.8 L 202.3 221.0 L 223.2 213.1 L 244.1 205.2 L 265.0 197.4 L 285.9 189.5 L 306.8 181.6 L 327.7 173.8 L 348.6 165.9 L 369.5 158.1 L 390.5 150.2 L 411.4 142.3 L 432.3 134.5 L 453.2 126.6 L 474.1 118.7 L 495.0 110.9 L 515.9 103.0 L 536.8 95.2 L 557.7 87.3 L 578.6 79.4 L 599.5 71.6 L 620.4 63.7 L 641.3 55.8 L 662.2 48.0 L 683.1 40.1 L 704.0 32.3" fill="none" stroke="#8a5a2b" stroke-width="2.5" />
    <text x="56.0" y="296" text-anchor="middle" font-size="12" fill="currentColor" fill-opacity="0.7">1</text>
    <text x="139.6" y="296" text-anchor="middle" font-size="12" fill="currentColor" fill-opacity="0.7">5</text>
    <text x="244.1" y="296" text-anchor="middle" font-size="12" fill="currentColor" fill-opacity="0.7">10</text>
    <text x="348.6" y="296" text-anchor="middle" font-size="12" fill="currentColor" fill-opacity="0.7">15</text>
    <text x="453.2" y="296" text-anchor="middle" font-size="12" fill="currentColor" fill-opacity="0.7">20</text>
    <text x="557.7" y="296" text-anchor="middle" font-size="12" fill="currentColor" fill-opacity="0.7">25</text>
    <text x="662.2" y="296" text-anchor="middle" font-size="12" fill="currentColor" fill-opacity="0.7">30</text>
    <text x="380" y="314" text-anchor="middle" font-size="13" fill="currentColor" fill-opacity="0.8">Rows in the batch</text>
    <text x="14" y="146" text-anchor="middle" font-size="13" fill="currentColor" fill-opacity="0.8" transform="rotate(-90 14 146)">Transaction bytes saved</text>
    <line x1="68" y1="28" x2="92" y2="28" stroke="#2f6f4f" stroke-width="2.5" /><text x="100" y="32" font-size="13" fill="currentColor">SOL transfer</text>
    <line x1="68" y1="48" x2="92" y2="48" stroke="#8a5a2b" stroke-width="2.5" /><text x="100" y="52" font-size="13" fill="currentColor">Token transfer</text>
</svg>

Measured: a SOL transfer costs 33 bytes per row through Ballista against 50 plain, breaking even at 5 rows; a token transfer costs 33 bytes per row through Ballista against 48 plain, breaking even at 6 rows.

<!-- /benchmark -->

### Where the compute goes

Most of the difference is not interpretation. Measured on templates that do nothing but repeat a
System transfer:

| Calls in the template | Compute units |
| ---: | ---: |
| 0 | 1,326 |
| 1 | 2,913 |
| 2 | 4,670 |
| 3 | 6,429 |
| 4 | 8,182 |

A run starts at about 1,300 compute units, which covers parsing the stored template, checking every
runtime account against its schema, and allocating the register file. Each call then adds about
1,757, against 150 for the same System transfer sent as a plain instruction. That gap splits three
ways: Solana charges a flat 1,000 units for any cross-program invocation, the callee still costs
its 150, and the remaining 600 or so is Ballista assembling the call from the template.

[The compute profile](/cu-profile) breaks this down further, one feature at a time, and names where
the cost could come out. The 1,000-unit charge is what any composing program pays. A hand-written Rust program that made the
same calls would pay it too, so it is the price of doing the work inside a program at all rather
than the price of a template. What a template adds on top is the fixed 1,300 and roughly 600 per
call, plus a few hundred per batch row and more where a pattern derives a PDA.

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
