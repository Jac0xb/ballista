# Token-account patterns

Templates that create, pay into, close and check SPL token accounts. An
[ATA](/reference/glossary#ata) (associated token account) is the standard token account for a given
wallet and mint. Each shows the template and the code that runs it, in TypeScript and Rust.

Most of these can also be done with plain instructions in one transaction. Templates earn their
place when they read a balance or a flag while the transaction runs, as in
[forward the whole token balance](/guide/runtime-values#forward-the-whole-token-balance) and
[consolidate only the funded accounts](/guide/loops#consolidate-only-the-funded-accounts).

Some recipes read a token account's balance straight from its data. In the SPL Token account
layout, the balance is a `u64` at byte offset 64. When a template reads raw bytes like this, have it
also pin the Token Program's address and require the account's owner and a minimum data length of
165 bytes. Those pins still admit a 355-byte Token multisig. A transfer from or to one fails, but
where no transfer would, require the length to be exactly 165. See
[what a pin proves](/guide/trust-model#pins).

## Assert, create, then transfer

For each recipient, check that the destination is the recipient's ATA, create it if it doesn't
exist yet, then transfer `amount` tokens to it. An ATA's address is a
[PDA](/reference/glossary#pda) of the Associated Token Account program, and its seeds are the
owner, the token program and the mint. `assertAta` derives that address and fails the run if the account
passed in doesn't match. If any step fails, the whole run reverts.

Each recipient is one row of a batch: two accounts, the recipient's wallet and its ATA.
`step.forEach` runs the three steps once per row.

::: code-group

<<< @/../clients/js/examples/docs/assert-create-then-transfer.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/assert-create-then-transfer.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#assert-create-then-transfer [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#assert-create-then-transfer [Rust · Run]

:::

## Existing-account token payroll

Send the same token amount to up to 32 token accounts that already exist. Each destination must be
owned by the Token Program and be at least 165 bytes long, the size of a token account. A Token
multisig passes both checks, but the transfer to it fails.

::: code-group

<<< @/../clients/js/examples/docs/existing-account-token-payroll.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/existing-account-token-payroll.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#existing-account-token-payroll [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#existing-account-token-payroll [Rust · Run]

:::

## Conditional ATA setup

Create an ATA only if it doesn't exist yet. `ensureAssociatedTokenAccount` calls the ATA program's
`Create` instruction with a `when` condition that the account is empty, so a repeat run skips the
call instead of failing. It uses `Create` rather than `CreateIdempotent` so that the condition is
what does the work. `CreateIdempotent` sent as a plain instruction does the same job without a
template.

::: code-group

<<< @/../clients/js/examples/docs/conditional-ata-setup.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/conditional-ata-setup.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#conditional-ata-setup [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#conditional-ata-setup [Rust · Run]

:::

## Close empty token accounts

For each token account in the list, read its balance (the `u64` at byte offset 64) and call SPL
Token's `CloseAccount` only if it is zero. Accounts that still hold tokens are skipped, so one
funded account doesn't fail the whole run. Closing an account returns its rent (the SOL deposit
that keeps an account open) to `rentDestination`.

::: code-group

<<< @/../clients/js/examples/docs/close-empty-token-accounts.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/close-empty-token-accounts.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#close-empty-token-accounts [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#close-empty-token-accounts [Rust · Run]

:::

## Exact token debit

Transfer tokens, then check that the source balance dropped by exactly `amount`. The template
records the balance before the transfer and compares it afterwards; any other change fails the
whole run.

::: code-group

<<< @/../clients/js/examples/docs/exact-token-debit.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/exact-token-debit.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#exact-token-debit [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#exact-token-debit [Rust · Run]

:::
