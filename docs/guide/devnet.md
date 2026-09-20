# Devnet workflow

## Deployment policy

Every version of the Ballista program is deployed immutably under its own address. Nothing can
change what a finalized template means after the fact, and templates are bound to the deployment
that finalized them. Moving to a new bytecode version means deploying a new program and uploading
templates under it.

| Build | Program | Status |
| --- | --- | --- |
| Pre-release | [`BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD`](https://explorer.solana.com/address/BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD?cluster=devnet) | Deployed on devnet; predates the current bytecode and `derivePda` |
| Current | not yet deployed | Run it locally with the Mollusk suite |

::: warning Templates from this repository need the final deployment
The devnet program above rejects them with `UnsupportedVersion`. Pass the final program address to
the SDKs once it is deployed, or run against Mollusk locally with
`pnpm build:program && pnpm test:integration`.
:::

## Run the ATA example

```bash
BALLISTA_KEYPAIR=/path/to/keypair.json \
BALLISTA_TEMPLATE_ADDRESS=5ttfid6DiryiFiPwoQZBQXaojHiXjf9nTJB93oVELvJB \
SOLANA_RPC_URL=https://api.devnet.solana.com \
SOLANA_WS_URL=wss://api.devnet.solana.com \
pnpm --dir clients/js exec tsx examples/ensure-usdc-ata.ts run
```

The stored example uses ordinary, non-idempotent ATA `Create` guarded by `isEmpty`. Its first run
created the account; its second run succeeded without an inner CPI, proving Ballista skipped the
instruction that would otherwise fail.

## Upload a revision

Finalized templates are immutable and cannot be closed, so every revision needs a fresh template
ID. Probe for one instead of guessing:

```ts
import { findFreeTemplateId } from '@jac0xb/ballista/kit';

const { templateId, templateAddress } = await findFreeTemplateId({ rpc, creator: payer.address });
```

```bash
BALLISTA_KEYPAIR=/path/to/keypair.json \
BALLISTA_TEMPLATE_ID=23001 \
pnpm --dir clients/js exec tsx examples/ensure-usdc-ata.ts upload
```

Anyone can send lamports to a future template address. Creation tolerates that: the program tops
the account up to rent exemption if needed and allocates it under the PDA signature, so dust cannot
block an ID.

## Inspect a template

::: code-group

```ts [TypeScript]
const account = decodeTemplateAccount(accountBytes);
const stats = inspectTemplate(account.payload);
console.log(account.creator, account.templateId, account.state, stats);
```

```rust [Rust]
let account = ballista_common::template::TemplateAccount::parse(&account_data)?;
let program = account.finalized_program()?;
let stats = program.verify()?;
println!("{stats:?}");
```

:::

## Read a failure

Simulate the transaction, take the custom error code, and decode it:

```ts
explainRunError(code, compiled)?.message;
// 'AccountConstraintFailed: account usdcMint does not satisfy its constraint'
```

The program also logs the failing program counter, opcode, and operands on one `sol_log_64` line.
