// #region template
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  defineTemplate,
  expression,
  step,
  systemTransfer,
} from '../../src/index.js';

/** Split `total` lamports: `partnerBps` of it to the partner, the rest to the treasury. */
export const basisPointRevenueSplit = defineTemplate({
  inputs: { total: { type: 'u64' }, partnerBps: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    source: { signer: true, writable: true },
    partner: { writable: true },
    treasury: { writable: true },
  },
  steps: [
    step.require(expression.lessThanOrEqual(expression.input('partnerBps'), expression.u64(10_000))),
    step.let(
      'partnerAmount',
      expression.divide(
        expression.multiply(expression.input('total'), expression.input('partnerBps')),
        expression.u64(10_000),
      ),
    ),
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('source'),
      to: account.fixed('partner'),
      lamports: expression.variable('partnerAmount'),
    }),
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('source'),
      to: account.fixed('treasury'),
      lamports: expression.subtract(expression.input('total'), expression.variable('partnerAmount')),
    }),
  ],
});
// #endregion template

// #region run
import type { Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '../../src/kit.js';

export function runBasisPointRevenueSplit(run: {
  templateAddress: Address;
  source: Address;
  partner: Address;
  treasury: Address;
  total: bigint;
  partnerBps: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(basisPointRevenueSplit),
    templateAddress: run.templateAddress,
    inputs: { total: run.total, partnerBps: run.partnerBps },
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      source: { address: run.source },
      partner: { address: run.partner },
      treasury: { address: run.treasury },
    },
  });
}
// #endregion run
