# Security posture

What protects a template, what does not, and what has and has not been reviewed.

## Audit status

**Ballista is on mainnet, and unaudited.** No third party has reviewed the program or the SDKs.
<AuditFund />

- **The devnet build**, at the address the SDKs use by default, is an older pre-release that
  rejects templates from this repository. Its [upgrade authority](/guide/trust-model#deployments)
  can still change it.
- **Each release will be immutable**, deployed with no upgrade authority. See
  [Deployments](/guide/trust-model#deployments).

Treat Ballista as unaudited software, and keep the value it controls to what you can afford to
lose.

## Checked on chain

The Ballista program checks every template once, at finalization, and checks every run against it,
whichever SDK built the template. [Finalization checks](/guide/trust-model#finalization-checks)
lists both, and [Who controls what](/guide/trust-model#who-controls-what) covers what they leave to
you: the transaction builder, the rest of the transaction, and the called programs and their
upgrade authorities.

## Checked by the SDK compilers, not the program

Both SDK compilers, TypeScript `compileTemplate` and Rust `Template::compile`, enforce these rules,
but the program does not. A template built by hand or with other tools can skip them, so check them
before you run a template you did not compile yourself.

- A called program must pin its `address`, so the caller cannot swap in another program.
- An account whose data is read must pin its `owner` or `address`. An owner pin alone doesn't fix
  the account's type: see [Pins](/guide/trust-model#pins).
- `unsafeUnpinned: true` in TypeScript, or `.unsafe_unpinned()` in Rust, turns both off for one
  account. The flag is not stored on chain.
- The SDK caps on steps, rows and data parts. See [Limits](/reference/limits).

## Formal verification

The Certora Solana Prover, Kani, four fuzzers and mutation testing check the program.
[Formal verification](/guide/formal-verification) says what each covers, what none of them
covers, and how to run them.

- **Proved:** 19 Certora rules, at commit `cb2fb2d`: section parsing, `u128` arithmetic, registry
  opens and fields, return-data provenance, and the account checks at run start. That predates the
  current verifier and the account-group opcodes, so the proofs need a re-run.
- **Sampled:** the verifier, the executor, the template lifecycle and the TypeScript compiler, by
  fuzzing, with mutants confirming that the tests catch a deleted check.
- **Not covered:** what called programs do, accounts passed in two slots (in the proofs), the
  deployed binary itself, and a whole template run end to end.
- **Found so far, all fixed:** two stack overflows the standard build tools didn't report; a bug
  in both compilers that could make a cap check inside a loop always pass; and a way for a template
  built without the compilers to call out before opening its registry entry, and lose a write.
- **In CI:** the prover runs only with a `CERTORAKEY` secret, which the repository doesn't have yet.

## Strengths

- **Errors pass through.** A called program's error reaches you unchanged, even a code in
  Ballista's own range. So a code alone doesn't say which program raised it: a callee's `6001` is
  also Ballista's `InvalidTemplateAccount`, and only the logs tell them apart. See
  [Which program failed](/guide/errors-and-events#which-program-failed).
- **No authority of its own.** Ballista never signs a template's calls, so a template can do only
  what the transaction's signers could do directly. See [Signing](/guide/trust-model#signing).
- **State only in registry entries.** Only a template's own runs can change its
  [registry entries](/guide/registries), and a run checks each entry before using it. See
  [State](/guide/trust-model#state).
- **Immutable templates.** A finalized template cannot be changed, so what you reviewed is what
  Ballista runs. That holds for Ballista's part only: the programs a template calls can be upgraded
  by their own authorities, and so can today's devnet build of Ballista.

See also [Trust model](/guide/trust-model) and [Failure modes and recovery](/guide/failure-modes).
