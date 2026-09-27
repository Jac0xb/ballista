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

```ts
const rebalance = defineTemplate({
  accounts: {
    jupiter: { executable: true, address: JUPITER_V6 },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM },
    user: { signer: true, writable: true },
    // No fixed mint: the caller decides which tokens are involved, and the checks decide
    // whether the result is acceptable. `owner` lets the template read their data.
    sourceA: { writable: true, owner: TOKEN_PROGRAM },
    destinationA: { writable: true, owner: TOKEN_PROGRAM },
    sourceB: { writable: true, owner: TOKEN_PROGRAM },
    destinationB: { writable: true, owner: TOKEN_PROGRAM },
    sourceC: { writable: true, owner: TOKEN_PROGRAM },
    destinationC: { writable: true, owner: TOKEN_PROGRAM },
  },
  inputs: {
    routeA: { type: 'bytes', maxLength: 512 },
    routeB: { type: 'bytes', maxLength: 512 },
    routeC: { type: 'bytes', maxLength: 512 },
    targetA: { type: 'u64' }, targetB: { type: 'u64' }, targetC: { type: 'u64' },
    minOutA: { type: 'u64' }, minOutB: { type: 'u64' }, minOutC: { type: 'u64' },
  },
  accountGroups: ['ammA', 'ammB', 'ammC'],
  steps: [
    // SPL token amount is the u64 at offset 64; owner is the pubkey at offset 32.
    step.snapshot('balanceA', expression.accountData(account.fixed('destinationA'), 64, 'u64')),
    step.let('needA', expression.lessThan(expression.snapshot('balanceA'), expression.input('targetA'))),
    step.require(expression.equal(
      expression.accountData(account.fixed('destinationA'), 32, 'pubkey'),
      expression.accountField(account.fixed('user'), 'key'),
    )),
    step.invoke({
      program: account.fixed('jupiter'),
      accounts: [
        // Jupiter's fixed accounts, in its order (abbreviated).
        { account: account.fixed('tokenProgram') },
        { account: account.fixed('user'), signer: true },
        { account: account.fixed('sourceA'), writable: true },
        { account: account.fixed('destinationA'), writable: true },
      ],
      accountGroup: 'ammA',
      data: [data.encode('bytes', expression.input('routeA'))],
      when: expression.variable('needA'),
    }),
    step.require(expression.or(
      expression.not(expression.variable('needA')),
      expression.greaterThanOrEqual(
        expression.subtract(
          expression.accountData(account.fixed('destinationA'), 64, 'u64'),
          expression.snapshot('balanceA'),
        ),
        expression.input('minOutA'),
      ),
    )),
    // ...the same four steps for B with ammB, and for C with ammC
  ],
});
```

For each swap, the template:

1. records the destination account's token balance;
2. decides whether the swap is needed, which is when that balance is below its target;
3. requires that the destination account belongs to the user;
4. runs the swap only if it is needed and, if it ran, requires that the balance rose by at least
   the minimum.

Running it with swaps A and C quoted and B skipped:

```ts
const run = buildKitRunInstruction({
  compiled,
  templateAddress,
  accounts: { jupiter, tokenProgram, user, sourceA, destinationA, sourceB, destinationB, sourceC, destinationC },
  inputs: {
    routeA: quoteA.data, routeB: new Uint8Array(), routeC: quoteC.data,
    targetA, targetB, targetC, minOutA, minOutB, minOutC,
  },
  accountGroups: {
    ammA: quoteA.remainingAccounts.map((meta) => ({ address: meta.address, writable: meta.isWritable })),
    ammB: [],
    ammC: quoteC.remainingAccounts.map((meta) => ({ address: meta.address, writable: meta.isWritable })),
  },
});
```

The routes come from off-chain quotes; a template cannot find a route on chain. What it can do is
fail the transaction unless the result passes the checks its author wrote.

## Budget

Nine fixed accounts plus three groups of about 25 accounts each come to roughly 85 accounts, within
Ballista's limit of 120 accounts per run (not counting the template account). Transaction size is
the tighter limit. A [version 1 transaction](/guide/transaction-v1) allows 4,096 bytes but at most
64 account addresses and no address lookup tables, so it cannot carry this many accounts. A version
0 transaction supports lookup tables but is limited to 1,232 bytes, so three swaps in one run need
an address lookup table and single-hop or short routes.

## Rust

The same pattern with the Rust builder, for a single group:

```rust
let mut builder = ProgramBuilder::new();
builder.account_groups(1);
let jupiter = builder.account(ACCOUNT_EXECUTABLE, Some(JUPITER_V6), None, 0);
let user = builder.account(ACCOUNT_SIGNER | ACCOUNT_WRITABLE, None, None, 0);
let route_input = builder.input(VALUE_BYTES, 512);
let route = builder.load_input(route_input);
let swap = builder.cpi_with_group(
    jupiter,
    &[(user, ACCOUNT_SIGNER | ACCOUNT_WRITABLE)],
    &[Segment::Register(DATA_REG_BYTES, route)],
    0,
);
builder.invoke(swap, None);

// Run data: one length byte per group, then the fixed inputs.
let inputs = RunInputs::new().groups(&[amm_accounts.len() as u8]).bytes(&quote.data).finish();
let mut accounts = vec![AccountMeta::new_readonly(JUPITER_V6, false), AccountMeta::new(user, true)];
accounts.extend(amm_accounts);
let run = ballista_sdk::run_instruction(template, accounts, &inputs);
```
