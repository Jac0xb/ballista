# Mental model

Ballista has three phases: author, finalize, and run.

## Author

The TypeScript API validates the authoring document with Zod and compiles names and expressions
into register-based bytecode. Protocol helpers such as `systemTransfer` and `tokenTransfer` are
convenience functions that emit ordinary generic CPI descriptors.

## Finalize

The program verifies the complete payload once before marking the template immutable:

1. Every table consumes exactly its declared byte range.
2. Every opcode and type is known.
3. Every register read follows an initialized write.
4. Account indices and CPI privileges fit their schemas.
5. The loop is forward-only, singular, and bounded.
6. Worst-case CPI count and generated data fit hard limits.

## Run

Run borrows the finalized record tables directly from account memory. It checks typed inputs and
runtime account bindings, fills a bounded register file, and invokes downstream programs. A failed
guard or CPI rolls back the entire Solana transaction.

```text
┌───────────── immutable template PDA ──────────────┐
│ header │ schemas │ VM records │ CPI tables │ blob │
└──────────────────── borrowed ──────────────────────┘
                           │
                   bounded registers
                           │
              generic CPI + outer signers
```

## `let` is not storage

`step.let('before', expression)` evaluates once and names the result register for later compiler
references. The name does not enter the wire format. `step.snapshot` is the same operation with a
name that communicates its usual role in pre/post checks.

## Accounts are capabilities

An account schema declares the maximum privilege a template may use. A CPI descriptor may request
only a subset. Ballista never creates signer authority; it can forward only signers already present
in the transaction.
