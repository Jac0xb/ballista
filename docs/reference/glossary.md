# Glossary

Short definitions of the words these docs use. Each term has its own link, such as
`/reference/glossary#cpi`.

## Solana terms

### CPI

Cross-program invocation: one program calling another inside the same transaction. Every call a
template makes is a CPI from the Ballista program.

### PDA

Program-derived address: an address computed from a program ID and a list of seeds, the byte
strings it is derived from. It has no private key, so only that program can sign for it. Deriving
one takes a bump, one extra seed byte that makes the result a valid program address; the canonical
bump is the highest that does. Templates only compare addresses with derived ones (`assertPda`);
see [When Ballista signs](/guide/trust-model#signing).

### ATA

Associated token account: the standard token account for a given wallet and mint, at a PDA of the
Associated Token Account program. `assertAta` checks that an account is the right one.

### Lamports

The smallest unit of SOL. One SOL is 1,000,000,000 lamports.

### Rent

The lamports an account must hold for its size to stay on chain, the rent-exempt minimum. Closing
the account returns them. A [registry entry](#entry) is never closed, so its rent stays locked.

### Compute units

Solana's measure of how much work a transaction does. Each transaction has a compute-unit limit,
and fails if it uses more. [Compute](/reference/limits#compute) says how to set it.

### Discriminator

The leading bytes of an instruction's data that tell a program which instruction to run. Anchor
programs use 8 bytes derived from the instruction's name.

### Instructions sysvar

A read-only account, `Sysvar1nstructions1111111111111111111111111`, in which Solana lists the
transaction's instructions. A template reads the other instructions through it; see
[Introspection](/reference/language#introspection).

## Ballista terms

### Template

A stored sequence of steps, with the inputs and accounts they use. It is uploaded once, locked, and
then anyone can run it. See [How it works](/guide/mental-model).

### Step

One item in a template's step list: `let`, `require`, `invoke`, `assign`, `setRegistry`, a loop
(`forEach` or `repeat`), or an output (`emit` or `setReturnData`). Steps run in order. See
[Steps](/reference/language#steps).

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

A list of accounts, sized by the caller at run time, that one call passes along. The template
can count them and test them against a filter, but not read them freely. See
[Account groups](/guide/accounts-and-cpis#account-groups).

### Runtime accounts

The accounts passed to a run after the template account: fixed accounts, then batch rows, then
account group members. At most 120, but about 61 fit in one transaction; see
[accounts per transaction](/reference/limits#accounts-per-transaction).

### Register

A numbered slot that holds one value during a run. A `let` binding names one. A template uses at
most 64 ([register budget](/reference/limits#registers)), and nothing in them survives the run.
State that must outlive a run goes in a [registry](#registry).

### Registry

State a template keeps between runs: named fields declared in `registries`, 1 to 512 bytes in all.
The values live in entries, one for each key. A template declares up to 8 registries. See
[Registries](/reference/language#registries).

### Entry

A registry entry: an account Ballista owns that holds one registry's fields for one key of one
template, after a 72-byte header. The first run that opens it creates it, and only that template's
runs can write it. Anyone can read it, and it is never closed. See
[Registry entries](/reference/wire-format#registry-entries).

### Payer

The account that pays a registry entry's [rent](#rent) when a run creates the entry. The template
names it in `account.registry`, and it must sign and be writable.

### Verifier

The part of the Ballista program that checks a whole template once, at upload, before it can be
run. It rejects anything malformed or able to exceed a [limit](/reference/limits).

### Finalize

The last upload step: the verifier checks the template, then locks it. A finalized template can
never be changed or closed. See [Template lifecycle](/guide/template-lifecycle).
