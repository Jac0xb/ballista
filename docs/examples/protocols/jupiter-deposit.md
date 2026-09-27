# Deposit exactly what a swap produced

This template swaps through Jupiter and deposits exactly what the swap produced into Kamino, in one
transaction.

Jupiter's `route` instruction carries the amount in, the *quoted* amount out, `slippageBps` and
`platformFeeBps`. Jupiter reports the amount that actually came out only as an event, which it
emits by making a CPI (a call from one program to another) to itself. An event isn't return data,
so a calling program can't read it with `get_return_data`. The only reliable source is the
destination token account, after the swap has run.

Kamino's `deposit_reserve_liquidity_and_obligation_collateral_v2` needs that number as its
`liquidity_amount`. A plain transaction has to fix it before the swap runs. Guess too high and the
deposit fails; guess too low and the rest stays in your token account.

The template reads the destination balance before and after the swap, requires the difference to
be at least `minimumOut`, and deposits exactly that difference.

::: tip Requesting the route
Ask Jupiter's Swap API for `useSharedAccounts: false`. The template always sends Jupiter's `route`
instruction. The API's default, `shared_accounts_route`, is a different instruction whose accounts
are in a different order.
:::

::: code-group

<<< ../../../clients/js/examples/protocols/jupiter-deposit-exact-output.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#group [Run · Rust]

:::

`route` starts its account list with the token program, the signing owner, and the owner's source
and destination token accounts. The template passes those four itself, so the destination it
measures is the account Jupiter credits. The rest of Jupiter's list changes from route to route, so
it arrives as an [account group](/guide/account-groups): a list of any length that the caller
supplies at run time and the template passes on to Jupiter. Group members keep the writable flag
(permission to be modified) that the transaction gave them, and are never passed as signers.

Your client passes Jupiter's instruction data without its first eight bytes, the discriminator
that names the instruction; the template adds the `route` discriminator itself. The TypeScript run
script, `clients/js/examples/protocols/run-jupiter-deposit.ts`, refuses data that isn't `route`,
checks that the Swap API's account list starts with the four accounts above, and forwards the rest
as the group. The Rust tab builds the same run.

Not yet run against Jupiter or Kamino: the template compiles and passes Ballista's verifier, and a
test checks that it calls `route` with its accounts in `route`'s order, but no test calls either
program.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
