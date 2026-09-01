# Ballista 0.3

Ballista is a bounded orchestration VM for reusable SVM transaction templates.

A creator compiles a template, stores it in an immutable PDA, and anyone can run it with typed
inputs and transaction accounts. The program validates flat bytecode before finalization, then
executes directly from borrowed account memory without rebuilding an AST.

Ballista is a clean break from the legacy Borsh task format. Old task accounts are not executable
by the 0.3 runtime.

## Devnet deployment

- Program: [`BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD`](https://explorer.solana.com/address/BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD?cluster=devnet)
- Upgrade authority: `5H4nnKkd9LjrpA9hw6RpxYY9aaEzK5rWNcc8nkagg4Yf`
- Deployment: [`49m5A7oEj1ZDHLDv6YhthDLzxYvWDM9L6b6uKhaqoVCntTJZ5bHD6YTxxLQgSPMqnt3YfPTFVdQ9ZQ1tF78LdZg1`](https://explorer.solana.com/tx/49m5A7oEj1ZDHLDv6YhthDLzxYvWDM9L6b6uKhaqoVCntTJZ5bHD6YTxxLQgSPMqnt3YfPTFVdQ9ZQ1tF78LdZg1?cluster=devnet)
- Deployed SBF: 78,192 bytes, SHA-256 `cd13bbf4d5e695ef9b50a2e6eaff749c0c9edd74c847efbc0cd5021ca2123b11`
- Explorer IDL: [`JDQL78RmakzfYKcWzAC56CmUGNhCMtje3HvDCyiH2xCX`](https://explorer.solana.com/address/JDQL78RmakzfYKcWzAC56CmUGNhCMtje3HvDCyiH2xCX?cluster=devnet)

The checked-in [IDL](idl/ballista.json) is published through Solana's Program Metadata program.
It describes Ballista's accounts and instructions for Explorer discovery; instruction fields marked
as raw trailing bytes still need the SDK's custom codecs rather than Anchor's Borsh encoder.

```bash
npx @solana-program/program-metadata@latest \
  --keypair /path/to/upgrade-authority.json \
  --rpc https://api.devnet.solana.com \
  write idl BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD idl/ballista.json
```

When the upgrade authority also pays, omit `--payer`; passing the same keypair through both flags
currently creates duplicate signer objects in the uploader.

## Conditional devnet USDC ATA

The [runnable TypeScript example](clients/js/examples/ensure-usdc-ata.ts) compiles an immutable
template that invokes the Associated Token Program's idempotent create instruction only when the
derived token account is empty. The condition is evaluated by Ballista during `Run`, so the caller
does not need an RPC existence check or a different transaction shape.

- Template: [`8jfDNcsxTqkWdhDsesg49RRrbRbaXUwUrBmZhE3R2fdS`](https://explorer.solana.com/address/8jfDNcsxTqkWdhDsesg49RRrbRbaXUwUrBmZhE3R2fdS?cluster=devnet), ID `21843`, 273-byte payload
- Template upload: [`thU1KCqun4ES8V6jgkZsmjJYF9dbr9uUxHAbZpMDWxhFCYuNs5gHiWBSATE4WEvjyzZDG2i2kLsUAN9ycRZmsLL`](https://explorer.solana.com/tx/thU1KCqun4ES8V6jgkZsmjJYF9dbr9uUxHAbZpMDWxhFCYuNs5gHiWBSATE4WEvjyzZDG2i2kLsUAN9ycRZmsLL?cluster=devnet)
- First run: [`3YYbs2Lv6V4BQNWXmDJ8u9huz5jUra3YX1cQ7SDcEcxYDr77gF6XGrdLWPTFpGZp5baXPSEnT95umhphgFNZ4Kq6`](https://explorer.solana.com/tx/3YYbs2Lv6V4BQNWXmDJ8u9huz5jUra3YX1cQ7SDcEcxYDr77gF6XGrdLWPTFpGZp5baXPSEnT95umhphgFNZ4Kq6?cluster=devnet) created the [USDC ATA](https://explorer.solana.com/address/4dqQVsm8JGmCE2mXpPu74x8Dc7wReuzdcEPjL2P9ZK31?cluster=devnet) in 16,400 CU
- Second run: [`4upq1cBPYUgTncaJce5TnXuzUyNeMhAiuJghjWPJLncCC2S6uo7D93JcM3kqwZycsYxhotXPxogdLiRw5J195vUV`](https://explorer.solana.com/tx/4upq1cBPYUgTncaJce5TnXuzUyNeMhAiuJghjWPJLncCC2S6uo7D93JcM3kqwZycsYxhotXPxogdLiRw5J195vUV?cluster=devnet) skipped the guarded CPI in 1,163 CU
- Measured run: [`2CMydKfZQUME1pYtJwtPumqqT4BnBWpz3D6TMVEyhrk1qyuvVk5izj9BknyuABgBoWYZQmxYkUnfZxTCCuvfoahX`](https://explorer.solana.com/tx/2CMydKfZQUME1pYtJwtPumqqT4BnBWpz3D6TMVEyhrk1qyuvVk5izj9BknyuABgBoWYZQmxYkUnfZxTCCuvfoahX?cluster=devnet) simulated and consumed exactly 1,313 CU, with a 1,445-CU buffered limit

Run the example with an explicitly configured devnet keypair and endpoint:

```bash
BALLISTA_KEYPAIR=/path/to/keypair.json \
SOLANA_RPC_URL=https://api.devnet.solana.com \
SOLANA_WS_URL=wss://api.devnet.solana.com \
pnpm --dir clients/js exec tsx examples/ensure-usdc-ata.ts run
```

Guards are ordinary typed boolean expressions and can be nested freely:

```ts
const when = expression.or(
  expression.and(expression.input('enabled'), expression.input('withinLimit')),
  expression.input('force'),
);
```

## Compute-unit measurement

The Kit adapter includes a compute-unit provider based on Kit 8.2's
`estimateResourceLimitsFactory`. It simulates the exact message with the maximum runtime budget,
adds the [recommended 10% safety margin](https://solana.com/docs/core/fees/compute-budget), caps the
request at 1,400,000 CUs, and carries the simulated loaded-account data limit into v1 messages.

```ts
import { createComputeUnitProvider } from '@jac0xb/ballista/kit';

const computeUnits = createComputeUnitProvider({ rpc });
const { transactionMessage: measuredMessage, estimate } =
  await computeUnits.estimateAndSet(transactionMessage);

console.log(estimate.simulatedComputeUnits, estimate.computeUnitLimit);
```

Simulation is a preflight estimate because account state can change before execution. After
confirmation, use `getComputeUnitsConsumed(transaction)` on the `getTransaction` response for the
authoritative executed value. Requested CUs—not consumed CUs—determine the legacy/v0 priority fee,
so callers should avoid a needlessly high limit. For on-program hotspot profiling, use Agave's
`compute_fn!` instrumentation in development builds; its logging has its own CU cost.

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
