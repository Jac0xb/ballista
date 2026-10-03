# Conditional calls

If one instruction fails, the whole transaction fails, and checking the state before sending
doesn't help: it can change before the transaction runs. A template can give one call a `when`
condition, checked while it runs. If it is false, the call is skipped and the run carries on.

The four examples below claim, liquidate, top up and initialize only when they should. `when` works
on `step.invoke`, which makes a [CPI](/reference/glossary#cpi), and on helpers such as
`systemTransfer`. Calls to other protocols use marked stand-ins; replace them with the protocol's
own.

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

## Top up to a target

Bring a bot's balance up to `target` [lamports](/reference/glossary#lamports) from a funder. The
template reads the bot's balance while it runs and sends the difference. When the bot already has
`target` or more, the transfer is skipped.

::: code-group

<<< @/../clients/js/examples/docs/top-up-to-a-target.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/top-up-to-a-target.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#top-up-to-a-target [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#top-up-to-a-target [Rust · Run]

:::

A plain transfer fixes its amount when the transaction is built, from a balance read off chain. By
the time it lands the balance may have changed, so the bot ends up above or below the target.

The amount uses `min` because a call's data is worked out even when `when` skips the call.
`target - botBalance` alone would go below zero and fail the run.

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
