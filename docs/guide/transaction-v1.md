# Transaction v1

Transaction v1 raises the serialized transaction limit from 1,232 to 4,096 bytes. Ballista benefits
because callers can pass larger inline account sets to one atomic `Run`; the stored template bytes
remain in the template account and are not repeated in the transaction.

## Required resource config

V1 resource defaults are zero. Set compute-unit and loaded-account-data limits in the message
config; Compute Budget instructions do not set them for v1.

::: code-group

```ts [TypeScript · Solana Kit]
const message = pipe(
  createTransactionMessage({ version: 1 }),
  (m) => setTransactionMessageFeePayerSigner(payer, m),
  (m) => setTransactionMessageLifetimeUsingBlockhash(blockhash, m),
  (m) => appendTransactionMessageInstruction(runInstruction, m),
);

const resources = createComputeUnitProvider({ rpc, marginBps: 1_000 });
const { transactionMessage, estimate } = await resources.estimateAndSet(message);

console.log(estimate.computeUnitLimit);
console.log(estimate.loadedAccountsDataSizeLimit);
```

```rust [Rust · Solana 4.2]
use solana_message::v1::{Message, TransactionConfig};

let config = TransactionConfig::empty()
    .with_compute_unit_limit(measured_cu)
    .with_loaded_accounts_data_size_limit(measured_loaded_bytes)
    .with_priority_fee(priority_fee_lamports);

let message = Message::try_compile_with_config(
    &payer,
    &[run_instruction],
    &[],
    recent_blockhash,
    config,
)?;
```

:::

The Ballista provider simulates once with maximum provisory limits, adds a configurable CU margin,
and rounds loaded-account bytes up to a 32 KiB page.

## Practical ceilings

- V1 permits 64 inline addresses and does not use address lookup tables.
- Ballista accepts at most 60 runtime account slots, leaving room for its program and template.
- A 30-recipient Ballista SOL batch currently measures 1,240 bytes.
- Compute and account locks will usually bind before the 4,096-byte packet ceiling.
- Send and simulate large transactions using base64 encoding.
- RPC readers must use `maxSupportedTransactionVersion: 1` for transactions and blocks.

See Solana's [larger transaction migration guide](https://solana.com/upgrades/larger-transaction-sizes).
