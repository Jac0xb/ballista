---
next:
  text: TypeScript SDK
  link: /reference/typescript
---

# Devnet workflow

Where the Ballista program is deployed, and how to run and upload the example template on devnet.

## Deployment policy

Each release of the Ballista program will be deployed immutably, under its own address and with
no upgrade authority (the key that could replace a program's code). A
[finalized](/reference/glossary#finalize) template can't be edited either, so once a release is
deployed that way, nothing can change what Ballista does with a template. The programs a template
calls keep their own upgrade authorities. A template belongs to the program deployment that
finalized it: a new version of the program is a new deployment at a new address, and templates
must be uploaded again under it.

| Build | Program | Status |
| --- | --- | --- |
| Pre-release | [`BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD`](https://explorer.solana.com/address/BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD?cluster=devnet) | On devnet. Built before the current template format and before templates could derive program addresses on chain (`assertPda`, `assertAta`). Has an upgrade authority, so you trust its holder: that key can change what every template on this build does. |
| Current | Not deployed yet | Run it locally with the test suite |

::: warning Templates from this repository need the final deployment
The devnet program above was built for an older template format. It rejects templates compiled
from this repository with `InvalidTemplateProgram` (custom error 6002). Once the final program is
deployed, pass its address to the SDKs. Until then, run templates locally: on a local validator,
as [Getting started](/guide/getting-started) does, or with
`pnpm build:program && pnpm test:integration`, which builds the program and runs the test suite in
Mollusk, a harness that runs Solana programs without a validator.
:::

## Run the ATA example

This runs a template that is already stored on devnet under the pre-release program. It creates
the devnet USDC associated token account (ATA: the standard token account address for a given
wallet and mint) for the keypair's wallet, or for `BALLISTA_WALLET` if you set it, when that
account does not exist yet.

```bash
BALLISTA_KEYPAIR=/path/to/keypair.json \
BALLISTA_TEMPLATE_ADDRESS=5ttfid6DiryiFiPwoQZBQXaojHiXjf9nTJB93oVELvJB \
SOLANA_RPC_URL=https://api.devnet.solana.com \
SOLANA_WS_URL=wss://api.devnet.solana.com \
pnpm --dir clients/js exec tsx examples/ensure-usdc-ata.ts run
```

The template calls the Associated Token Account program's ordinary `Create` instruction, which
fails if the account already exists, and guards the call with `isEmpty` so it runs only when the
account has no data yet. On devnet, the first run created the account. A second run succeeded
without calling the Associated Token Account program, which shows that Ballista skipped a call
that would otherwise have failed.

## Upload a revision

A finalized template cannot be changed or closed, so each new version of a template needs a new
template ID. `findFreeTemplateId` returns the lowest ID that is still free for a creator:

```ts
import { findFreeTemplateId } from '@jac0xb/ballista/kit';

const { templateId, templateAddress } = await findFreeTemplateId({ rpc, creator: payer.address });
```

To upload the example under a new ID:

```bash
BALLISTA_KEYPAIR=/path/to/keypair.json \
BALLISTA_TEMPLATE_ID=23001 \
pnpm --dir clients/js exec tsx examples/ensure-usdc-ata.ts upload
```

The example uploads to the pre-release program, so until the final program is deployed this
upload is rejected with `InvalidTemplateProgram`, as described above.

A template's address is a [PDA](/reference/glossary#pda) whose seeds are the word `template`, the
creator, and the template ID, so anyone can work out the address in advance and send
[lamports](/reference/glossary#lamports) to it. That does not block the ID. If the address already
holds lamports when the template is created, the program tops the account up to the
[rent-exempt](/reference/glossary#rent) minimum if needed, then allocates the account and assigns
it to itself, signing for the template address.

## Inspect a template

`decodeTemplateAccount` and `inspectTemplate` in TypeScript, or `TemplateAccount::parse` in Rust,
read a stored template. See [Inspecting a template](/guide/inspecting-templates).

## Read a failure

When a run fails, simulate the transaction and take the custom error code from the result.
`explainRunError` turns the code into a message that names the failing step, account, or input:

```ts
explainRunError(code, compiled)?.message;
// 'AccountConstraintFailed: account usdcMint does not satisfy its constraint'
```

`compiled` is the compiled template the SDK produced when you authored it. The program also logs
the location of the failure as one line of five numbers, written with `sol_log_64`.
