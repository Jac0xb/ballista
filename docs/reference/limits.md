# Limits

This page lists every hard limit on a template and on a run, and says when each one is checked. The
values come from `common/src/template/wire.rs`.

Terms used below:

- **Runtime accounts:** the accounts passed to a run after the template account: fixed accounts,
  then batch rows, then account group members.
- **CPI:** cross-program invocation, a call from the template to another program. "CPIs per run"
  counts each loop iteration separately, at the template's maximum number of rows.
- **Register:** a numbered slot that holds one value during a run.
- **VM instruction:** one 16-byte instruction of the compiled bytecode.
- **PDA:** program-derived address, an address computed from a program ID and a list of seeds.
  The bump is one extra seed byte that makes the result a valid program address.

| Resource | Hard limit |
| --- | ---: |
| Compiled template payload | 10,240 bytes |
| Runtime accounts per run | 120 |
| Inputs, fixed plus row | 32 |
| Row inputs per batch row | 0 to 8 |
| Input values per run (fixed plus rows) | 256 |
| Run data (account group lengths plus input values) | 1,024 bytes |
| Account groups | 8, up to 255 members each |
| Registers | 64 |
| VM instructions | 1 to 128 |
| CPIs per run, counting each loop iteration | 64 |
| Accounts per CPI, including a forwarded account group | 64 |
| Instruction data per CPI | 4,096 bytes |
| Accounts per batch row | 1 to 8 |
| Minimum batch rows | 0 to the maximum |
| PDA seeds per derivation, not counting the bump | 15 |
| Bytes per PDA seed | 32 |
| Readable CPI return data | 1,024 bytes |

## TypeScript SDK limits

The TypeScript SDK adds four limits of its own. Templates built with the Rust builder are bound only
by the table above; there, the maximum number of batch rows is limited by the 120 runtime accounts,
and by the 64 CPIs per run when the loop body invokes a program.

| Resource | Limit |
| --- | ---: |
| Top-level steps | 128 |
| Steps in a loop body | 64 |
| Batch rows (`maxIterations`) | 60 |
| Data parts per CPI | 64 |

## Static versus runtime

Static limits are checked before any run: by the compiler, and by the program's verifier when the
template is created or finalized. They reject a template that could exceed a limit in its worst
case:

- register and instruction counts;
- the number of inputs and account groups, and the runtime accounts needed at the maximum number
  of rows;
- CPIs per run, with the loop at its maximum number of rows;
- accounts listed in each CPI;
- PDA seed counts and seed sizes;
- the largest instruction data each CPI could produce;
- fixed-offset reads that extend past the account's declared minimum data length;
- carried registers that change type inside the loop.

Runtime limits depend on what the caller supplies, so `Run` checks them on every run:

- the size and encoding of the run data, and each input value;
- the number of runtime accounts, at most 120;
- the number of rows, which must divide evenly and fall between the template's minimum and
  maximum;
- account group sizes against the accounts supplied;
- each fixed and row account's signer, writable, and executable flags, owner, address, and data
  length (account group members have no constraints);
- each CPI's listed accounts plus its forwarded account group, against the 64-account limit.

## Heap

A run allocates its decoded inputs, its register file, and one set of buffers for building CPIs,
and reuses those buffers for every CPI. PDA seeds are assembled on the stack. Heap use therefore
does not grow with the number of CPIs, and stays within Solana's default 32 KiB heap even for 64
CPIs of 4 KiB each.

## Transaction ceilings

Solana also limits the transaction as a whole. A version 1 transaction can be up to 4,096 bytes
and list at most 64 account addresses. It cannot use address lookup tables, which are on-chain
lists of addresses that a transaction refers to by index instead of listing each address in full.
Besides the runtime accounts, the template account and the Ballista program count toward those 64,
as does the fee payer when it is not one of the runtime accounts, and so do the accounts of any
other instruction in the transaction. Ballista allows up to 120 runtime accounts, but a run with
more than about 60 needs a version 0 transaction with an address lookup table, which is limited to
1,232 bytes.

Compute cost, measured in compute units (Solana's measure of execution cost), depends on the
template. PDA bump searches, account data reads, CPIs, and the work of the invoked programs all add
to it. Simulate the exact transaction and set its compute-unit limit
from the measurement plus a margin; the TypeScript SDK's `createComputeUnitProvider` does this.

## When to split a workflow

If a template approaches several of these limits at once, split it or redesign it. Several small
templates are easier to review than one that does everything, especially when the steps do not
need to succeed or fail together.
