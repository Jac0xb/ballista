# Security posture

What protects a template, what does not, and what has and has not been reviewed.

## Audit status

**Ballista has not been audited.** No third party has reviewed the program or the SDKs. The program
deployed on devnet is a pre-release build that still has an upgrade authority. Treat Ballista as
unaudited software and keep the value it controls to what you can afford to lose.

## Checked on chain

The Ballista program enforces these for every template, whichever SDK built it.

**At upload** (the verifier, before a template is finalized):

- the bytes are well formed, with no unknown instructions or reserved bits;
- every value is set before it is read, and has the type each instruction expects;
- every account reference points at a declared account;
- no call passes a declared account as signer or writable unless its declaration requires that.
  [Account-group](/guide/account-groups) members are the exception: they have no declaration and
  are passed with the transaction's own writable flag, never as signers;
- a program that is called is declared `executable`;
- at most eight loops, never nested, each with a fixed maximum, and at most 64 calls even in the
  worst case;
- fixed-offset reads stay within the account's declared minimum length.

**At every run:**

- each account matches its declaration: signer, writable, executable, address, owner and minimum
  length;
- inputs decode exactly, and the account and row counts are in range;
- arithmetic and casts are checked, and a failed `require` stops the run;
- the template is finalized, and the run never writes to it.

## Checked only by the TypeScript compiler

These rules help you write a safe template, but the program does not enforce them. A template built
with the Rust builder, or by hand, can skip them, so check them when you
[inspect a template](/guide/inspecting-templates) someone else wrote.

- A called program must pin its `address`, so the caller cannot swap in another program.
- An account whose data is read must pin its `owner` or `address`, so the byte offsets mean what
  the template assumes.
- `unsafeUnpinned: true` turns both off for one account. The flag is not stored on chain.
- The TypeScript caps on steps, rows and data parts. See [Limits](/reference/limits).

## Formal verification

14 rules are set up for the Certora Solana Prover: `u64` and `i64` arithmetic, comparisons, casts,
error codes and the template parser. 13 more, covering account checks, the template lifecycle and
type safety, are written but blocked by prover limitations, so they are not proved.
[Formal verification](/guide/formal-verification) has the details.

## Strengths

- **Errors pass through.** A called program's error reaches you unchanged. Ballista never relabels
  it.
- **Flat heap.** A run allocates its buffers once and reuses them for every call, so memory use does
  not grow with the number of calls.
- **Few `unsafe` blocks.** The release program uses `unsafe` only around Solana system calls (logs,
  return data, hashing and the CPI itself), to fill a CPI's account list in place, and to read the
  Instructions sysvar and read-only accounts without copying them.
- **No authority of its own.** Ballista never signs as a PDA, holds no funds, and keeps no state
  between runs. A template can only do what the transaction's own signers could do directly.
- **Immutable templates.** A finalized template cannot be changed, so what you reviewed is what
  runs.

See also [Trust model](/guide/trust-model) and [Failure modes and recovery](/guide/failure-modes).
