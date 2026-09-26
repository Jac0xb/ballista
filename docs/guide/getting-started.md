# Getting started

This walkthrough compiles a SOL-transfer template, plans its immutable upload, and builds a run
instruction. Ballista is pre-release; use the workspace packages until the npm and crates.io
artifacts are published.

## Install the workspace

```bash
git clone https://github.com/Jac0xb/ballista.git
cd ballista
corepack enable
pnpm install
pnpm test
```

Requirements are Node.js 22+, pnpm 11.24, Rust, and Solana CLI 4.2+ for transaction-v1 local tests.

## 1. Define and compile

```ts
import {
  account,
  compileTemplate,
  defineTemplate,
  expression,
  systemTransfer,
} from '@jac0xb/ballista';

const template = defineTemplate({
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

const compiled = compileTemplate(template);
console.log(compiled.hash, compiled.stats);
```

Compilation is deterministic. Account and input names are developer-facing only; the payload uses
indices and fixed-size records.

## 2. Plan an upload

::: code-group

```ts [TypeScript]
const upload = planTemplateUpload(compiled, 7);

for (const instruction of upload.instructions) {
  console.log(instruction.kind, instruction.data.length);
}
```

```rust [Rust]
use ballista_sdk::{create_template_instruction, find_template_pda};

let payload = std::fs::read("artifacts/sol-transfer.bvm")?;
let (template, _) = find_template_pda(&creator, 7);
let create = create_template_instruction(creator, 7, &payload);
```

:::

Small templates use `CreateTemplate`. Larger templates use begin, sequential writes, and finalize;
the SDK can resume from the account's `written_len`.

## 3. Build a run

::: code-group

```ts [TypeScript]
const run = buildRunInstruction({
  compiled,
  programAddress: BALLISTA_BYTES,
  templateAddress: TEMPLATE_BYTES,
  inputs: { amount: 50_000_000n },
  accounts: {
    systemProgram: { address: SYSTEM_PROGRAM_BYTES },
    sender: { address: SENDER_BYTES },
    recipient: { address: RECIPIENT_BYTES },
  },
});
```

```rust [Rust]
let mut inputs = Vec::new();
inputs.extend_from_slice(&50_000_000u64.to_le_bytes());

let run = ballista_sdk::run_instruction(
    template,
    runtime_accounts,
    &inputs,
);
```

:::

The creator is not required to run a finalized template. Any account declared as a signer must be a
signer in the outer transaction.

## Next

- Learn the [template lifecycle](/guide/template-lifecycle).
- Add [snapshots and assertions](/guide/assertions).
- Build [30-recipient payroll](/examples/payments#bounded-sol-payroll).
- Send with [transaction v1](/guide/transaction-v1).
