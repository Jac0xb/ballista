# Ballista

Ballista is an on-chain transaction-template engine for Solana. Define a multi-program workflow
once, compile it into bounded bytecode, store it in an immutable account, and let any caller execute
it with fresh inputs and accounts—without deploying another custom program.

It is designed for the space between a one-off transaction and a bespoke smart contract: payroll
batches, guarded token operations, account setup, treasury flows, post-CPI invariants, and other
complex but finite orchestration. Templates can read accounts and the clock, perform checked typed
math, require conditions, invoke arbitrary programs, and iterate one statically bounded account
range. They cannot keep mutable state, run unbounded loops, custody PDAs, or invent signer authority.

The runtime validates the complete flat program before finalization and then executes fixed-size
records directly from borrowed account memory. Developer names and `let` bindings disappear during
compilation; there is no Borsh AST to allocate or deserialize during `Run`.

Explore the [documentation and dual TypeScript/Rust examples](https://jac0xb.github.io/ballista/).

Ballista is a clean break from the legacy Borsh task format. Old task accounts are not executable
by the 0.3 runtime.

## Devnet deployment

- Program: [`BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD`](https://explorer.solana.com/address/BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD?cluster=devnet)
- Upgrade authority: `5H4nnKkd9LjrpA9hw6RpxYY9aaEzK5rWNcc8nkagg4Yf`
- Deployment: [`49m5A7oEj1ZDHLDv6YhthDLzxYvWDM9L6b6uKhaqoVCntTJZ5bHD6YTxxLQgSPMqnt3YfPTFVdQ9ZQ1tF78LdZg1`](https://explorer.solana.com/tx/49m5A7oEj1ZDHLDv6YhthDLzxYvWDM9L6b6uKhaqoVCntTJZ5bHD6YTxxLQgSPMqnt3YfPTFVdQ9ZQ1tF78LdZg1?cluster=devnet)
- Deployed SBF: 78,192 bytes, SHA-256 `cd13bbf4d5e695ef9b50a2e6eaff749c0c9edd74c847efbc0cd5021ca2123b11`
- Explorer IDL: [`JDQL78RmakzfYKcWzAC56CmUGNhCMtje3HvDCyiH2xCX`](https://explorer.solana.com/address/JDQL78RmakzfYKcWzAC56CmUGNhCMtje3HvDCyiH2xCX?cluster=devnet)

That deployment is an earlier pre-release build and rejects templates compiled from this
repository with `UnsupportedVersion`. The program is deployed once and immutably; until that
deployment exists, run templates locally with `pnpm build:program && pnpm test:integration`. See
the [deployment policy](https://jac0xb.github.io/ballista/guide/devnet).

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
template that invokes the Associated Token Program's ordinary, non-idempotent `Create` instruction
only when the derived token account is empty. The condition is evaluated by Ballista during `Run`,
so the caller does not need an RPC existence check or a different transaction shape. A repeat run
therefore proves the guard was applied: the same CPI would fail if Ballista invoked it again.

- Template: [`5ttfid6DiryiFiPwoQZBQXaojHiXjf9nTJB93oVELvJB`](https://explorer.solana.com/address/5ttfid6DiryiFiPwoQZBQXaojHiXjf9nTJB93oVELvJB?cluster=devnet), ID `21844`, 264-byte payload
- Template upload: [`61vW5fapGuwfVJQgMZaHvRqQkYwccjkUiGKXnEBPohw2kb3ut2LEPqFnn5Sa9yZ7iyweoxDn9pVmTeBbn8zUmKgv`](https://explorer.solana.com/tx/61vW5fapGuwfVJQgMZaHvRqQkYwccjkUiGKXnEBPohw2kb3ut2LEPqFnn5Sa9yZ7iyweoxDn9pVmTeBbn8zUmKgv?cluster=devnet)
- First run: [`4RuKcVTGA6uUpqifbDH5UAGRrVgbCjoFjH6hte4LQq3rKKrVHesja1HWBPZysZyfrEtdxce7aX95JuoEHXw1SVc4`](https://explorer.solana.com/tx/4RuKcVTGA6uUpqifbDH5UAGRrVgbCjoFjH6hte4LQq3rKKrVHesja1HWBPZysZyfrEtdxce7aX95JuoEHXw1SVc4?cluster=devnet) logged ordinary `Create` and created the [USDC ATA](https://explorer.solana.com/address/39mgswt673yjLKFHSstpT9UsQrPpzoimbDsYA1e16XS4?cluster=devnet) in 18,042 CU
- Repeat run: [`124J8UQzfJpJGpgoWZ3XxWHzH66LsrE66AziHvUumAxhUm2oHyrEcAqZ2W9tUYBXWyd9tKrhFPARPKmKTYm16G4s`](https://explorer.solana.com/tx/124J8UQzfJpJGpgoWZ3XxWHzH66LsrE66AziHvUumAxhUm2oHyrEcAqZ2W9tUYBXWyd9tKrhFPARPKmKTYm16G4s?cluster=devnet) succeeded in 1,318 CU with no inner instructions; without the guard, ordinary `Create` would fail on the existing ATA

Run the example with an explicitly configured devnet keypair and endpoint:

```bash
BALLISTA_KEYPAIR=/path/to/keypair.json \
BALLISTA_TEMPLATE_ADDRESS=5ttfid6DiryiFiPwoQZBQXaojHiXjf9nTJB93oVELvJB \
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
request at 1,400,000 CUs, and rounds the simulated loaded-account data limit up to a 32 KiB page
for v1 messages.

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

The 4,096-byte ceiling is opt-in through transaction v1; legacy and v0 remain limited to 1,232
bytes. V1 has no address lookup tables and permits at most 64 inline account addresses. It also
requires explicit compute-unit and loaded-account-data limits, both of which
`estimateAndSet` writes into the v1 message config. Compute Budget instructions are no-ops in v1,
and RPC readers must pass `maxSupportedTransactionVersion: 1` when fetching transactions or
blocks. See Solana's [larger transaction migration guide](https://solana.com/upgrades/larger-transaction-sizes).

## Execution model

- Guarded generic CPIs; protocol helpers exist only in the SDK and declare the program they target.
- Typed values: `bool`, `u64`, `i64`, `u128`, `pubkey`, and bounded `bytes`.
- Checked arithmetic, comparisons, casts, account/clock reads, `select`, and `require`.
- Fixed-offset data reads checked against the account's declared length at finalize, plus
  dynamic-offset reads and a guarded read of the previous CPI's return data.
- Lexical `let`/`snapshot` bindings for pre/post-CPI delta assertions without persistent state.
- Loop-carried variables that survive across batch rows, so a template can enforce a total.
- Canonical PDA derivation and SDK-level `assertPda` / `assertAta` relationship guards.
- One optional bounded tail-account iterator with stride `1..=8`, a minimum row count, and at most
  64 expanded CPIs.
- Constant heap per run: CPI scratch is allocated once and PDA seeds live on the stack.
- Failures name their location: the error code carries the program counter, account index, or
  input index, and the SDK maps it back to the authoring step. Invoked programs' errors pass
  through untouched.
- Programs and data reads must be pinned unless the author opts out with `unsafeUnpinned`.
- Public, repeatable execution. CPI signers must already be outer transaction signers.
- No PDA custody, mutable instance state, scheduler, replay policy, or unbounded control flow.

## Quick start

Requirements: Node.js 22+, pnpm 11.24, Rust, and Solana CLI 4.2+ for local transaction-v1 tests.

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
pnpm test               # Rust core and property tests + TypeScript SDK tests
pnpm check              # Rust, SDK typecheck, and docs build
pnpm fixtures           # regenerate the shared compiler fixtures after a compiler change
pnpm build:sdk          # ESM and declarations
pnpm build:program      # Solana SBF program
pnpm test:integration   # Agave-aligned Mollusk suite (build program first)
pnpm docs:dev           # local documentation server

cargo run -p ballista-sdk --example author_template   # author templates from Rust
cargo run -p ballista-sdk --example run_template      # encode inputs and decode errors from Rust
```

## Template lifecycle

Template PDAs use `['template', creator, templateId]`. Small payloads use `CreateTemplate`;
larger payloads use `BeginTemplate`, sequential `WriteTemplateChunk` calls, and `FinalizeTemplate`.
Only uploading templates can be cancelled. Finalized bytes are immutable and cannot be closed.

The 80-byte account header records creator, ID, upload state, lengths, bump, and SHA-256 payload
hash. The payload is a canonical series of fixed-size Zerocopy record tables followed by constant
pubkeys and literal bytes. See [the scope and limits](docs/scope.md) and the
[25-use-case capability matrix](docs/use-cases.md). Current packet, compute, and heap observations are in
[the measurements note](docs/benchmarks.md).

## Repository layout

- `programs/ballista`: Pinocchio on-chain program.
- `common`: flat wire format, borrowed account view, static verifier, and the `ProgramBuilder`.
- `clients/js`: Zod-first authoring compiler, codecs, error decoding, and the Kit adapter.
- `clients/rust`: Rust authoring, instruction codecs, typed inputs, and error decoding.
- `fixtures`: compiler output and error-name tables shared by the Rust and TypeScript suites.
- `tests/ballista`: Mollusk integration suite against the compiled SBF program.
- `certora`: Certora Solana Prover specifications and the `cvlr-pinocchio` adapter; see the
  [formal verification guide](https://jac0xb.github.io/ballista/guide/formal-verification).

Licensed under MIT.
