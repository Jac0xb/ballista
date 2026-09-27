// #region template
import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  defineTemplate,
  expression,
  step,
  tokenTransfer,
} from '../../src/index.js';

/** Pay `amount` tokens from one source to each of 1 to 32 existing token accounts. */
export const existingAccountTokenPayroll = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  accounts: {
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    source: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 },
    authority: { signer: true },
  },
  batch: {
    maxIterations: 32,
    minIterations: 1,
    row: { destination: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 } },
  },
  steps: [
    step.forEach([
      tokenTransfer({
        tokenProgram: account.fixed('tokenProgram'),
        source: account.fixed('source'),
        destination: account.iteration('destination'),
        authority: account.fixed('authority'),
        amount: expression.input('amount'),
      }),
    ]),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { buildKitRunInstruction } from '../../src/kit.js';

const TOKEN_PROGRAM = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');

export function runExistingAccountTokenPayroll(run: {
  templateAddress: Address;
  source: Address;
  authority: Address;
  destinations: readonly Address[];
  amount: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(existingAccountTokenPayroll),
    templateAddress: run.templateAddress,
    inputs: { amount: run.amount },
    accounts: {
      tokenProgram: { address: TOKEN_PROGRAM },
      source: { address: run.source },
      authority: { address: run.authority },
    },
    batchRows: run.destinations.map((destination) => ({ destination: { address: destination } })),
  });
}
// #endregion run
