# Amounts read at run time

A Solana transaction fixes its instruction data when it is signed, so every amount in it must be
known in advance. Many useful amounts are not: the balance to sweep, the debt to repay, the part of
a deposit to pass on. They exist only when the transaction executes.

A Ballista template can read a number from an account while it runs and pass it to the next call.
This page shows four ways to use that: sweep a balance, forward a token balance, repay a debt, and
split a deposit.

In the examples, `step.let` computes a value once and gives it a name, and `step.require` stops
the whole transaction unless its condition holds. `systemTransfer` and `tokenTransfer` call the
System program and the Token program. Each example has a **Template** tab, the whole template, and
a **Run** tab, the code that builds the instruction to run it, in TypeScript or Rust. Where an
example calls another protocol, the code uses marked stand-ins (the System program and its
Transfer data) so that it compiles and runs as written; replace them with the protocol's own.

## Sweep above a reserve

Move everything above a minimum balance (the reserve) from a vault to a destination, whatever the
balance is when the transaction executes.

::: code-group

<<< @/../clients/js/examples/docs/sweep-above-a-reserve.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/sweep-above-a-reserve.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#sweep-above-a-reserve [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#sweep-above-a-reserve [Rust · Run]

:::

The template reads the vault's balance in [lamports](/reference/glossary#lamports) and transfers
whatever is above the reserve. The caller passes only the reserve. If the balance is not above the
reserve, the `require` stops the run before the subtraction.

## Forward the whole token balance

Move a token account's entire balance to another token account.

::: code-group

<<< @/../clients/js/examples/docs/forward-the-whole-token-balance.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/forward-the-whole-token-balance.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#forward-the-whole-token-balance [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#forward-the-whole-token-balance [Rust · Run]

:::

`accountData` reads a value of the given type at a byte offset in an account's data. An SPL Token
account stores its balance as a `u64` at offset 64, so the template reads it there. That read is
only meaningful if the account really is a token account, so the template requires both accounts
to be owned by the Token program and to hold at least 165 bytes of data. The `require` stops the
run when the balance is zero.

::: tip Finding a field's offset
An offset is the number of bytes before the field in the account's data. Add up the sizes of the
fields that come before it in the program's account struct: an SPL Token account starts with a
32-byte mint and a 32-byte owner, so its `u64` amount starts at byte 64. Anchor programs put an
8-byte discriminator first, so their first field is at byte 8. An Anchor IDL lists each account's
fields in order with their types, which is enough to add up the sizes, as long as every field
before the one you want has a fixed size.
:::

## Repay exactly what is owed

Repay a loan in full, or with everything the borrower holds if that is less. The debt grows with
every slot (the interval in which Solana produces a block), so an amount the client computes
before signing is already out of date when the transaction executes.

::: code-group

<<< @/../clients/js/examples/docs/repay-exactly-what-is-owed.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/repay-exactly-what-is-owed.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#repay-exactly-what-is-owed [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#repay-exactly-what-is-owed [Rust · Run]

:::

The template reads the debt from the loan account (a `u64` at `DEBT_OFFSET` in this example's
layout) and the borrower's balance, then pays the smaller of the two. `step.invoke` makes a
[CPI](/reference/glossary#cpi) into the lending program. `data.literal` writes fixed bytes, here
the discriminator that selects the repay instruction, and `data.encode` appends the amount as a
`u64`.

A fixed amount would be wrong in one direction or the other: most lending programs reject an
overpayment, and an underpayment leaves the loan open. `min` never pays more than is owed, and it
pays the whole debt whenever the borrower can cover it.

## Split what arrived

Send a percentage of a vault's balance above a reserve to a partner, and the rest to a treasury.
The balance keeps changing as deposits arrive, so the split is computed when the transaction
executes.

::: code-group

<<< @/../clients/js/examples/docs/split-what-arrived.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/split-what-arrived.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#split-what-arrived [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#split-what-arrived [Rust · Run]

:::

`shareBps` is the partner's share in basis points (hundredths of a percent, so 10,000 is 100%).
Integer division rounds the partner's share down. The treasury receives the remainder rather than
a second percentage, so rounding never leaves a lamport of the distributable amount behind.
