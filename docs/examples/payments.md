# Payment patterns

Templates that pay out SOL: a payroll, a revenue split, weighted rewards, a refund with a deadline
and a capped sweep. Each shows the template and the code that runs it, in TypeScript and Rust.

::: warning Batching alone is not a reason
A plain transaction can already send many transfers. The payroll below is one of those cases: the
template adds a single instruction and a sequence of calls that is stored on chain and was checked
when it was uploaded, and nothing else. Templates earn their place when an amount or a decision
only exists while the transaction runs; see [amounts read at run time](/guide/runtime-values) and
[loops that decide per row](/guide/loops).
:::

## Bounded SOL payroll

Pay the same amount to up to 30 recipients with one Ballista instruction. The recipients form a
[batch](/reference/glossary#batch): a list of accounts supplied when the template runs, where each
entry is a [row](/reference/glossary#row). `step.forEach` runs the transfer once per row. The Rust
tabs build the same template with the lower-level `ProgramBuilder`, introduced in
[Author it in Rust](/guide/getting-started#author-it-in-rust).

::: code-group

<<< @/../clients/js/examples/docs/bounded-sol-payroll.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/bounded-sol-payroll.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#bounded-sol-payroll [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#bounded-sol-payroll [Rust · Run]

:::

## Basis-point revenue split

Split `total` [lamports](/reference/glossary#lamports) between a partner and a treasury. `partnerBps` is
the partner's share in basis points, or hundredths of a percent, so 10,000 is 100%. The template
rejects a share above 10,000, pays the partner `total × partnerBps / 10,000` rounded down, and pays
the treasury `total` minus that. Because the treasury's amount is a subtraction, rounding can't
create or lose lamports.

::: code-group

<<< @/../clients/js/examples/docs/basis-point-revenue-split.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/basis-point-revenue-split.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#basis-point-revenue-split [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#basis-point-revenue-split [Rust · Run]

:::

## Index-weighted rewards

Pay each recipient a multiple of `base` set by its place in the list: the first gets 1 × `base`,
the second 2 × `base`, and so on. `expression.loopIndex()` is the current row's position, starting
at 0.

::: code-group

<<< @/../clients/js/examples/docs/index-weighted-rewards.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/index-weighted-rewards.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#index-weighted-rewards [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#index-weighted-rewards [Rust · Run]

:::

## Deadline refund

Refund `refundAmount` lamports to the customer only if the run executes at or before `deadline`, a
Unix timestamp. After the deadline, the `when` condition skips the transfer and the run still
succeeds.

::: code-group

<<< @/../clients/js/examples/docs/deadline-refund.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/deadline-refund.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#deadline-refund [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#deadline-refund [Rust · Run]

:::

::: warning Authorization
Ballista has no authority over the escrow. `escrowAuthority` must sign the transaction itself. If
the refund comes from a call to an escrow program instead, that program must approve it by its own
rules.
:::

## Reserve-preserving sweep

Move up to `cap` lamports from `payer` to `vault` without letting `payer` fall below `reserve`. The
template records the balance and fails if it is already below `reserve`. It then sends the smaller
of `cap` and the amount above the reserve, and checks that the balance is still at least
`reserve`.

::: code-group

<<< @/../clients/js/examples/docs/reserve-preserving-sweep.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/reserve-preserving-sweep.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#reserve-preserving-sweep [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#reserve-preserving-sweep [Rust · Run]

:::
