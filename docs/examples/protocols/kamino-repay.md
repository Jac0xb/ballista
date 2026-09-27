# Repay what the swap produced

This template sells collateral through Jupiter and repays a Kamino loan with what the sale
produced, in one transaction.

Deleveraging means selling collateral and repaying a loan with the proceeds. Two numbers are
unknown when you sign: how much the swap returns, and how much you owe by the time the transaction
runs. Interest keeps accruing, so a debt figure fetched by the client is already out of date when
the transaction lands.

The template handles the first number. It swaps, measures how much of the borrowed token arrived,
and requires at least `minimumRepayment`. It then refreshes the Kamino reserve, because a repayment
is priced against a freshly refreshed reserve, and repays the amount the swap produced.

It does not read the debt: it repays exactly what the swap produced.

Jupiter's `route` starts its account list with the token program, the signer, and the signer's
source and destination token accounts: here the collateral account and the borrowed-token account.
The template passes those four itself; the rest of the route's accounts arrive as an
[account group](/guide/account-groups). `routeArgs` is Jupiter's instruction data without its
eight-byte discriminator; the template adds the `route` discriminator itself.

::: tip Requesting the route
Ask Jupiter's Swap API for `useSharedAccounts: false`. The template always sends Jupiter's `route`
instruction. The API's default, `shared_accounts_route`, is a different instruction whose accounts
are in a different order.
:::

::: code-group

<<< ../../../clients/js/examples/protocols/kamino-repay-swap-output.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#group [Run · Rust]

:::

The Rust tab builds the run for [deposit exactly what a swap produced](/examples/protocols/jupiter-deposit).
This template's run has the same form, with the route's remaining accounts passed as an account
group, but uses its own accounts and inputs in the order the template declares them.

Not yet run against Jupiter or Kamino: the template compiles and passes Ballista's verifier, and a
test checks that it calls `route` with its accounts in `route`'s order, but no test calls either
program.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
