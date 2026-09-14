# Batch execution

A template may declare one repeated tail-account row. The caller supplies zero or more rows after
the fixed accounts, and the VM infers the iteration count from the remaining account count.

## Thirty-recipient payroll

::: code-group

```ts [TypeScript · template]
const payroll = defineTemplate({
  inputs: { lamportsPerRecipient: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_BYTES },
    treasury: { signer: true, writable: true },
  },
  batch: {
    maxIterations: 30,
    row: { recipient: { writable: true } },
  },
  steps: [
    step.forEach([
      systemTransfer({
        systemProgram: account.fixed('systemProgram'),
        from: account.fixed('treasury'),
        to: account.iteration('recipient'),
        lamports: expression.input('lamportsPerRecipient'),
      }),
    ]),
  ],
});
```

```rust [Rust · account rows]
let mut runtime_accounts = vec![
    AccountMeta::new_readonly(system_program, false),
    AccountMeta::new(treasury, true),
];
runtime_accounts.extend(
    recipients.iter().map(|key| AccountMeta::new(*key, false)),
);

let run = ballista_sdk::run_instruction(
    template,
    runtime_accounts,
    &lamports_per_recipient.to_le_bytes(),
);
```

:::

## Stride-two rows

```ts
batch: {
  maxIterations: 20,
  row: {
    owner: {},
    tokenAccount: { writable: true },
  },
},
steps: [
  step.forEach([
    assertAta({
      associatedTokenAccount: account.iteration('tokenAccount'),
      owner: account.iteration('owner'),
      mint: account.fixed('mint'),
      tokenProgram: account.fixed('tokenProgram'),
      associatedTokenProgram: account.fixed('associatedTokenProgram'),
    }),
    ensureAssociatedTokenAccount({ /* row account references */ }),
    tokenTransfer({ /* row destination */ }),
  ]),
]
```

The row stride must be `1..=8`, the tail count must divide evenly by the stride, and rows cannot
exceed `maxIterations`. There is no nested loop, backward jump, or condition-controlled `while`.
Root steps may execute before and after the loop.

::: warning Count CPIs, not just rows
The hard ceiling is 64 expanded CPIs. A 30-row body with two CPIs expands to 60; a third CPI would
make the template invalid at finalization.
:::
