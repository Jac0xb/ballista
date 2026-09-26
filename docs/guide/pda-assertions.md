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
the runtime reserves the final seed for the bump.

## Supply the bump

Searching for the canonical bump means hashing with 255, then 254, and so on until the result is
off the curve. The runtime charges 1,500 compute units for every attempt, and a run cannot know in
advance how many it will take. The caller already knows the answer: the bump is public, stable,
and derived off chain for free. Pass it and the derivation runs once.

```ts
assertPda({
  account: account.fixed('position'),
  program: account.fixed('protocolProgram'),
  seeds: [
    expression.bytes(new TextEncoder().encode('position')),
    expression.accountField(account.fixed('owner'), 'key'),
    expression.input('positionId'),
  ],
  bump: expression.input('positionBump'), // u64, 0 to 255
});
```

`assertAta` takes the same option, and `expression.pda(program, seeds, bump)` is the underlying
expression. Inside a batch, give each row its own bump with a row input:

```ts
batch: {
  maxIterations: 32,
  row: { recipient: {}, ata: { writable: true } },
  rowInputs: { ataBump: { type: 'u64' } },
},
steps: [step.forEach([
  assertAta({
    associatedTokenAccount: account.iteration('ata'),
    owner: account.iteration('recipient'),
    mint: account.fixed('mint'),
    tokenProgram: account.fixed('tokenProgram'),
    associatedTokenProgram: account.fixed('associatedTokenProgram'),
    bump: expression.rowInput('ataBump'),
  }),
])]
```

The guarantee is unchanged. A wrong bump either produces an address on the curve, which is not a
valid program address and fails the run, or produces a different off-curve address, which fails
the comparison. Nothing a caller can pass makes a substituted account pass the check.

| Derivation | Compute units |
| --- | ---: |
| Canonical search, one literal seed | 4,852 |
| Supplied bump, one literal seed | 1,898 |

The search cost grows with the bump's depth; the supplied cost does not. For an associated token
account whose canonical bump is 250, the same assertion costs 11,325 units searching and 4,069
with the bump supplied. Full numbers are in the [compute profile](/cu-profile).

::: warning Variable compute
Without a supplied bump the search has variable compute cost. Measure templates that derive many
PDAs, especially inside a batch loop.
:::
