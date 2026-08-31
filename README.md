# Ballista 0.3

Ballista is a bounded orchestration VM for reusable SVM transaction templates.

A creator compiles a template, stores it in an immutable PDA, and anyone can run it with typed
inputs and transaction accounts. The program validates flat bytecode before finalization, then
executes directly from borrowed account memory without rebuilding an AST.

Ballista is a clean break from the legacy Borsh task format. Old task accounts are not executable
by the 0.3 runtime.

## Execution model

- Guarded generic CPIs; protocol helpers exist only in the SDK.
- Typed values: `bool`, `u64`, `i64`, `u128`, `pubkey`, and bounded `bytes`.
- Checked arithmetic, comparisons, casts, account/clock reads, `select`, and `require`.
- One optional bounded tail-account iterator with stride `1..=8` and at most 64 expanded CPIs.
- Public, repeatable execution. CPI signers must already be outer transaction signers.
- No PDA custody, mutable instance state, scheduler, replay policy, or unbounded control flow.

## Quick start

Requirements: Node.js 22+, pnpm 11.24, Rust, and Solana CLI 4.1+.

```bash
pnpm install
pnpm test
```

Define and compile a reusable SOL transfer:

```ts
import {
  account,
  compileTemplate,
  defineTemplate,
  expression,
  planTemplateUpload,
  systemTransfer,
} from '@jac0xb/ballista';

const transfer = defineTemplate({
  inputs: { lamports: { type: 'u64' } },
  accounts: {
    systemProgram: { address: new Uint8Array(32), executable: true },
    sender: { signer: true, writable: true },
    recipient: { writable: true },
  },
  steps: [
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('sender'),
      to: account.fixed('recipient'),
      lamports: expression.input('lamports'),
    }),
  ],
});

const compiled = compileTemplate(transfer);
const upload = planTemplateUpload(compiled, 0);
```

Use `@jac0xb/ballista/kit` for v1 transaction construction, exact packet measurement, PDA
derivation, and run/upload instruction conversion.

## Commands

```bash
pnpm test               # fast Rust core tests + TypeScript SDK tests
pnpm check              # supported Rust crates + SDK typecheck
pnpm build:sdk          # ESM and declarations
pnpm build:program      # Solana SBF program
pnpm test:integration   # Agave-aligned SBF lifecycle/CPI suite (build program first)
```

## Template lifecycle

Template PDAs use `['template-v2', creator, templateId]`. Small payloads use `CreateTemplate`;
larger payloads use `BeginTemplate`, sequential `WriteTemplateChunk` calls, and `FinalizeTemplate`.
Only uploading templates can be cancelled. Finalized bytes are immutable and cannot be closed.

The 80-byte account header records creator, ID, upload state, lengths, bump, and SHA-256 payload
hash. The payload is a canonical series of fixed-size Zerocopy record tables followed by constant
pubkeys and literal bytes. See [the scope and limits](docs/scope.md) and the
[25-use-case capability matrix](usecases.md). Current packet, compute, and heap observations are in
[the measurements note](docs/benchmarks.md).

## Repository layout

- `programs/ballista`: Pinocchio on-chain program.
- `common`: flat wire format, borrowed account view, and static verifier.
- `clients/js`: Zod-first authoring SDK and codecs.
- `clients/rust`: Rust instruction codecs and account/PDA helpers.

Licensed under MIT.
