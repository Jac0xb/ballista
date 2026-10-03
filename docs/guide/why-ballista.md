# Why Ballista?

Ballista stores a sequence of Solana program calls, with checks between them, as a template on
chain. Anyone can run the template later with new inputs, and either every step succeeds or the
whole transaction fails. This page explains when that is worth doing, what a template can do that a
plain transaction cannot, and when to write your own program instead.

Most Solana automation starts as client code that builds transactions. As it grows, the same
account ordering, checks, instruction encoding, and sequence of program calls get copied into bots,
frontends, scripts, and backends. The usual next step is a custom program, even when the logic is
short and keeps little or no state of its own. With Ballista, the sequence is stored on chain once, and every
client runs the same checked version.

## What a transaction cannot say

Putting many calls in one transaction is not the reason to use Ballista. Thirty plain SOL transfers
fit in a single [version 1 transaction](/guide/transaction-v1) of 1,670 bytes, well inside its
4,096-byte limit. Sent that way, they need no template, no rent for a template account, and none of
the compute units (Solana's measure of execution cost) that a run uses.

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

## What it removes

- Rebuilding the same instruction sequence in every client.
- Writing and deploying a custom program for a short, fixed workflow.
- Relying on a simulation before sending (a preflight) for conditions the template can check while
  the transaction executes.
- Sending the workflow's logic with every run: a run carries only its inputs and accounts.
- Client-side loops that add one top-level instruction per item until the transaction is too large.

## What it deliberately does not replace

Ballista is not a general smart-contract language. It makes CPIs (cross-program invocations: one
program calling another) in a fixed order, with checks between them. Write your own program when you
need to hold funds (custody), sign as a PDA (program-derived address: an address a program controls,
with no private key), keep private state that changes between transactions, run your own accounting
or permission rules, prevent replays, or loop in ways that cannot be bounded in advance.

| Requirement | Ballista | Dedicated program |
| --- | --- | --- |
| Reusable, fixed sequence of CPIs | Excellent fit | Works, but more code |
| Checks on accounts and the clock during execution | Built in | Custom implementation |
| Bounded loops, over a list of rows or a counted number of passes | Built in | Custom implementation |
| Small state between runs: counters, spending limits, allowlists | Built in, as [registry entries](/guide/registries) | Custom implementation |
| Protocol-owned state machine | No | Yes |
| Sign as a program PDA | No | Yes |
| Unbounded loops | No | Possible, within compute limits |

## Where the boundary sits

- Every account a call touches must still be listed in the transaction. Only the template's bytes
  stay on chain; each run supplies its own inputs and accounts.
- A finalized template (one that has been checked and locked on chain) is public, and anyone can
  run it. Authority comes from the transaction's signers and from the checks inside the programs the
  template calls.
- A template never runs on its own, signs as a PDA, or spends assets of its own. Automation that
  runs without a user signing needs a delegate or authority model from another program.
- A template has at most eight loops, never nested, each with a fixed maximum number of passes, so
  the work a run can do is always bounded.

## The security bargain

The template language is kept small so that the Ballista program can check a whole template once,
when it is finalized, before anyone runs it. Finalization proves that the template:

- always finishes;
- never reads a value before setting it;
- refers only to accounts it declares;
- never passes a declared account to a CPI as a signer (an account that signed the transaction) or
  as writable (allowed to change) unless the account's declaration requires that privilege.
  [Account group](/guide/account-groups) members, which have no declaration, are passed as writable
  when the transaction marked them writable, and never as signers;
- stays within fixed limits on the number of CPIs and the size of their data, even in the worst
  case.

Finalization does not check that a called program's address is pinned; only the TypeScript
compiler requires that. See [Pins](/guide/trust-model#pins).

What finalization cannot decide is who may run the template. Every run still relies on the
transaction's signers, and on the checks inside the programs it calls, for authorization.

::: tip A rule of thumb
If you can draw the workflow as a short list of program calls with checks between them, plus at
most one table of rows handled the same way, it is a good fit for Ballista.
:::
