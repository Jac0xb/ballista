# Devnet workflow

The Ballista program is deployed at
[`BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD`](https://explorer.solana.com/address/BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD?cluster=devnet).

::: warning Current deployment
The current binary predates the `derivePda` opcode. Templates containing `assertPda` or `assertAta`
need the next program upgrade and IDL refresh. Templates using existing instructions—including
`let` and `snapshot`, which compile to ordinary registers—remain compatible.
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

```bash
BALLISTA_KEYPAIR=/path/to/keypair.json \
BALLISTA_TEMPLATE_ID=23001 \
pnpm --dir clients/js exec tsx examples/ensure-usdc-ata.ts upload
```

Use a new template ID for every revision. Finalized templates are immutable and cannot be closed.

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
