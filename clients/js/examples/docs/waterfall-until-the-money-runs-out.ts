// #region template
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  defineTemplate,
  expression,
  step,
  systemTransfer,
} from '@jac0xb/ballista';

/** Pay creditors in row order, each the smaller of what it is owed and what is left. */
export const waterfallUntilTheMoneyRunsOut = defineTemplate({
  inputs: { reserve: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    treasury: { signer: true, writable: true },
  },
  batch: {
    maxIterations: 8,
    minIterations: 1,
    row: { creditor: { writable: true } },
    rowInputs: { owed: { type: 'u64' } },
  },
  steps: [
    step.let(
      'remaining',
      expression.subtract(expression.accountField(account.fixed('treasury'), 'lamports'), expression.input('reserve')),
    ),
    step.forEach(
      [
        step.let('pay', expression.min(expression.variable('remaining'), expression.rowInput('owed'))),
        systemTransfer({
          systemProgram: account.fixed('systemProgram'),
          from: account.fixed('treasury'),
          to: account.iteration('creditor'),
          lamports: expression.variable('pay'),
          when: expression.greaterThan(expression.variable('pay'), expression.u64(0)),
        }),
        step.assign('remaining', expression.subtract(expression.variable('remaining'), expression.variable('pay'))),
      ],
      { carry: ['remaining'] },
    ),
  ],
});
// #endregion template

// #region run
import type { Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '@jac0xb/ballista/kit';

/** `creditors` in priority order, each with what it is owed. */
export function runWaterfallUntilTheMoneyRunsOut(run: {
  templateAddress: Address;
  treasury: Address;
  reserve: bigint;
  creditors: readonly { address: Address; owed: bigint }[];
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(waterfallUntilTheMoneyRunsOut),
    templateAddress: run.templateAddress,
    inputs: { reserve: run.reserve },
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      treasury: { address: run.treasury },
    },
    // One row per creditor, and one set of row inputs per row, in the same order.
    batchRows: run.creditors.map((creditor) => ({ creditor: { address: creditor.address } })),
    batchInputs: run.creditors.map((creditor) => ({ owed: creditor.owed })),
  });
}
// #endregion run
