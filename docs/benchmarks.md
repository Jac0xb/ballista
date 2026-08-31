# Ballista 0.3 measurements

Measurements are produced by the Agave 4.1-aligned Mollusk 0.14 suite from the compiled SBF
program. They are regression evidence, not cluster-wide fee or latency promises.

| Stored SOL batch | Compute units |
| ---: | ---: |
| 1 transfer | 2,747 |
| 8 transfers | 16,176 |
| 30 transfers | 58,120 |

A 30-recipient Ballista-only v1 message built and measured with Solana Kit 8.2 is **1,240 bytes**
against the v1 limit of 4,096 bytes. Its Run instruction contains 33 account metas (template,
System Program, source, and 30 recipients), and the transaction has 34 unique account keys after
including the Ballista program. Account locks and compute therefore become relevant well before
template bytes, which are stored on-chain and absent from `Run` data.

## Heap and parsing boundary

The 64-slot runtime register file is 2,560 bytes on the 64-bit SBF/host layout. A batch allocates one
additional 2,560-byte snapshot so root registers can be restored around each iteration. Inputs are
allocated only for the declared input count. CPI metadata and generated data use separately bounded
vectors; the latter is statically capped at 4,096 bytes.

Program and account record tables are borrowed directly from immutable template data. `Run` does
not allocate or deserialize an AST. The heap above is execution working state, not a copy of the
stored bytecode.
