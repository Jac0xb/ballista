# Accounts and CPIs

A template declares each account it uses, with what it must be: a signer, writable, a program, or
pinned by `address`, `owner` and `minDataLength`. A run rejects any account that doesn't match, a
CPI can't pass an account with more privilege than its declaration gives, and Ballista never signs
a template's calls; see [Privileges](/guide/trust-model#privileges).

## Protocol helper

This template moves tokens between two token accounts. Its `accounts` section shows each kind of
requirement: a pinned program, a signer, and two writable token accounts pinned by owner and size.

::: code-group

<<< @/../clients/js/examples/docs/token-transfer.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/token-transfer.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#token-transfer [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#token-transfer [Rust · Run]

:::

`tokenTransfer` is a shortcut, not a special instruction in the Ballista program. It compiles to an
ordinary CPI: the Token Program's accounts, the byte that selects its Transfer instruction, and the
amount encoded as a `u64`. Rust's `token_transfer` compiles to the same bytes.

## Generic CPI

`step.invoke` builds any CPI from parts: the program to call, the accounts with the privileges to
pass, and the instruction data as a list of pieces, either literal bytes or encoded values. The
optional `when` condition is covered [below](#conditional-invocation).

::: code-group

<<< @/../clients/js/examples/docs/generic-cpi.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/generic-cpi.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#generic-cpi [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#generic-cpi [Rust · Run]

:::

`MY_PROGRAM` and `MY_DISCRIMINATOR` are placeholders for your program's address and instruction.
A CPI's data can be at most 4,096 bytes. Finalization works out the largest size each CPI's data
can reach, here 8 + 8 + 128 bytes, and rejects a template that could exceed the limit.

## Account groups

A CPI's account list is normally fixed when the template is written, one declared account in each
position, which lets finalization check every account the template touches. Some programs need
accounts the author can't know in advance, such as the pools along a swap route. An account group
removes that limit: the template declares the group by name, the caller supplies its members at run
time, and a CPI passes them after its own declared accounts, the way Solana programs pass "remaining
accounts".

### What a group is

- A template declares up to eight groups by name: `accountGroups: ['routeA', 'routeB']`.
- At run time the caller passes each group's members after the fixed accounts and batch rows, and
  the run data starts with one byte per group giving its size. A group may be empty.
- An `invoke` names the group it forwards with `accountGroup`. The CPI receives the invoke's
  declared accounts first, then every member of the group, in the order the caller supplied them.
- One CPI can pass at most 64 accounts, counting its declared accounts and the group's members. A
  run that goes over fails with `CpiAccountLimitExceeded` (error 6021), and the error reports the
  total.

### What a group is not

Members have no requirements, and a template can't read a member's data freely. It can count the
members and test each one against a filter it pins: the owner program, a data length, and bytes at
fixed offsets (see [Checking what a group holds](#checking-what-a-group-holds)). Anything else the
template must check about the result, such as a change in balance, has to be read from declared
accounts. That is why a swap's user token accounts belong among the declared accounts, not in the
group.

Each member is passed as writable if the transaction marked it writable, but never as a signer. So
a template can pass on a signature only through an account its author declared. The extra accounts
a swap route needs are pools and vaults, which never sign anyway.

A group belongs to a CPI, not to a batch row: an `invoke` inside `forEach` forwards the same group
on every row.

### Checking what a group holds

A swap call carries the user's signature, and the group carries whatever accounts the route names.
A route that slips in another of the user's token accounts could spend from it. The template below
refuses such a route before it swaps.

::: code-group

<<< @/../clients/js/examples/docs/swap-through-a-checked-route.ts#template [TypeScript · Template]

<<< @/../clients/js/examples/docs/swap-through-a-checked-route.ts#run [TypeScript · Run]

<<< @/../clients/rust/examples/docs_templates.rs#swap-through-a-checked-route [Rust · Template]

<<< @/../clients/rust/examples/docs_runs.rs#swap-through-a-checked-route [Rust · Run]

:::

- `groupAny(group, filter)` is true when any member matches. `groupCount` returns how many match,
  and `groupLength` how many members the caller passed.
- A member matches when its owner is one of `programs`, its data holds each `match` value at its
  offset, and its address is none of `exceptKeys`.
- Here the filter finds a Token or Token-2022 account whose owner, at byte 32, is the user.
  `source` and `destination` are excepted, so the swap can still use them.
- A route that breaks the check fails with `RequirementFailed` (6015) at `noOtherUserTokenAccount`,
  and nothing moves.
- To require that something is there instead, require `groupAny`: for example, a token account
  of the user's that the route pays into.

The programs are pinned in the template, so a reader sees which accounts it checks.
[Account groups](/reference/language#account-groups) has the rules.

### Choosing between swaps at run time

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

### Budget

A transaction uses at most 64 accounts, lookup tables included. The Ballista program and the
template account take two, and `user` can pay the fee, so the run has 62. Nine are fixed, which
leaves the three groups 53 between them, about 17 each: enough for short routes, so cap each
quote's accounts (Jupiter's `maxAccounts`). A version 1 transaction lists
all 64 addresses in its 4,096 bytes; a version 0 transaction needs an address lookup table to fit
them in 1,232. The run data has its own limit of 1,024 bytes, which is why this template caps each
route at 256 bytes. See [accounts per transaction](/reference/limits#accounts-per-transaction).

## Conditional invocation

`when` makes a single CPI optional. Its condition is evaluated when the run reaches that step. If
the condition is false, the call is skipped and the run continues with the next step. A template
that emits a [run event](/guide/errors-and-events#run-events) records which calls actually ran.

Compare `step.require`, which fails the whole transaction when its condition is false. Use `when`
for work that is sometimes unnecessary, such as creating an account that may already exist. Use
`step.require` for a condition whose failure means something is wrong.

`expression.returnData` reads the data a called program returns, but not from a call with a `when`
condition, because a skipped call returns nothing.
