# Conditional calls

A Solana transaction cannot skip one of its own instructions. If one call fails, the whole
transaction fails: the fee is still paid, nothing takes effect, and every other instruction in the
transaction is lost with it. Checking the state before sending does not solve this, because the
state can change between the check and the moment the transaction executes.

A Ballista template can attach a condition to a single call with `when`. The template evaluates
the condition while it runs. If the condition is false, the template skips that call and carries
on with the next step.

This page shows four examples: claim rewards only when some are pending, liquidate a position only
when it is unhealthy, top up a balance only when it is low, and create an account only if it does
not exist yet. In the code, `step.invoke` makes a [CPI](/reference/glossary#cpi), and
`systemTransfer` is a shortcut for a CPI to the System program. Both accept `when`. The examples
that call another protocol use marked stand-ins (the System program and its Transfer data) so they
compile and run as written; replace them with the protocol's own.

## Claim only when there is something

Call a protocol's claim instruction only when its rewards account shows a pending amount above
zero. `PENDING_OFFSET` is the byte offset of that amount in the rewards account. A keeper (a bot
that sends routine transactions for a protocol) can send this on a schedule without checking
first.

::: code-group

<<< @/../clients/js/examples/docs/claim-only-when-there-is-something.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/claim-only-when-there-is-something.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#claim-only-when-there-is-something [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#claim-only-when-there-is-something [Rust · Run]

:::

Most protocols treat a claim of nothing as an error, and an error reverts the whole transaction.
With `when`, an empty epoch is a no-op instead of a failure.

## Liquidate only when unhealthy

Liquidate a lending position only when its health value is below a threshold the caller passes.
The template reads the health value from the position account at `HEALTH_OFFSET`.

::: code-group

<<< @/../clients/js/examples/docs/liquidate-only-when-unhealthy.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/liquidate-only-when-unhealthy.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#liquidate-only-when-unhealthy [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#liquidate-only-when-unhealthy [Rust · Run]

:::

Keepers that watch lending positions often send the same liquidation for the same position. Only
the first can succeed. The others execute after the position has already changed. With a plain
liquidation instruction, each of those transactions fails. With `when`, they succeed and skip the
call, so any other work in them still takes effect.

## Top up only when low

Send `topUp` [lamports](/reference/glossary#lamports) from a funder to a bot's account, but only
when the bot's balance is below `floor`.

::: code-group

<<< @/../clients/js/examples/docs/top-up-only-when-low.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/top-up-only-when-low.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#top-up-only-when-low [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#top-up-only-when-low [Rust · Run]

:::

A scheduled job that always tops up drains the funder. One that checks first has read a balance
that may have changed by the time the transfer lands.

## Initialize only if missing

Create an account only if it does not exist yet.

::: code-group

<<< @/../clients/js/examples/docs/initialize-only-if-missing.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/initialize-only-if-missing.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#initialize-only-if-missing [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#initialize-only-if-missing [Rust · Run]

:::

`isEmpty` is true when the account holds no data, as it does before it is created. A few programs
offer an idempotent `Create`, one that succeeds even when the account already exists. For the
others, the caller must know whether the account exists, and still be right when the transaction
executes.

## Only calls take `when`

`when` belongs to `step.invoke` and the helpers that build one. Every other step runs whenever the
run reaches it:

- **A registry write** has no condition. To change a field only sometimes, write
  `expression.select(condition, newValue, currentValue)`, which writes the current value back
  otherwise, as the [allowlist](/guide/registries#an-allowlist) does.
- **An `emit`** has no condition. Every `emit` the run reaches is logged.
- **A return-data read** must come straight after a call without `when`. After a guarded call, the
  compiler refuses it, and so does the verifier.
- **A value** that depends on a condition is a `select`, not a skipped step.

The full rules are under [Registries](/reference/language#registries) and
[Output](/reference/language#output).
