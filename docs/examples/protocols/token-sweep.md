# Sell a whole balance

This template sells everything in a token account through Jupiter, whatever the balance turns out
to be when the transaction runs. Typical sources are a fee account, an airdrop claim, a vesting
withdrawal or the leftovers from an earlier swap.

A Jupiter route normally sells a fixed amount, set when the route is built. If the balance is lower
when the transaction runs, the route fails. If it is higher, the difference stays behind.

Jupiter's `route` carries the amount to sell as `in_amount`, after the route plan and before the
quote. So your client passes the route plan and the quote's numbers separately, and the template
writes the instruction data itself. It:

- reads the balance and fails the run unless it is above `dustFloor`;
- passes that balance to Jupiter as `in_amount`, with the quoted output scaled to match
  (`quotedOutAmount × balance / quotedInAmount`);
- requires the proceeds to be at least that scaled quote, less `slippageBps`;
- requires the account to hold no more than `dustFloor` afterwards.

The route plan splits its input by percentage, so the same plan can sell more or less than it was
quoted for. How far the balance can drift from the quote is limited by the pools the route's
accounts cover.

`route` starts its account list with the token program, the signer, and the signer's source and
destination token accounts. The template passes those four itself; the rest of the route's
accounts arrive as an [account group](/guide/account-groups).

::: tip Requesting the route
Ask Jupiter's Swap API for `useSharedAccounts: false`. The template always sends Jupiter's `route`
instruction. The API's default, `shared_accounts_route`, is a different instruction whose accounts
are in a different order.
:::

::: code-group

<<< ../../../clients/js/examples/protocols/token-sweep-into-swap.ts [Template · TypeScript]

<<< ../../../clients/rust/examples/protocol_runs.rs#group [Run · Rust]

:::

The balance is the SPL Token account's `amount`, a `u64` at byte offset 64. Both token accounts
must belong to the original SPL Token program, so a Token-2022 account fails the template's owner
check.

The Rust tab builds the run for [deposit exactly what a swap produced](/examples/protocols/jupiter-deposit).
This template's run has the same form, with the route's remaining accounts passed as an account
group, but uses its own accounts and inputs in the order the template declares them.

Not yet run against Jupiter: the template compiles and passes Ballista's verifier, and a test
checks that it calls `route` with its accounts in `route`'s order and sells the balance it read.
No test calls Jupiter.

[All protocol templates](/examples/protocols/) · [What has been tested](/examples/protocols/#what-has-been-tested)
