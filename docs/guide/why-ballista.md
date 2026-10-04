# Why Ballista?

Ballista stores a sequence of Solana program calls, with checks between them, as a template on
chain. Anyone can run the template later with new inputs, and either every step succeeds or the
whole transaction fails. This page explains when that is worth doing, what it costs, and when to
write your own program instead.

::: warning Not audited
Ballista is on mainnet, but no third party has audited the program or the SDKs. Use it at your own
risk. [Details](/guide/security#audit-status)

<AuditFund />
:::

Most Solana automation starts as client code that builds transactions. As it grows, the same
account ordering, checks, instruction encoding, and sequence of program calls get copied into bots,
frontends, scripts, and backends. The usual next step is a custom program, even when the logic is
short and keeps little or no state of its own. With Ballista, the sequence is stored on chain once,
and every client runs the same checked version.

## What a transaction cannot say

Putting many calls in one transaction is not the reason to use Ballista. Thirty plain SOL transfers
fit in a single version 1 transaction of 1,670 bytes, well inside its 4,096-byte limit. Sent that
way, they need no template, no rent for a template account, and none of the
[compute units](/reference/glossary#compute-units) that a run uses.

The reason is what a transaction cannot express. Instruction data is fixed when the transaction is
signed, so a transaction cannot move *whatever balance is there*, repay *exactly what is owed*,
skip a call that would fail, or make one payment depend on the one before it. A template runs on
chain, reads accounts as it goes, and decides:

- [Amounts read at run time](/guide/runtime-values): read a balance, then spend it.
- [Conditional calls](/guide/conditional): skip a call instead of failing the
  transaction.
- [Loops over rows and counts](/guide/loops): each row decides from what it finds, or steps repeat
  a number of times read during the run.
- [Safety guardrails](/guide/guardrails): check the result after a call returns, and undo
  everything if the check fails.

A guardrail can also check a change, such as how much a balance dropped, more cheaply than
[Lighthouse](https://github.com/Jac0xb/lighthouse). Lighthouse stores the before value in a memory
account, which takes an extra instruction and rent; a template keeps it in the run with
`step.snapshot`, so the check needs neither.

## What it removes

- **Repeated client code.** Every client stops rebuilding the same instruction sequence.
- **A custom program.** A short, fixed workflow needs no program of your own.
- **Preflight guesses.** Conditions are checked while the transaction runs, not in a simulation
  beforehand.
- **Logic in every transaction.** A run carries only its inputs and accounts.
- **Oversized batches.** One instruction covers every item, instead of one per item.

## What it costs {#cost}

Ballista charges no fee. Besides Solana's usual transaction fees, you pay
[rent](/reference/glossary#rent) once for each account Ballista creates, and compute units on every
run. Rent is 5,080 lamports a byte at today's rate, counting 128 bytes of overhead per account;
`getMinimumBalanceForRentExemption` returns the current figure.

- **Upload.** The creator pays for the template account, an 80-byte header plus the compiled
  template: (128 + 80 + template bytes) × 5,080 lamports. The 216-byte
  [Getting started](/guide/getting-started) template locks 2,153,920 lamports (about 0.0022 SOL);
  the [largest template allowed](/reference/limits) locks about 0.053 SOL. A
  [finalized](/reference/glossary#finalize) template is never closed, so its rent stays locked.
- **Registry entries.** The run that creates an [entry](/guide/registries) pays for it, a 72-byte
  header plus the fields: (128 + 72 + field bytes) × 5,080 lamports. Entries are never closed
  either.
- **Each run.** Measured in the test suite, an empty run takes 581 compute units, one SOL transfer
  2,390, and a 30-row SOL payroll 46,435. In a loop of Jupiter swaps, each pass adds about 3,400 to
  Jupiter's own cost.

## When to write your own program {#boundary}

Ballista makes [CPIs](/reference/glossary#cpi) in a fixed order, with checks between them; it is
not a general smart-contract language. Write your own program for any of these:

- **Custody.** A template can't take custody of funds: it has no authority of its own.
- **PDA signing.** A template can't sign as a [PDA](/reference/glossary#pda); see
  [when Ballista signs](/guide/trust-model#signing).
- **Protocol-owned state.** [Registry entries](/guide/registries) hold small per-template state,
  such as counters, spending limits, allowlists and nonces, so permission rules and replay
  protection fit in a template. A state machine, or state other programs rely on, needs a program.
- **More than about 61 accounts.** A run receives every account it touches from its transaction,
  which caps them; see [Limits](/reference/limits#transaction-ceilings).
- **Deep CPI routes.** A run takes one level of Solana's [call depth](/reference/limits#call-depth),
  leaving one fewer for the programs it calls.
- **Compute-critical or unbounded work.** Every run adds Ballista's own [cost](#cost), and every
  loop has a fixed maximum number of passes.

A finalized template is public, and anyone can run it, but it never runs by itself. Automation that
runs without a user signing needs a delegate or authority model from another program.

## The security bargain

The template language is kept small so that the Ballista program can check a whole template once,
when it is finalized, before anyone runs it. The trust model lists
[what finalization checks](/guide/trust-model#finalization-checks). Pinning a called program's
address is checked only by the SDK compilers, not the program; see [Pins](/guide/trust-model#pins).

What finalization cannot decide is who may run the template. Every run still relies on the
transaction's signers, and on the checks inside the programs it calls, for authorization.

::: tip A rule of thumb
If you can draw the workflow as a short list of program calls with checks between them, plus at
most one table of rows handled the same way, it is a good fit for Ballista.
:::
