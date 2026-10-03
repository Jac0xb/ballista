# Security posture

What protects a template, what does not, and what has and has not been reviewed.

## Audit status

**Ballista has not been audited.** No third party has reviewed the program or the SDKs. The program
deployed on devnet is a pre-release build that still has an upgrade authority. Treat Ballista as
unaudited software and keep the value it controls to what you can afford to lose.

## Checked on chain

The Ballista program checks every template once, at finalization, and checks every run against it,
whichever SDK built the template. [Finalization checks](/guide/trust-model#finalization-checks)
lists both.

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

15 rules are set up for the Certora Solana Prover: `u64` and `i64` arithmetic, comparisons, casts,
error codes and the template parser. 14 more, covering `u128` arithmetic, `multiplyDivide`, account
checks, the template lifecycle and type safety, are written but blocked, so they are not proved.
[Formal verification](/guide/formal-verification) has the details.

## Strengths

- **Errors pass through.** A called program's error reaches you unchanged. Ballista never relabels
  it.
- **Flat heap.** A run allocates its buffers once and reuses them for every call, so memory use does
  not grow with the number of calls.
- **Few `unsafe` blocks.** The release program uses `unsafe` only:
  - around Solana system calls (logs, return data, hashing, the curve check and the CPI itself);
  - to fill a CPI's account list in place;
  - to read the Instructions sysvar and read-only accounts without copying them;
  - to read and write registry entries in place;
  - to build a PDA's seed buffer in place when deriving an address.
- **No authority of its own.** Ballista never signs a template's calls, so a template can do only
  what the transaction's signers could do directly. See [Signing](/guide/trust-model#signing).
- **State only in registry entries.** Only a template's own runs can change its
  [registry entries](/guide/registries), and a run checks each entry before using it. See
  [State](/guide/trust-model#state).
- **Immutable templates.** A finalized template cannot be changed, so what you reviewed is what
  runs.

See also [Trust model](/guide/trust-model) and [Failure modes and recovery](/guide/failure-modes).
