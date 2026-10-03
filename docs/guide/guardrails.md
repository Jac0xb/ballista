# Safety guardrails

A guardrail is a check that runs in the same transaction as the operation it protects. This page
shows five: a swap deadline and minimum output, a fixed program address and account owner, an
oracle price band, a cap on how much SOL the payer can spend, and a check that an account is the
one a program would derive.

If a guardrail fails, the whole transaction fails, and every [CPI](/reference/glossary#cpi) made
before it is rolled back. Most of the examples use `step.require`, which stops the transaction
unless its condition holds. Every example here guards a call to another protocol. So that the code
compiles and runs as written, it calls marked stand-ins (the System program and its Transfer data);
replace them with the protocol's own.

## Deadline and minimum output

Before calling a swap program with route data the client built, check that the quote has not
expired and that it promises at least the minimum output.

::: code-group

<<< @/../clients/js/examples/docs/deadline-and-minimum-output.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/deadline-and-minimum-output.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#deadline-and-minimum-output [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#deadline-and-minimum-output [Rust · Run]

:::

`clockUnixTimestamp` is the network's clock when the transaction executes. The quoted and minimum
outputs are both inputs from the caller, so this check only enforces what the client claims. To
protect the output without trusting the caller, record the destination token account's balance
with `step.snapshot` before the swap, and check how much it grew after the swap, as
[swap then deposit](/examples/composition#swap-then-deposit) does.

Also cap the route's platform fee: whoever builds the run picks its account and rate, so require
`platformFeeBps` to be at most a constant.

## Pinned program and owner

Fix the address of each program the template calls, and the owner of each protocol account, in
the template's account schema: its list of accounts and the rules each must meet.

::: code-group

<<< @/../clients/js/examples/docs/pinned-program-and-owner.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/pinned-program-and-owner.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#pinned-program-and-owner [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#pinned-program-and-owner [Rust · Run]

:::

`executable: true` with an `address` means the account must be that exact program. `owner` means
the account must belong to the given program, `minDataLength` means it must hold at least that
many bytes of data, and `writable` means the transaction may change it. If any account breaks its
rules, the run fails before the first step, so a caller cannot swap in a different program, or an
account another program owns.

The owner pin doesn't fix which account `position` is, or even its type: any account the protocol
owns with at least 128 bytes passes, another user's position included. Before relying on its data,
also check its type, by its discriminator or exact length, and its identity, as
[Canonical position account](#canonical-position-account) does with a derivation. See
[Pins](/guide/trust-model#pins).

## Oracle price band

Read a price from an oracle account (an account in which a price feed publishes prices) and stop
the run unless the price lies between a minimum and a maximum. `PRICE_OFFSET` is the byte offset
of the price in the oracle's account layout. The oracle account's `owner` is fixed in the account
schema, as in the previous example, so the price comes from the oracle program. That doesn't say
which feed: one program owns every feed's accounts. The band is two inputs, so it guards the caller
against a price move, not against whoever builds the transaction.

::: code-group

<<< @/../clients/js/examples/docs/oracle-price-band.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/oracle-price-band.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#oracle-price-band [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#oracle-price-band [Rust · Run]

:::

::: warning Layout, feed and freshness
Ballista does not understand oracle formats. The template must read the price at the offset the
oracle protocol documents, check that the account holds the feed it expects, by its address or a
feed ID in its data, and check the publish slot or timestamp so that a stale price is rejected.
[Act only on a fresh price](/examples/protocols/pyth-gate) does all three for Pyth.
:::

## Maximum lamport spend

Record a signer's balance in [lamports](/reference/glossary#lamports) before calling another
program, and fail the whole transaction if the balance dropped by more than a limit. A signer is an
account that signed the transaction; here it is the payer.

::: code-group

<<< @/../clients/js/examples/docs/maximum-lamport-spend.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/maximum-lamport-spend.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#maximum-lamport-spend [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#maximum-lamport-spend [Rust · Run]

:::

`step.snapshot` saves the payer's balance before the call. After the call, the template reads the
balance again and requires that it dropped by at most `maximumSpend`. Because the check runs after
the call, it limits what the call actually spent, whatever instruction the client chose.
Ballista's subtraction fails instead of going below zero, so the run also fails if the call leaves
the payer with more lamports than before.

## Canonical position account

Check that the position account the caller passed is the one the protocol derives for this owner
and position ID. An owner check alone would accept any account the protocol owns that has a valid
layout, including another position.

::: code-group

<<< @/../clients/js/examples/docs/canonical-position-account.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/canonical-position-account.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#canonical-position-account [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#canonical-position-account [Rust · Run]

:::

`assertPda` computes a [PDA](/reference/glossary#pda) from the protocol program and the seeds,
searching for the canonical bump, and the run fails if `position` has a different address. The
caller derives the same address to build the account list, as both Run tabs show.
[PDA and ATA assertions](/guide/pda-assertions) covers the bump and what it costs.
