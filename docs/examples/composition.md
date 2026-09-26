# Protocol composition

Ballista is protocol-neutral. The client still discovers routes, quotes, and accounts; the stored
template defines how those pieces may be composed and which runtime conditions must remain true.

## Swap then deposit

Feed client-built instruction data into two generic CPIs and assert the actual intermediate token
delta.

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

| Cost | Ballista | Plain instructions, weaker | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 5,783 | 300 | +5,483 |
| Transaction bytes, every run | 385 | 278 | +107 |
| Compute units, upload once | 6,895 | none | — |
| Transaction bytes, upload once | 624 in 1 transaction | none | — |
| Rent locked in the template account | 0.00282 SOL for 428 bytes | none | — |

One Ballista instruction against 2 plain instructions, measured with Mollusk. The protocol call is stood in by a System transfer, so neither row includes the protocol's own work. Both instructions can be sent back to back, but the intermediate token delta is never checked, so a bad fill still deposits. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->

## Claim then distribute

Claim once into a treasury account, then distribute a fixed amount across a bounded recipient table.

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

One Ballista instruction covering 16 rows against 17 plain instructions, measured with Mollusk. The protocol call is stood in by a System transfer, so neither row includes the protocol's own work. A claim instruction followed by one token transfer per recipient does the same work. Ballista buys one instruction and a stored, verified shape, not a capability you lack.

<!-- /benchmark -->

## Primary or fallback route

Compile two routes with complementary guards. Exactly one is invoked for each run.

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

| Cost | Ballista | Plain instructions, weaker | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,512 | 150 | +3,362 |
| Transaction bytes, every run | 345 | 240 | +105 |
| Compute units, upload once | 4,907 | none | — |
| Transaction bytes, upload once | 520 in 1 transaction | none | — |
| Rent locked in the template account | 0.0023 SOL for 324 bytes | none | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. The protocol call is stood in by a System transfer, so neither row includes the protocol's own work. The client picks a route and sends that one instruction. The choice is made before signing, not from state at execution time. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->

## Time-gated governance execution

Check an executable-after timestamp and a protocol state flag before forwarding an execution CPI.

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

| Cost | Ballista | Plain instructions, not equivalent | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,828 | 150 | +3,678 |
| Transaction bytes, every run | 342 | 240 | +102 |
| Compute units, upload once | 7,835 | none | — |
| Transaction bytes, upload once | 520 in 1 transaction | none | — |
| Rent locked in the template account | 0.0023 SOL for 324 bytes | none | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. The protocol call is stood in by a System transfer, so neither row includes the protocol's own work. A transaction cannot read a proposal flag and a timestamp and refuse to execute. Gating on chain needs a program.

<!-- /benchmark -->

## Bounded keeper crank

Call the same maintenance instruction over rows of existing protocol accounts without giving
Ballista scheduling or authority responsibilities.

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

Ballista does not wake itself. A bot, user, or keeper service still decides when to submit the run.

<!-- benchmark:bounded-keeper-crank -->

| Cost | Ballista | Plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 45,396 | 3,600 | +41,796 |
| Transaction bytes, every run | 1,826 | 2,162 | −336 |
| Compute units, upload once | 14,322 | none | — |
| Transaction bytes, upload once | 450 in 1 transaction | none | — |
| Rent locked in the template account | 0.00194 SOL for 254 bytes | none | — |

One Ballista instruction covering 24 rows against 24 plain instructions, measured with Mollusk. The protocol call is stood in by a System transfer, so neither row includes the protocol's own work. One crank instruction per row does the same work. Ballista buys one instruction and a stored, verified shape, not a capability you lack.

<!-- /benchmark -->
