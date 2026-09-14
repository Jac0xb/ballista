# PDA and ATA assertions

Canonical PDA derivation lets a template prove that caller-supplied accounts have the relationship
the workflow expects. It does not let Ballista sign for those accounts.

## Assert an associated token account

::: code-group

```ts [TypeScript · template]
assertAta({
  associatedTokenAccount: account.fixed('destinationAta'),
  owner: account.fixed('recipient'),
  mint: account.fixed('mint'),
  tokenProgram: account.fixed('tokenProgram'),
  associatedTokenProgram: account.fixed('associatedTokenProgram'),
});
```

```rust [Rust · account binding]
let destination_ata = get_associated_token_address_with_program_id(
    &recipient,
    &mint,
    &token_program,
);

let run = ballista_sdk::run_instruction(
    template,
    vec![
        AccountMeta::new_readonly(associated_token_program, false),
        AccountMeta::new_readonly(token_program, false),
        AccountMeta::new_readonly(recipient, false),
        AccountMeta::new_readonly(mint, false),
        AccountMeta::new(destination_ata, false),
    ],
    &[],
);
```

:::

`assertAta` derives `[owner, tokenProgram, mint]` under the Associated Token Program and compares
the result with the supplied token-account key.

## Assert an arbitrary PDA

```ts
assertPda({
  account: account.fixed('position'),
  program: account.fixed('protocolProgram'),
  seeds: [
    expression.bytes(new TextEncoder().encode('position')),
    expression.accountField(account.fixed('owner'), 'key'),
    expression.input('positionId'), // u64, encoded little-endian
  ],
});
```

The program account must be declared executable. Each expression is encoded according to its VM
type. A template may supply at most 15 seeds, each with a statically proven maximum of 32 bytes;
the runtime reserves the final seed for the canonical bump search.

::: warning Variable compute
Canonical bump search has variable compute cost. Measure templates that derive many PDAs, especially
inside a batch loop.
:::
