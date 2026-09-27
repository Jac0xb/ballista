# Repay what the swap produced

<p class="protocol-line">Kamino · Jupiter</p>

**Status:** Compiles and passes the verifier; not yet run against Jupiter or Kamino.

## What it does

Sells collateral through Jupiter and repays a Kamino loan with what the sale produced, in one
transaction.

Deleveraging means selling collateral and repaying a loan with the proceeds. Two numbers are
unknown when you sign: how much the swap returns, and how much you owe by the time the transaction
runs. Interest keeps accruing, so a debt figure fetched by the client is already out of date when
the transaction lands.

The template handles the first number. It swaps, measures how much of the borrowed token arrived,
and requires at least `minimumRepayment`. It then refreshes the Kamino reserve, because a repayment
is priced against a freshly refreshed reserve, and repays the amount the swap produced. It does not
read the debt.

## Template

::: code-group

<<< ../../../clients/js/examples/protocols/kamino-repay-swap-output.ts [TypeScript · Template]

<<< ../../../clients/rust/examples/protocol_templates.rs#kamino-repay [Rust · Template]

<<< ../../../clients/js/examples/protocols/run/kamino-repay.ts [TypeScript · Run]

<<< ../../../clients/rust/examples/protocol_templates_run.rs#kamino-repay [Rust · Run]

:::

## Run it

Jupiter's `route` starts its account list with the token program, the signer, and the signer's
source and destination token accounts: here the collateral account and the borrowed-token account.
The template passes those four itself; the rest of the route's accounts arrive as the
`routeAccounts` [account group](/guide/account-groups).

The Run tabs pass the 11 declared accounts in order, then the inputs `routeArgs` and
`minimumRepayment`, then the group. `routeArgs` is Jupiter's instruction data without its
eight-byte discriminator; the template adds the `route` discriminator itself.

::: tip Requesting the route
Ask Jupiter's Swap API for `useSharedAccounts: false`. The template always sends Jupiter's `route`
instruction. The API's default, `shared_accounts_route`, is a different instruction whose accounts
are in a different order.
:::

## What has been tested

- The template compiles and passes Ballista's verifier.
- A test checks that it calls `route` with its accounts in `route`'s order, and that it repays
  exactly what the swap produced (`clients/js/src/protocol-semantics.test.ts`).
- The Rust template is byte-identical to the TypeScript one, and the Rust run passes the accounts
  and inputs the template declares (`clients/rust/tests/protocol_templates.rs`).
- No test calls Jupiter or Kamino.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
