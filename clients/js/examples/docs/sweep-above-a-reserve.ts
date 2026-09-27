// #region template
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  defineTemplate,
  expression,
  step,
  systemTransfer,
} from '../../src/index.js';

/** Move everything above `reserve` from the vault to the destination. */
export const sweepAboveAReserve = defineTemplate({
  inputs: { reserve: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    vault: { signer: true, writable: true },
    destination: { writable: true },
  },
  steps: [
    step.let('balance', expression.accountField(account.fixed('vault'), 'lamports')),
    step.require(expression.greaterThan(expression.variable('balance'), expression.input('reserve'))),
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('vault'),
      to: account.fixed('destination'),
      lamports: expression.subtract(expression.variable('balance'), expression.input('reserve')),
    }),
  ],
});
// #endregion template

// #region run
import type { Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '../../src/kit.js';

/** The caller names the reserve, never the amount. */
export function runSweepAboveAReserve(run: {
  templateAddress: Address;
  vault: Address;
  destination: Address;
  reserve: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(sweepAboveAReserve),
    templateAddress: run.templateAddress,
    inputs: { reserve: run.reserve },
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      vault: { address: run.vault },
      destination: { address: run.destination },
    },
  });
}
// #endregion run
