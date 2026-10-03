# Transaction v1

Solana's version 1 transaction format (v1) raises the maximum size of a transaction from 1,232
bytes to 4,096 bytes. Here is how to build one that runs a Ballista template.

A template's steps are stored on chain in its template account, so a `Run` instruction carries
only the template account, the run's accounts, and its inputs. Each account adds its 32-byte
address to the transaction, so runs with many accounts gain the most from v1: more of them fit in
one transaction, which still succeeds or fails as a whole.

## Set the resource limits

A v1 transaction must declare two limits in its message: the compute-unit limit (compute units
measure the work a transaction may do) and the loaded-account-data limit (how many bytes of account
data it may load). Both default to zero. Compute Budget instructions, which set these limits for
older transaction versions, do not set them for v1.

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

In TypeScript, `createComputeUnitProvider` fills in both limits. It simulates the transaction once
with both limits at their maximums. It then sets the compute-unit limit to the measured amount plus
a margin (`marginBps` is in basis points, so `1_000` adds 10%), and rounds the measured account
data up to the next multiple of 32 KiB. In Rust, measure both values by simulating the transaction,
then set them in the message's `TransactionConfig`.

## Limits

A v1 transaction lists at most 64 account addresses and cannot use address lookup tables, so a run
fits about 60 runtime accounts. [Limits](/reference/limits#accounts-per-transaction) has the
details, and when a larger run needs a v0 transaction instead.

- A run of a 30-recipient SOL payroll template measures 1,240 bytes as a v1 transaction.
- Encode large transactions as base64 when you send or simulate them.
- When you fetch transactions or blocks over RPC, pass `maxSupportedTransactionVersion: 1`.

See Solana's [larger transaction migration guide](https://solana.com/upgrades/larger-transaction-sizes).
