# Token-account patterns

Templates that create, pay into, close and check SPL token accounts. An ATA (associated token
account) is the standard token account for a given wallet and mint.

Each recipe ends with a cost table, measured with Mollusk (a tool that runs Solana programs
locally). It compares one Ballista run with the plain instructions that do the same work, in
compute units (Solana's measure of execution cost) and transaction bytes. Where the column is
headed just **Plain instructions**, a plain transaction does the same work with the same
guarantees, for less compute. **Plain instructions, weaker checks** means the plain version can't enforce
the template's check on chain. Templates earn their cost when they read a balance or a flag while
the transaction runs, as in [forward the whole token balance](/guide/runtime-values#forward-the-whole-token-balance)
and [consolidate only the funded accounts](/guide/loops#consolidate-only-the-funded-accounts).

Some recipes read a token account's balance straight from its data. In the SPL Token account
layout, the balance is a `u64` at byte offset 64. When a template reads raw bytes like this, have it
also require the Token Program's address, the account's owner and its minimum data length (165
bytes for a token account), so it can't be handed a different kind of account.

## Assert, create, then transfer

For each recipient, check that the destination is the recipient's ATA, create it if it doesn't
exist yet, then transfer `amount` tokens to it. An ATA's address is a PDA (an address derived from
seeds, with no private key) of the Associated Token Account program, and its seeds are the owner,
the token program and the mint. `assertAta` derives that address and fails the run if the account
passed in doesn't match. If any step fails, the whole run reverts.

Each recipient is one row of a batch: two accounts, the recipient's wallet and its ATA.
`step.forEach` runs the three steps once per row.

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

One Ballista instruction for 8 rows, compared with 16 plain instructions. The associated token account program's create-if-missing instruction, then a transfer, for each recipient. That program derives the address itself, so the guarantee matches. What the template adds is one instruction and a sequence of calls stored on chain, not something plain instructions cannot do.

<!-- /benchmark -->

## Existing-account token payroll

Send the same token amount to up to 32 token accounts that already exist. Each destination must be
owned by the Token Program and be at least 165 bytes long, the size of a token account, so the
template can't be pointed at a different kind of account.

::: code-group

```ts [TypeScript · template]
batch: {
  maxIterations: 32,
  row: {
    destination: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 },
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

One Ballista instruction for 32 rows, compared with 32 plain instructions. One SPL Token transfer per destination does the same work. What the template adds is one instruction and a sequence of calls stored on chain, not something plain instructions cannot do.

<!-- /benchmark -->

## Conditional ATA setup

Create an ATA only if it doesn't exist yet. `ensureAssociatedTokenAccount` calls the ATA program's
`Create` instruction with a `when` condition that the account is empty, so a repeat run skips the
call instead of failing. It uses `Create` rather than `CreateIdempotent` so that the condition is
what does the work. As the cost table shows, `CreateIdempotent` on its own does the same job as one
plain instruction.

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

One Ballista instruction, compared with 1 plain instruction. The associated token account program's create-if-missing instruction does the same in one instruction. What the template adds is one instruction and a sequence of calls stored on chain, not something plain instructions cannot do.

<!-- /benchmark -->

## Close empty token accounts

For each token account in the list, read its balance (the `u64` at byte offset 64) and call SPL
Token's `CloseAccount` only if it is zero. Accounts that still hold tokens are skipped, so one
funded account doesn't fail the whole run. Closing an account returns its rent (the SOL deposit
that keeps an account open) to `rentDestination`.

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

| Cost | Ballista | Plain instructions, weaker checks | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 34,064 | 1,888 | +32,176 |
| Transaction bytes, every run | 803 | 842 | −39 |
| Compute units, upload once | 8,575 | none | — |
| Transaction bytes, upload once | 471 in 1 transaction | none | — |
| Rent locked in the template account | 0.00205 SOL for 275 bytes | none | — |

One Ballista instruction for 16 rows, compared with 16 plain instructions. One close instruction per account works only while every account is empty: the token program refuses to close a funded account, which fails the whole transaction instead of skipping that account. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->

## Exact token debit

Transfer tokens, then check that the source balance dropped by exactly `amount`. The template
records the balance before the transfer and compares it afterwards; any other change fails the
whole run.

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
// If the source changes by anything other than `amount`, the check after the transfer fails the run.
```

:::

<!-- benchmark:exact-token-debit -->

| Cost | Ballista | Plain instructions, weaker checks | Difference |
| --- | ---: | ---: | ---: |
| Compute units, every run | 3,780 | 76 | +3,704 |
| Transaction bytes, every run | 316 | 250 | +66 |
| Compute units, upload once | 6,338 | none | — |
| Transaction bytes, upload once | 515 in 1 transaction | none | — |
| Rent locked in the template account | 0.00227 SOL for 319 bytes | none | — |

One Ballista instruction, compared with 1 plain instruction. A bare transfer moves the tokens; nothing proves the source was debited by exactly that amount and no more. Enforcing that on chain any other way means deploying your own program.

<!-- /benchmark -->
