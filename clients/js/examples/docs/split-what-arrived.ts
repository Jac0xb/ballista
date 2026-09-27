// #region template
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  defineTemplate,
  expression,
  step,
  systemTransfer,
} from '../../src/index.js';

/** Pay a partner `shareBps` of the vault's balance above `reserve`, and the treasury the rest. */
export const splitWhatArrived = defineTemplate({
  inputs: { reserve: { type: 'u64' }, shareBps: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    vault: { signer: true, writable: true },
    partner: { writable: true },
    treasury: { writable: true },
  },
  steps: [
    step.let(
      'distributable',
      expression.subtract(expression.accountField(account.fixed('vault'), 'lamports'), expression.input('reserve')),
    ),
    step.let(
      'partnerShare',
      expression.divide(
        expression.multiply(expression.variable('distributable'), expression.input('shareBps')),
        expression.u64(10_000),
      ),
    ),
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('vault'),
      to: account.fixed('partner'),
      lamports: expression.variable('partnerShare'),
    }),
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('vault'),
      to: account.fixed('treasury'),
      lamports: expression.subtract(expression.variable('distributable'), expression.variable('partnerShare')),
    }),
  ],
});
// #endregion template

// #region run
import type { Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '../../src/kit.js';

export function runSplitWhatArrived(run: {
  templateAddress: Address;
  vault: Address;
  partner: Address;
  treasury: Address;
  reserve: bigint;
  shareBps: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(splitWhatArrived),
    templateAddress: run.templateAddress,
    inputs: { reserve: run.reserve, shareBps: run.shareBps },
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      vault: { address: run.vault },
      partner: { address: run.partner },
      treasury: { address: run.treasury },
    },
  });
}
// #endregion run
