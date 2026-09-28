// #region template
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  defineTemplate,
  expression,
  step,
  systemTransfer,
} from '../../src/index.js';

/** Pay `amount` lamports from the treasury to each of 1 to 30 recipients. */
export const boundedSolPayroll = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    treasury: { signer: true, writable: true },
  },
  batch: { maxIterations: 30, minIterations: 1, row: { recipient: { writable: true } } },
  steps: [
    step.forEach([
      systemTransfer({
        systemProgram: account.fixed('systemProgram'),
        from: account.fixed('treasury'),
        to: account.iteration('recipient'),
        lamports: expression.input('amount'),
      }),
    ]),
  ],
});
// #endregion template

// #region run
import type { Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '../../src/kit.js';

/** Throws before sending if there are more than 30 recipients or none. */
export function runBoundedSolPayroll(run: {
  templateAddress: Address;
  treasury: Address;
  recipients: readonly Address[];
  amount: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(boundedSolPayroll),
    templateAddress: run.templateAddress,
    inputs: { amount: run.amount },
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      treasury: { address: run.treasury },
    },
    batchRows: run.recipients.map((recipient) => ({ recipient: { address: recipient } })),
  });
}
// #endregion run
