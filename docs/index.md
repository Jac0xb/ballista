---
layout: home
title: Ballista — execution, composed
titleTemplate: false
description: A small machine for complex Solana transactions. Arrange calls, add conditions, store the sequence on chain, and run it with fresh inputs.
---

<div class="source-heading">
  <h2>A template, in practice.</h2>
  <span>§ 03 / THE SMALLEST USEFUL SEQUENCE</span>
</div>

Define a transfer in TypeScript, then call its stored template from either language.
The signer supplies the authority. Ballista supplies the sequence.

::: code-group

```ts [TypeScript · compose]
const transfer = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_BYTES },
    sender: { signer: true, writable: true },
    recipient: { writable: true },
  },
  steps: [
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('sender'),
      to: account.fixed('recipient'),
      lamports: expression.input('amount'),
    }),
  ],
});

const compiled = compileTemplate(transfer);
```

```rust [Rust · execute]
use ballista_sdk::run_instruction;
use solana_program::instruction::AccountMeta;

let run = run_instruction(
    template,
    vec![
        AccountMeta::new_readonly(system_program, false),
        AccountMeta::new(sender, true),
        AccountMeta::new(recipient, false),
    ],
    &amount.to_le_bytes(),
);
```

:::

The [getting started guide](/guide/getting-started) includes imports, compilation, upload, and your
first run. For the machine underneath, read the [wire format](/reference/wire-format).
