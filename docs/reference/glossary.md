# Glossary

Short definitions of the words these docs use. Each term has its own link, such as
`/reference/glossary#cpi`.

## Solana terms

### CPI

Cross-program invocation: one program calling another inside the same transaction. Every call a
template makes is a CPI from the Ballista program.

### PDA

Program-derived address: an address computed from a program ID and a list of seeds. It has no
private key, so only that program can sign for it. Ballista never signs as a PDA; templates only
compare addresses with derived ones (`assertPda`).

### ATA

Associated token account: the standard token account for a given wallet and mint, at a PDA of the
Associated Token Account program. `assertAta` checks that an account is the right one.

### Lamports

The smallest unit of SOL. One SOL is 1,000,000,000 lamports.

### Compute units

Solana's measure of how much work a transaction does. Each transaction has a compute-unit limit.

## Ballista terms

### Template

A stored sequence of steps, with the inputs and accounts they use. It is uploaded once, locked, and
then anyone can run it. See [How it works](/guide/mental-model).

### Step

One entry in a template's step list: `let`, `require`, `invoke`, `assign`, a loop (`forEach` or
`repeat`), or an output (`emit` or `setReturnData`). Steps run in order. See
[Steps and control flow](/reference/language#steps-and-control-flow).

### Run input

A typed value the caller sends with a run, such as an amount or a deadline. A **row input** is sent
once per batch row.

### Batch

The part of a template that repeats: a row of accounts, and optionally row inputs, that the caller
supplies once per item. The template sets the maximum number of rows. See
[Batch execution](/guide/batching).

### Row

One set of batch accounts and row inputs. `forEach` runs its steps once per row, and
`account.iteration(name)` names the current row's account.

### Count loop

A loop that runs its steps a number of times read at run time, up to a maximum the template sets
(at most 255), without rows: `step.repeat`. A template can have up to 8 loops of either kind, one
after another. See [Loops](/reference/language#loops).

### Pass

One run through a loop's steps: once per row for `forEach`, once per count for `repeat`. Values
marked as carried keep their result from one pass to the next.

### Account group

A list of accounts, sized by the caller at run time, that one call passes along without the
template reading them. See [Account groups](/guide/account-groups).

### Runtime accounts

The accounts passed to a run after the template account: fixed accounts, then batch rows, then
account group members. At most 120.

### Register

A numbered slot that holds one value during a run. A `let` binding names one. A template uses at
most 64, and nothing in them survives the run.

### Verifier

The part of the Ballista program that checks a whole template once, at upload, before it can be
run. It rejects anything malformed or able to exceed a [limit](/reference/limits).

### Finalize

The last upload step: the verifier checks the template, then locks it. A finalized template can
never be changed or closed. See [Template lifecycle](/guide/template-lifecycle).
