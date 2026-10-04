// #region template
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  defineTemplate,
  expression,
  step,
  systemTransfer,
} from '@jac0xb/ballista';

/** Pay the recipient in row `i` (counting from 0) `(i + 1) × base` lamports. */
export const indexWeightedRewards = defineTemplate({
  inputs: { base: { type: 'u64' } },
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
        lamports: expression.multiply(
          expression.add(expression.loopIndex(), expression.u64(1)),
          expression.input('base'),
        ),
      }),
    ]),
  ],
});
// #endregion template

// #region run
import type { Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '@jac0xb/ballista/kit';

/** The order of `recipients` sets each one's multiple of `base`. */
export function runIndexWeightedRewards(run: {
  templateAddress: Address;
  treasury: Address;
  recipients: readonly Address[];
  base: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(indexWeightedRewards),
    templateAddress: run.templateAddress,
    inputs: { base: run.base },
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      treasury: { address: run.treasury },
    },
    batchRows: run.recipients.map((recipient) => ({ recipient: { address: recipient } })),
  });
}
// #endregion run
