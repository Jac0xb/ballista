# Mental model

This page describes the three stages of a template's life: you write it (author), the Ballista
program checks and locks it (finalize), and anyone runs it (run).

## Author

You write a template in TypeScript, or in Rust with `ProgramBuilder`. The TypeScript SDK checks the
template's structure with the Zod validation library, then compiles it into bytecode: a compact list
of fixed-size instructions that refer to accounts and inputs by position and keep working values in
numbered slots called registers.

Helpers such as `systemTransfer` and `tokenTransfer` are shortcuts. Each one produces an ordinary
CPI (cross-program invocation: one program calling another) with the target program's accounts and
instruction data filled in.

## Finalize

Before a template is locked, the program checks the complete template once:

1. Every section is exactly as long as the template's header says.
2. Every instruction and value type is one the program knows.
3. No instruction reads a register before an earlier instruction has set it.
4. Every account reference points at a declared account, and no CPI passes an account as a signer
   (an account that signed the transaction) or as writable (allowed to change) unless the account's
   declaration requires that privilege.
5. There is at most one loop. It only moves forward and has a fixed maximum number of rows.
6. Even in the worst case, the template makes at most 64 CPIs, and none carries more than 4,096
   bytes of data.

A template that passes is locked: it can never be changed or closed.

## Run

A run reads the stored template directly from its account, without copying it. It checks the
caller's input values and accounts against the template's declarations, works through the steps
using at most 64 registers, and calls other programs. Those calls carry only the signatures of the
outer transaction. If a check or a call fails, the whole Solana transaction is rolled back.

```text
┌───────────── immutable template PDA ──────────────┐
│ header │ schemas │ VM records │ CPI tables │ blob │
└──────────────────── borrowed ──────────────────────┘
                           │
                   bounded registers
                           │
              generic CPI + outer signers
```

The diagram shows the template account. It sits at a PDA (program-derived address: an address owned
by a program, with no private key) of the Ballista program. Its sections are:

- **header**: the format version, the template's settings, and the size of every other section.
- **schemas**: the declared accounts and inputs.
- **VM records**: the compiled instructions, one fixed-size record each. VM stands for the small
  virtual machine that runs them.
- **CPI tables**: each CPI's program, account list, and data layout.
- **blob**: literal bytes, such as the bytes that identify an instruction to the program it calls.

*Borrowed* means a run reads these sections in place. It keeps its working values in a fixed number
of registers, and it makes each CPI with the outer transaction's signers only.
