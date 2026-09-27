# Protocol composition

Templates that call other programs in sequence, with checks between the calls. Ballista works with
any program. Your client still finds routes, quotes and accounts as it would for a plain
transaction; the template fixes the order of the calls and the conditions that must hold while they
run.

Each call to another program is a CPI (one program calling another), made with `step.invoke` and
instruction data your client supplies. Each recipe ends with a cost table, measured with Mollusk (a
tool that runs Solana programs locally), in compute units (Solana's measure of execution cost) and
transaction bytes. For these tables, every protocol call was replaced with a simple SOL transfer,
so the numbers include the cost of making each call but not the protocol's own work. In the table
headings, **weaker checks** means plain instructions can't enforce the template's check on chain,
and **closest plain instructions** means plain instructions can't do the job at all.

## Swap then deposit

Swap, check how many tokens the swap actually delivered, then deposit. Your client builds the
instruction data for both calls. Between them, the template requires `receivedTokens` to have grown
by at least `minimumOut`; if it hasn't, the deposit never happens and the whole run reverts. The
deposit amount is whatever your client put in the deposit data. To deposit exactly what the swap
produced, see [deposit exactly what a swap produced](/examples/protocols/jupiter-deposit).

::: code-group

```ts [TypeScript · template]
steps: [
  step.snapshot('before', expression.accountData(account.fixed('receivedTokens'), 64, 'u64')),
  step.invoke({
    program: account.fixed('swapProgram'),
    accounts: swapAccounts,
    data: [data.encode('bytes', expression.input('swapData'))],
  }),
  step.snapshot('afterSwap', expression.accountData(account.fixed('receivedTokens'), 64, 'u64')),
  step.require(expression.greaterThanOrEqual(
    expression.subtract(expression.snapshot('afterSwap'), expression.snapshot('before')),
    expression.input('minimumOut'),
  )),
  step.invoke({
    program: account.fixed('vaultProgram'),
    accounts: depositAccounts,
    data: [data.encode('bytes', expression.input('depositData'))],
  }),
]
```

```rust [Rust · route binding]
let inputs = encode_swap_deposit_inputs(
    quote.minimum_out,
    &quote.swap_instruction.data,
    &deposit_instruction.data,
);
let metas = merge_in_template_order(&quote.accounts, &deposit_accounts);
let run = ballista_sdk::run_instruction(template, metas, &inputs);
```

:::

<!-- benchmark:swap-then-deposit -->

| Cost | Ballista | Plain instructions, weaker checks | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 5,783 | 300 | +5,483 |
| Transaction bytes, every run | 385 | 278 | +107 |
| Compute units, upload once | 6,895 | none | — |
| Transaction bytes, upload once | 624 in 1 transaction | none | — |
| Rent locked in the template account | 0.00282 SOL for 428 bytes | none | — |

One Ballista instruction, compared with 2 plain instructions. A SOL transfer stands in for the protocol call, so neither column includes the protocol's own work. Both instructions can be sent back to back, but nothing checks how many tokens the swap produced, so a swap that returns too little still deposits. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->

## Claim then distribute

Claim rewards into a treasury token account, then pay the same amount to each recipient in a list.

::: code-group

```ts [TypeScript · template]
steps: [
  step.invoke({
    program: account.fixed('rewardsProgram'),
    accounts: claimAccounts,
    data: [data.literal(CLAIM_DISCRIMINATOR)],
  }),
  step.forEach([
    tokenTransfer({
      tokenProgram: account.fixed('tokenProgram'),
      source: account.fixed('treasuryTokens'),
      destination: account.iteration('recipientTokens'),
      authority: account.fixed('authority'),
      amount: expression.input('amountPerRecipient'),
    }),
  ]),
]
```

```rust [Rust · rows]
let mut metas = claim_and_treasury_metas;
metas.extend(recipient_token_accounts.into_iter().map(|key| AccountMeta::new(key, false)));
let run = ballista_sdk::run_instruction(template, metas, &amount.to_le_bytes());
```

:::

<!-- benchmark:claim-then-distribute -->

| Cost | Ballista | Plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 30,432 | 1,366 | +29,066 |
| Transaction bytes, every run | 974 | 1,148 | −174 |
| Compute units, upload once | 7,394 | none | — |
| Transaction bytes, upload once | 575 in 1 transaction | none | — |
| Rent locked in the template account | 0.00258 SOL for 379 bytes | none | — |

One Ballista instruction for 16 rows, compared with 17 plain instructions. A SOL transfer stands in for the protocol call, so neither column includes the protocol's own work. A claim instruction followed by one token transfer per recipient does the same work. What the template adds is one instruction and a sequence of calls stored on chain, not something plain instructions cannot do.

<!-- /benchmark -->

## Primary or fallback route

Include two routes, each behind a `when` condition, where one condition is the opposite of the
other. Each run calls exactly one route, chosen by the `usePrimary` input.

::: code-group

```ts [TypeScript · template]
const usePrimary = expression.input('usePrimary');

steps: [
  step.invoke({
    program: account.fixed('primaryProgram'),
    accounts: primaryAccounts,
    data: [data.encode('bytes', expression.input('primaryData'))],
    when: usePrimary,
  }),
  step.invoke({
    program: account.fixed('fallbackProgram'),
    accounts: fallbackAccounts,
    data: [data.encode('bytes', expression.input('fallbackData'))],
    when: expression.not(usePrimary),
  }),
]
```

```rust [Rust · route choice]
let mut inputs = vec![u8::from(use_primary)];
append_bounded_bytes(&mut inputs, &primary_data);
append_bounded_bytes(&mut inputs, &fallback_data);
let run = ballista_sdk::run_instruction(template, both_route_metas, &inputs);
```

:::

<!-- benchmark:primary-or-fallback-route -->

| Cost | Ballista | Plain instructions, weaker checks | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,512 | 150 | +3,362 |
| Transaction bytes, every run | 345 | 240 | +105 |
| Compute units, upload once | 4,907 | none | — |
| Transaction bytes, upload once | 520 in 1 transaction | none | — |
| Rent locked in the template account | 0.0023 SOL for 324 bytes | none | — |

One Ballista instruction, compared with 1 plain instruction. A SOL transfer stands in for the protocol call, so neither column includes the protocol's own work. The client picks a route and sends that one instruction. The choice is made before signing, not from state at execution time. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->

## Time-gated governance execution

Execute a governance action only if the proposal account says it is approved and its execution time
has passed. The template reads both fields from the proposal account during the run, then forwards
the execute instruction your client built. `APPROVED_OFFSET` and `TIME_OFFSET` are the byte
positions of those fields in your governance program's proposal account.

::: code-group

```ts [TypeScript · template]
const approved = expression.accountData(account.fixed('proposal'), APPROVED_OFFSET, 'bool');
const executableAfter = expression.accountData(account.fixed('proposal'), TIME_OFFSET, 'i64');

steps: [
  step.require(expression.and(
    approved,
    expression.greaterThanOrEqual(expression.clockUnixTimestamp(), executableAfter),
  )),
  step.invoke({
    program: account.fixed('governanceProgram'),
    accounts: executeAccounts,
    data: [data.encode('bytes', expression.input('executeData'))],
  }),
]
```

```rust [Rust · execute]
let inputs = encode_bounded_bytes(&execute_instruction.data);
let run = ballista_sdk::run_instruction(template, governance_metas, &inputs);
```

:::

<!-- benchmark:time-gated-governance-execution -->

| Cost | Ballista | Closest plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,828 | 150 | +3,678 |
| Transaction bytes, every run | 342 | 240 | +102 |
| Compute units, upload once | 7,835 | none | — |
| Transaction bytes, upload once | 520 in 1 transaction | none | — |
| Rent locked in the template account | 0.0023 SOL for 324 bytes | none | — |

One Ballista instruction, compared with 1 plain instruction. A SOL transfer stands in for the protocol call, so neither column includes the protocol's own work. A transaction cannot read a proposal flag and a timestamp and refuse to execute. Gating on chain needs a program.

<!-- /benchmark -->

## Bounded keeper crank

Call the same maintenance instruction, often called a crank, once for each market and queue pair in
a list; each pair is one row of a batch. A keeper, the bot or service that sends maintenance
transactions, signs each call. Ballista holds no authority over the accounts.

::: code-group

```ts [TypeScript · template]
batch: {
  maxIterations: 24,
  row: { market: { writable: true }, queue: { writable: true } },
},
steps: [step.forEach([
  step.invoke({
    program: account.fixed('protocolProgram'),
    accounts: [
      { account: account.iteration('market'), writable: true, signer: false },
      { account: account.iteration('queue'), writable: true, signer: false },
      { account: account.fixed('keeper'), writable: false, signer: true },
    ],
    data: [data.literal(CRANK_DISCRIMINATOR)],
  }),
])]
```

```rust [Rust · keeper run]
let mut metas = vec![
    AccountMeta::new_readonly(protocol_program, false),
    AccountMeta::new_readonly(keeper, true),
];
for (market, queue) in selected_rows {
    metas.push(AccountMeta::new(market, false));
    metas.push(AccountMeta::new(queue, false));
}
let run = ballista_sdk::run_instruction(template, metas, &[]);
```

:::

Ballista doesn't run on a schedule. A bot, a user or a keeper service still decides when to send
each run.

<!-- benchmark:bounded-keeper-crank -->

| Cost | Ballista | Plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 45,396 | 3,600 | +41,796 |
| Transaction bytes, every run | 1,826 | 2,162 | −336 |
| Compute units, upload once | 14,322 | none | — |
| Transaction bytes, upload once | 450 in 1 transaction | none | — |
| Rent locked in the template account | 0.00194 SOL for 254 bytes | none | — |

One Ballista instruction for 24 rows, compared with 24 plain instructions. A SOL transfer stands in for the protocol call, so neither column includes the protocol's own work. One crank instruction per row does the same work. What the template adds is one instruction and a sequence of calls stored on chain, not something plain instructions cannot do.

<!-- /benchmark -->
