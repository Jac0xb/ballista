# `@jac0xb/ballista`

Zod authoring, deterministic flat-bytecode compilation, and instruction codecs for Ballista 0.3.

```ts
import { account, compileTemplate, defineTemplate, expression, systemTransfer } from '@jac0xb/ballista';

const task = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: new Uint8Array(32) },
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

const compiled = compileTemplate(task);
```

The core entry point has no RPC dependency. Import `@jac0xb/ballista/kit` for address conversion,
PDA derivation, transaction-v1 sizing, and Kit-native run or upload instructions.
