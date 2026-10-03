# Transaction v1

Solana's version 1 transaction format (v1) raises the maximum size of a transaction from 1,232
bytes to 4,096 bytes. Here is how to build one that runs a Ballista template.

A template's steps are stored on chain in its template account, so a `Run` instruction carries
only the template account, the run's accounts, and its inputs. Each account adds its 32-byte
address to the transaction, so runs with many accounts gain the most from v1: more of them fit in
one transaction, which still succeeds or fails as a whole.

## Set the resource limits

A v1 transaction must declare two limits in its message: the
[compute-unit](/reference/glossary#compute-units) limit and the loaded-account-data limit (how many
bytes of account data it may load). Both default to zero. Compute Budget instructions, which set
these limits for older transaction versions, do not set them for v1.

The code continues [Getting started](/guide/getting-started): `payer` signs, and `runInstruction`
(`run` in Rust) is a run instruction such as its sweep. In Rust, v1 messages need
`solana-message` 4.2 or later, which Getting started's install line adds.

::: code-group

<<< @/../clients/js/examples/start/transaction-v1.ts#v1 [TypeScript]

<<< @/../clients/rust/examples/docs_start.rs#v1 [Rust]

:::

In TypeScript, `createComputeUnitProvider` fills in both limits. It simulates the transaction once
with both limits at their maximums. It then sets the compute-unit limit to the measured amount plus
a margin (`marginBps` is in basis points, so `1_000` adds 10%), and rounds the measured account
data up to the next multiple of 32 KiB. In Rust, measure both values by simulating the transaction,
then set them in the message's `TransactionConfig`.

## Limits

A transaction uses at most 64 accounts in any version, lookup tables included, so a run fits about
61 runtime accounts. A v1 transaction lists them all without a table. See
[accounts per transaction](/reference/limits#accounts-per-transaction).

- A run of a 30-recipient SOL payroll template measures 1,240 bytes as a v1 transaction.
- Encode large transactions as base64 when you send or simulate them.
- When you fetch transactions or blocks over RPC, pass `maxSupportedTransactionVersion: 1`.

See Solana's [larger transaction migration guide](https://solana.com/upgrades/larger-transaction-sizes).
