/**
 * Run a template whose signer is not the fee payer: a relayer pays for the run, and the vault's
 * owner signs the sweep the template makes from it.
 *
 * The TypeScript reference includes the region below. `sdk-examples.test.ts` signs the result
 * with two generated keys.
 */
// #region second-signer
import {
  appendTransactionMessageInstruction,
  createTransactionMessage,
  pipe,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signTransactionMessageWithSigners,
  type Address,
  type BlockhashLifetimeConstraint,
  type TransactionSigner,
} from '@solana/kit';

import type { CompiledTemplate } from '../../src/index.js';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '../../src/kit.js';

/**
 * `compiled` is the sweep from Getting started, whose `vault` must sign. The vault's binding
 * carries its signer, so signing the message signs with it as well as with the fee payer.
 */
export async function relayedSweep(input: {
  compiled: CompiledTemplate;
  templateAddress: Address;
  relayer: TransactionSigner;
  vault: TransactionSigner;
  destination: Address;
  reserve: bigint;
  latestBlockhash: BlockhashLifetimeConstraint;
}) {
  const run = buildKitRunInstruction({
    compiled: input.compiled,
    templateAddress: input.templateAddress,
    inputs: { reserve: input.reserve },
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      vault: { address: input.vault.address, signer: input.vault },
      destination: { address: input.destination },
    },
  });
  const message = pipe(
    createTransactionMessage({ version: 0 }),
    (m) => setTransactionMessageFeePayerSigner(input.relayer, m),
    (m) => setTransactionMessageLifetimeUsingBlockhash(input.latestBlockhash, m),
    (m) => appendTransactionMessageInstruction(run, m),
  );
  return signTransactionMessageWithSigners(message);
}
// #endregion second-signer
