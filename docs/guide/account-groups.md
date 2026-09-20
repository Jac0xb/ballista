# Account groups

A CPI's account list is positional and fixed when the template is authored. That is what lets the
verifier check every account a template touches, and it is also why a template that swaps through
an aggregator would otherwise be pinned to one route shape. Account groups are the escape hatch:
caller-sized lists of accounts that a template declares by name and a CPI forwards after its own
declared accounts, the way Solana programs pass "remaining accounts".

## What a group is

- The template declares up to eight groups by name: `accountGroups: ['routeA', 'routeB']`.
- At run time the caller supplies the members of each group after the fixed accounts and batch
  rows, and the run data starts with one byte per group giving its size. A group may be empty.
- An `invoke` names the group it forwards with `accountGroup`. The CPI receives the invoke's
  declared accounts first, then every member of the group, in the order the caller supplied them.
- Declared accounts plus group members must fit the 64-account CPI limit; a run that exceeds it
  fails with `CpiAccountLimitExceeded` (6021) and the total as context.

## What a group is not

Group members are opaque to the template. They have no constraints, cannot be read, and no
instruction can name one. Anything the template must check about the outcome (a balance delta, a
destination owner, a mint) is read from declared accounts, which is where a swap's user token
accounts belong.

Group members never sign. They are forwarded with the writable flag the transaction gave them and
with signer status cleared, so a template can only delegate a signature through a slot its author
declared. The accounts a route needs beyond the user's own are pools and vaults, which never sign.

Groups are per CPI, not per batch row. An `invoke` inside `forEach` forwards the same group on
every iteration.

## Choosing between swaps at run time

The template below snapshots three token balances, decides which of three swaps to run, and runs
the chosen ones. Each swap has its own route bytes and its own group, so any route over any token
fits the same finalized template. A swap the caller did not quote gets an empty group and empty
route bytes and costs nothing.

```ts
const rebalance = defineTemplate({
  accounts: {
    jupiter: { executable: true, address: JUPITER_V6 },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM },
    user: { signer: true, writable: true },
    // Not pinned to a mint: the caller decides which tokens are involved and the guards decide
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

The routes come from off-chain quotes; the template cannot compute one on chain. What it does is
refuse to complete unless the observed result clears the guards its author wrote.

## Budget

Nine fixed accounts plus three groups of about 25 is roughly 85 runtime accounts, inside the cap of
120. The transaction size limit of 1,232 bytes is the real ceiling: three swaps need an address
lookup table and single-hop or short routes.

## Rust

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
