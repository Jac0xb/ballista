# `@jac0xb/ballista`

Zod authoring, deterministic flat-bytecode compilation, and instruction codecs for Ballista.

```ts
import { account, compileTemplate, defineTemplate, expression, step, systemTransfer } from '@jac0xb/ballista';

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

Snapshot values before a CPI and assert the postcondition without adding mutable on-chain state:

```ts
steps: [
  step.snapshot('before', expression.accountField(account.fixed('sender'), 'lamports')),
  systemTransfer({ /* ... */ }),
  step.require(
    expression.equal(
      expression.accountField(account.fixed('sender'), 'lamports'),
      expression.subtract(expression.snapshot('before'), expression.input('amount')),
    ),
  ),
]
```

`assertPda` verifies a canonical program-derived address; `assertAta` specializes it to the
Associated Token Program's `[owner, tokenProgram, mint]` seeds. These are relationship assertions,
not PDA signer support.

The core entry point has no RPC dependency. Import `@jac0xb/ballista/kit` for address conversion,
PDA derivation, transaction-v1 sizing, and Kit-native run or upload instructions. V1 messages need
explicit compute and loaded-account-data limits; `createComputeUnitProvider().estimateAndSet()`
sets both from one simulation with CU and 32 KiB data-size headroom.
