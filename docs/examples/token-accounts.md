# Token-account patterns

Offsets below use the legacy SPL Token account layout. Pin the Token Program address, account owner,
and minimum data length whenever reading raw fields.

## Assert, create, then transfer

Validate that the destination is the recipient's canonical ATA, create it only when empty, then
transfer tokens—all atomically.

::: code-group

```ts [TypeScript · loop body]
step.forEach([
  assertAta({
    associatedTokenAccount: account.iteration('destinationAta'),
    owner: account.iteration('recipient'),
    mint: account.fixed('mint'),
    tokenProgram: account.fixed('tokenProgram'),
    associatedTokenProgram: account.fixed('associatedTokenProgram'),
  }),
  ensureAssociatedTokenAccount({
    associatedTokenProgram: account.fixed('associatedTokenProgram'),
    payer: account.fixed('payer'),
    associatedTokenAccount: account.iteration('destinationAta'),
    owner: account.iteration('recipient'),
    mint: account.fixed('mint'),
    systemProgram: account.fixed('systemProgram'),
    tokenProgram: account.fixed('tokenProgram'),
  }),
  tokenTransfer({
    tokenProgram: account.fixed('tokenProgram'),
    source: account.fixed('source'),
    destination: account.iteration('destinationAta'),
    authority: account.fixed('authority'),
    amount: expression.input('amount'),
  }),
]);
```

```rust [Rust · row bindings]
for recipient in recipients {
    let ata = get_associated_token_address_with_program_id(&recipient, &mint, &token_program);
    metas.push(AccountMeta::new_readonly(recipient, false));
    metas.push(AccountMeta::new(ata, false));
}
let run = ballista_sdk::run_instruction(template, metas, &amount.to_le_bytes());
```

:::

<!-- benchmark:assert-create-then-transfer -->

| Approach | Compute units | Transaction bytes | Stored on chain |
| --- | ---: | ---: | --- |
| Ballista | 105,512 | 743 | 471-byte template, once |
| Plain instructions | 66,376 | 758 | none |
| Difference | +39,136 | −15 | — |

One Ballista instruction covering 4 rows against 8 plain instructions, measured with Mollusk. ATA CreateIdempotent then Transfer per recipient. The ATA program derives the address itself, so the guarantee matches. Ballista buys one instruction and a stored, verified shape, not a capability you lack.

<!-- /benchmark -->

## Existing-account token payroll

::: code-group

```ts [TypeScript · template]
batch: {
  maxIterations: 32,
  row: {
    destination: { writable: true, owner: TOKEN_PROGRAM_BYTES, minDataLength: 165 },
  },
},
steps: [step.forEach([
  tokenTransfer({
    tokenProgram: account.fixed('tokenProgram'),
    source: account.fixed('source'),
    destination: account.iteration('destination'),
    authority: account.fixed('authority'),
    amount: expression.input('amount'),
  }),
])]
```

```rust [Rust · run]
let mut metas = fixed_token_metas;
metas.extend(token_accounts.iter().map(|key| AccountMeta::new(*key, false)));
let run = ballista_sdk::run_instruction(template, metas, &amount.to_le_bytes());
```

:::

<!-- benchmark:existing-account-token-payroll -->

| Approach | Compute units | Transaction bytes | Stored on chain |
| --- | ---: | ---: | --- |
| Ballista | 17,074 | 547 | 175-byte template, once |
| Plain instructions | 608 | 586 | none |
| Difference | +16,466 | −39 | — |

One Ballista instruction covering 8 rows against 8 plain instructions, measured with Mollusk. One SPL Token transfer per destination does the same work. Ballista buys one instruction and a stored, verified shape, not a capability you lack.

<!-- /benchmark -->

## Conditional ATA setup

Use ordinary ATA `Create`, not `CreateIdempotent`, so the `isEmpty` guard has observable value.

::: code-group

```ts [TypeScript · template]
ensureAssociatedTokenAccount({
  associatedTokenProgram: account.fixed('associatedTokenProgram'),
  payer: account.fixed('payer'),
  associatedTokenAccount: account.fixed('ata'),
  owner: account.fixed('wallet'),
  mint: account.fixed('mint'),
  systemProgram: account.fixed('systemProgram'),
  tokenProgram: account.fixed('tokenProgram'),
});
```

```rust [Rust · run]
use solana_program::instruction::AccountMeta;

let run = ballista_sdk::run_instruction(
    template,
    vec![
        AccountMeta::new_readonly(associated_program, false),
        AccountMeta::new_readonly(token_program, false),
        AccountMeta::new_readonly(system_program, false),
        AccountMeta::new_readonly(mint, false),
        AccountMeta::new(payer, true),
        AccountMeta::new_readonly(wallet, false),
        AccountMeta::new(ata, false),
    ],
    &[],
);
// Re-running skips ordinary Create when `ata` already contains data.
```

:::

<!-- benchmark:conditional-ata-setup -->

| Approach | Compute units | Transaction bytes | Stored on chain |
| --- | ---: | ---: | --- |
| Ballista | 16,648 | 407 | 232-byte template, once |
| Plain instructions | 13,518 | 341 | none |
| Difference | +3,130 | +66 | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. ATA CreateIdempotent is the same behavior in one instruction. Ballista buys one instruction and a stored, verified shape, not a capability you lack.

<!-- /benchmark -->

## Close empty token accounts

Read the token amount at offset 64 and invoke SPL Token `CloseAccount` only when it is zero.

::: code-group

```ts [TypeScript · loop body]
const isEmpty = expression.equal(
  expression.accountData(account.iteration('tokenAccount'), 64, 'u64'),
  expression.u64(0),
);

step.invoke({
  program: account.fixed('tokenProgram'),
  accounts: [
    { account: account.iteration('tokenAccount'), writable: true, signer: false },
    { account: account.fixed('rentDestination'), writable: true, signer: false },
    { account: account.fixed('authority'), writable: false, signer: true },
  ],
  data: [data.literal(Uint8Array.of(9))],
  when: isEmpty,
});
```

```rust [Rust · account rows]
let mut metas = fixed_metas;
metas.extend(empty_candidates.iter().map(|key| AccountMeta::new(*key, false)));
let run = ballista_sdk::run_instruction(template, metas, &[]);
```

:::

<!-- benchmark:close-empty-token-accounts -->

| Approach | Compute units | Transaction bytes | Stored on chain |
| --- | ---: | ---: | --- |
| Ballista | 10,160 | 407 | 195-byte template, once |
| Plain instructions, weaker | 472 | 362 | none |
| Difference | +9,688 | +45 | — |

One Ballista instruction covering 4 rows against 4 plain instructions, measured with Mollusk. CloseAccount per candidate works only while every candidate is empty: SPL Token rejects a funded account, which fails the whole transaction instead of skipping that row. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->

## Exact token debit

::: code-group

```ts [TypeScript · invariant]
step.snapshot('before', expression.accountData(account.fixed('source'), 64, 'u64')),
tokenTransfer({ /* source, destination, authority, amount */ }),
step.require(expression.equal(
  expression.accountData(account.fixed('source'), 64, 'u64'),
  expression.subtract(expression.snapshot('before'), expression.input('amount')),
)),
```

```rust [Rust · run]
let run = ballista_sdk::run_instruction(template, token_metas, &amount.to_le_bytes());
// Unexpected fees or debits make the post-CPI requirement fail atomically.
```

:::

<!-- benchmark:exact-token-debit -->

| Approach | Compute units | Transaction bytes | Stored on chain |
| --- | ---: | ---: | --- |
| Ballista | 3,849 | 316 | 255-byte template, once |
| Plain instructions, weaker | 76 | 250 | none |
| Difference | +3,773 | +66 | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. A bare transfer moves the tokens; nothing proves the source was debited by exactly that amount and no more. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->
