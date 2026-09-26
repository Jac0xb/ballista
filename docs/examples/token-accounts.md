# Token-account patterns

Offsets below use the legacy SPL Token account layout. Pin the Token Program address, account owner,
and minimum data length whenever reading raw fields.

Several patterns here are conveniences: a plain transaction sends the same instructions with the
same guarantees, and the measured tables say so. The ones that earn a template read a balance or
a flag mid-run — [forward the whole token balance](/examples/runtime-values#forward-the-whole-token-balance)
and [consolidate only the funded accounts](/examples/loops#consolidate-only-the-funded-accounts).

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

| Cost | Ballista | Plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 191,260 | 123,752 | +67,508 |
| Transaction bytes, every run | 1,007 | 1,122 | −115 |
| Compute units, upload once | 6,938 | none | — |
| Transaction bytes, upload once | 747 in 1 transaction | none | — |
| Rent locked in the template account | 0.00345 SOL for 551 bytes | none | — |

One Ballista instruction covering 8 rows against 16 plain instructions, measured with Mollusk. ATA CreateIdempotent then Transfer per recipient. The ATA program derives the address itself, so the guarantee matches. Ballista buys one instruction and a stored, verified shape, not a capability you lack.

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

| Cost | Ballista | Plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 55,183 | 2,432 | +52,751 |
| Transaction bytes, every run | 1,339 | 1,738 | −399 |
| Compute units, upload once | 5,366 | none | — |
| Transaction bytes, upload once | 451 in 1 transaction | none | — |
| Rent locked in the template account | 0.00195 SOL for 255 bytes | none | — |

One Ballista instruction covering 32 rows against 32 plain instructions, measured with Mollusk. One SPL Token transfer per destination does the same work. Ballista buys one instruction and a stored, verified shape, not a capability you lack.

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

| Cost | Ballista | Plain instructions | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 16,658 | 13,518 | +3,140 |
| Transaction bytes, every run | 407 | 341 | +66 |
| Compute units, upload once | 7,286 | none | — |
| Transaction bytes, upload once | 508 in 1 transaction | none | — |
| Rent locked in the template account | 0.00224 SOL for 312 bytes | none | — |

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

| Cost | Ballista | Plain instructions, weaker | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 34,064 | 1,888 | +32,176 |
| Transaction bytes, every run | 803 | 842 | −39 |
| Compute units, upload once | 8,575 | none | — |
| Transaction bytes, upload once | 471 in 1 transaction | none | — |
| Rent locked in the template account | 0.00205 SOL for 275 bytes | none | — |

One Ballista instruction covering 16 rows against 16 plain instructions, measured with Mollusk. CloseAccount per candidate works only while every candidate is empty: SPL Token rejects a funded account, which fails the whole transaction instead of skipping that row. Enforcing that on chain any other way means deploying your own program.

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

| Cost | Ballista | Plain instructions, weaker | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,780 | 76 | +3,704 |
| Transaction bytes, every run | 316 | 250 | +66 |
| Compute units, upload once | 6,338 | none | — |
| Transaction bytes, upload once | 515 in 1 transaction | none | — |
| Rent locked in the template account | 0.00227 SOL for 319 bytes | none | — |

One Ballista instruction against 1 plain instruction, measured with Mollusk. A bare transfer moves the tokens; nothing proves the source was debited by exactly that amount and no more. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->
