/**
 * Fit a run with many accounts into a version 0 transaction by looking its accounts up in address
 * lookup tables.
 *
 * The TypeScript reference includes the region below. `sdk-examples.test.ts` builds a 30-row
 * payroll that is too large for a version 0 transaction until its accounts come from a table.
 */
// #region lookup-tables
import {
  appendTransactionMessageInstruction,
  compressTransactionMessageUsingAddressLookupTables,
  createTransactionMessage,
  fetchAddressesForLookupTables,
  pipe,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  type Address,
  type BlockhashLifetimeConstraint,
  type GetMultipleAccountsApi,
  type Instruction,
  type Rpc,
  type TransactionSigner,
} from '@solana/kit';

/**
 * A version 0 message holding `run`, with each account the `tables` hold looked up in them. A
 * looked-up account takes 1 byte of the transaction instead of 32. Signers and the program the
 * transaction calls stay in the message.
 */
export async function runWithLookupTables(input: {
  rpc: Rpc<GetMultipleAccountsApi>;
  feePayer: TransactionSigner;
  latestBlockhash: BlockhashLifetimeConstraint;
  run: Instruction;
  tables: Address[];
}) {
  const message = pipe(
    createTransactionMessage({ version: 0 }),
    (m) => setTransactionMessageFeePayerSigner(input.feePayer, m),
    (m) => setTransactionMessageLifetimeUsingBlockhash(input.latestBlockhash, m),
    (m) => appendTransactionMessageInstruction(input.run, m),
  );
  const addressesByTable = await fetchAddressesForLookupTables(input.tables, input.rpc);
  return compressTransactionMessageUsingAddressLookupTables(message, addressesByTable);
}
// #endregion lookup-tables
