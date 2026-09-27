// #region template
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  defineTemplate,
  expression,
  step,
  systemTransfer,
} from '../../src/index.js';

/** Pay each recipient its own amount, carried as a row input. */
export const rowAmounts = defineTemplate({
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    treasury: { signer: true, writable: true },
  },
  batch: {
    maxIterations: 30,
    minIterations: 1,
    row: { recipient: { writable: true } },
    rowInputs: { amount: { type: 'u64' } },
  },
  steps: [
    step.forEach([
      systemTransfer({
        systemProgram: account.fixed('systemProgram'),
        from: account.fixed('treasury'),
        to: account.iteration('recipient'),
        lamports: expression.rowInput('amount'),
      }),
    ]),
  ],
});
// #endregion template

// #region run
import type { Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '../../src/kit.js';

/** `payees` with each one's amount in lamports. */
export function runRowAmounts(run: {
  templateAddress: Address;
  treasury: Address;
  payees: readonly { address: Address; lamports: bigint }[];
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(rowAmounts),
    templateAddress: run.templateAddress,
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      treasury: { address: run.treasury },
    },
    // One row of accounts and one row of inputs per payee, in the same order.
    batchRows: run.payees.map((payee) => ({ recipient: { address: payee.address } })),
    batchInputs: run.payees.map((payee) => ({ amount: payee.lamports })),
  });
}
// #endregion run
