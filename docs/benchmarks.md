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
