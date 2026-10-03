/**
 * Transaction v1 (`docs/guide/transaction-v1.md`): run a template in a version 1 transaction, with
 * its compute-unit and loaded-account-data limits set from a simulation. The region continues
 * Getting started, so it uses `rpc`, `sendAndConfirm` and the Solana Kit functions `connect.ts`
 * imports, and imports only what that file doesn't. It is type-checked, not run: `payer` and
 * `runInstruction` stand for a signer and a run instruction, such as Getting started's `vault` and
 * `sweepInstruction(...)`.
 */
import {
  assertIsTransactionWithBlockhashLifetime,
  createSolanaRpcSubscriptions,
  createTransactionMessage,
  pipe,
  sendAndConfirmTransactionFactory,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signTransactionMessageWithSigners,
  type Instruction,
  type TransactionSigner,
} from '@solana/kit';

import { rpc } from './connect.js';

// As `connect.ts` builds it.
const sendAndConfirm = sendAndConfirmTransactionFactory({
  rpc,
  rpcSubscriptions: createSolanaRpcSubscriptions('ws://127.0.0.1:8900'),
});
declare const payer: TransactionSigner;
declare const runInstruction: Instruction;

// #region v1
import { appendTransactionMessageInstruction } from '@solana/kit';
import { createComputeUnitProvider } from '../../src/kit.js';

const { value: blockhash } = await rpc.getLatestBlockhash().send();
const message = pipe(
  createTransactionMessage({ version: 1 }),
  (m) => setTransactionMessageFeePayerSigner(payer, m),
  (m) => setTransactionMessageLifetimeUsingBlockhash(blockhash, m),
  (m) => appendTransactionMessageInstruction(runInstruction, m),
);

// Simulate once with both limits at their maximums, then set them from the result.
const resources = createComputeUnitProvider({ rpc, marginBps: 1_000 });
const { transactionMessage, estimate } = await resources.estimateAndSet(message);
console.log(estimate.computeUnitLimit, estimate.loadedAccountsDataSizeLimit);

const transaction = await signTransactionMessageWithSigners(transactionMessage);
assertIsTransactionWithBlockhashLifetime(transaction);
await sendAndConfirm(transaction, { commitment: 'confirmed' });
// #endregion v1
