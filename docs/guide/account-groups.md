# Account groups

This page explains account groups: lists of accounts whose length and contents the caller chooses
at run time, and which a template passes along to a CPI (cross-program invocation: a call to another
program).

A CPI's account list is normally fixed when the template is written, one declared account in each
position. That lets finalization (the one-time check that locks a template on chain) check every
account the template touches. It also means a template that swaps through an aggregator would be
tied to the accounts of one route. An account group removes that limit. The template declares the
group by name, the caller supplies its members at run time, and a CPI passes them after its own
declared accounts, the way Solana programs pass "remaining accounts".

## What a group is

- A template declares up to eight groups by name: `accountGroups: ['routeA', 'routeB']`.
- At run time the caller passes each group's members after the fixed accounts and batch rows, and
  the run data starts with one byte per group giving its size. A group may be empty.
- An `invoke` names the group it forwards with `accountGroup`. The CPI receives the invoke's
  declared accounts first, then every member of the group, in the order the caller supplied them.
- One CPI can pass at most 64 accounts, counting its declared accounts and the group's members. A
  run that goes over fails with `CpiAccountLimitExceeded` (error 6021), and the error reports the
  total.

## What a group is not

The template cannot see inside a group. Members have no requirements, cannot be read, and no step
can refer to one. Anything the template must check about the result, such as a change in balance,
a destination's owner, or a mint, has to be read from declared accounts. That is why a swap's user
token accounts belong among the declared accounts, not in the group.

Each member is passed as writable if the transaction marked it writable, but never as a signer. So
a template can pass on a signature only through an account its author declared. The extra accounts
a swap route needs are pools and vaults, which never sign anyway.

A group belongs to a CPI, not to a batch row: an `invoke` inside `forEach` forwards the same group
on every row.

## Choosing between swaps at run time

The template below records three token balances, decides which of three swaps are needed, and runs
only those. Each swap has its own route data and its own group, so one finalized template works for
any route over any tokens. When a balance already meets its target, the template skips that swap,
and the caller passes an empty group and empty route data for it.

::: code-group

<<< @/../clients/js/examples/docs/rebalance-three-swaps.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/rebalance-three-swaps.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#rebalance-three-swaps [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#rebalance-three-swaps [Rust · Run]

:::

For each swap, the template:

1. records the destination account's token balance;
2. decides whether the swap is needed, which is when that balance is below its target;
3. requires that the destination account belongs to the user;
4. runs the swap only if it is needed and, if it ran, requires that the balance rose by at least
   the minimum.

The TypeScript writes these steps once, in `swapLeg`, and repeats them for A, B and C; the Rust
loops over the three legs.

To skip a swap, the caller passes an empty group and empty route data for it. In the run data, the
three group lengths come first, one byte each, then the inputs in declaration order. In the
accounts, each group's members follow the declared accounts, group A's first.

The routes come from off-chain quotes; a template cannot find a route on chain. What it can do is
fail the transaction unless the result passes the checks its author wrote.

## Budget

A transaction uses at most 64 accounts, lookup tables included. The Ballista program and the
template account take two, and `user` can pay the fee, so the run has 62. Nine are fixed, which
leaves the three groups 53 between them, about 17 each: enough for short routes, so cap each
quote's accounts (Jupiter's `maxAccounts`). A [version 1 transaction](/guide/transaction-v1) lists
all 64 addresses in its 4,096 bytes; a version 0 transaction needs an address lookup table to fit
them in 1,232. The run data has its own limit of 1,024 bytes, which is why this template caps each
route at 256 bytes. See [accounts per transaction](/reference/limits#accounts-per-transaction).
