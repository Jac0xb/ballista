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
