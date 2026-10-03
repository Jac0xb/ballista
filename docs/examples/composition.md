# Protocol composition

Templates that call other programs in sequence, with checks between the calls. Ballista works with
any program. Your client still finds routes, quotes and accounts as it would for a plain
transaction; the template fixes the order of the calls and the conditions that must hold while they
run.

Each call to another program is a [CPI](/reference/glossary#cpi), made with `step.invoke` and
instruction data your client supplies. Each recipe shows the template and the code that runs it, in
TypeScript and Rust. So that the code compiles and runs as written, the protocol calls use marked
stand-ins (the System program and its Transfer data); replace them with the protocols' own.

## Swap then deposit

Swap, check how many tokens the swap actually delivered, then deposit. Your client builds the
instruction data for both calls. Between them, the template requires `receivedTokens` to have grown
by at least `minimumOut`; if it hasn't, the deposit never happens and the whole run reverts. The
deposit amount is whatever your client put in the deposit data. To deposit exactly what the swap
produced, see [deposit exactly what a swap produced](/examples/protocols/jupiter-deposit).

::: code-group

<<< @/../clients/js/examples/docs/swap-then-deposit.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/swap-then-deposit.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#swap-then-deposit [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#swap-then-deposit [Rust · Run]

:::

## Claim then distribute

Claim rewards into a treasury token account, then pay the same amount to each recipient in a list.
The claim runs once, before the loop; the transfer runs once per row.

::: code-group

<<< @/../clients/js/examples/docs/claim-then-distribute.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/claim-then-distribute.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#claim-then-distribute [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#claim-then-distribute [Rust · Run]

:::

## Primary or fallback route

Include two routes, each behind a `when` condition, where one condition is the opposite of the
other. Each run calls exactly one route, chosen by the `usePrimary` input.

::: code-group

<<< @/../clients/js/examples/docs/primary-or-fallback-route.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/primary-or-fallback-route.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#primary-or-fallback-route [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#primary-or-fallback-route [Rust · Run]

:::

## Time-gated governance execution

Execute a governance action only if the proposal account says it is approved and its execution time
has passed. The template reads both fields from the proposal account during the run, then forwards
the execute instruction your client built. `APPROVED_OFFSET` and `TIME_OFFSET` are the byte
positions of those fields in your governance program's proposal account.

::: code-group

<<< @/../clients/js/examples/docs/time-gated-governance-execution.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/time-gated-governance-execution.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#time-gated-governance-execution [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#time-gated-governance-execution [Rust · Run]

:::

## Bounded keeper crank

Call the same maintenance instruction, often called a crank, once for each market and queue pair in
a list; each pair is one row of a batch. A keeper, the bot or service that sends maintenance
transactions, signs each call and is passed first, before the row's market and queue. Ballista holds
no authority over the accounts.

::: code-group

<<< @/../clients/js/examples/docs/bounded-keeper-crank.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/bounded-keeper-crank.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#bounded-keeper-crank [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#bounded-keeper-crank [Rust · Run]

:::

Ballista doesn't run on a schedule. A bot, a user or a keeper service still decides when to send
each run.

## Run another template

A template can call Ballista's `Run` to run another finalized template, then read what that run
returns. Here the inner template swaps, requires a minimum, and returns what arrived with
`step.setReturnData`. The outer template runs it, reads the amount with `expression.returnData`,
and deposits exactly that.

::: code-group

<<< @/../clients/js/examples/docs/swap-and-return-what-arrived.ts#template [TypeScript · Inner]

<<< @/../clients/js/examples/docs/nested-swap-then-deposit.ts#template [TypeScript · Outer]

<<< @/../clients/js/examples/docs/nested-swap-then-deposit.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#swap-and-return-what-arrived [Rust · Inner]

<<< @/../clients/rust/examples/docs_templates.rs#nested-swap-then-deposit [Rust · Outer]

<<< @/../clients/rust/examples/docs_runs.rs#nested-swap-then-deposit [Rust · Run]

:::

- **Pin the inner template by address.** Return data shows only that a Ballista run set it, not
  which template ran. A finalized template can't be changed or closed, so its address fixes what
  it does. Upload the inner template first, and put its address in place of the stand-in.
- **Pass the inner run's accounts as its own run would:** the inner template account, then its
  accounts in the order it declares them. An account the inner run needs as a signer must be
  declared and passed as one by the outer template.
- **Read the return data straight after the call,** which has no `when`, as the
  [output rules](/reference/language#output) require.
- **It costs a call frame.** The inner run is frame 2, so its own calls start at frame 3 of
  Solana's [5](/reference/limits#call-depth), and the inner run and its calls all count toward
  the transaction's [instruction trace](/reference/limits#instruction-trace). In return, the inner
  run has its own registers, VM instructions and 64 CPIs.
