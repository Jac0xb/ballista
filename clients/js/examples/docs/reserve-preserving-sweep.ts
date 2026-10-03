// #region template
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  defineTemplate,
  expression,
  step,
  systemTransfer,
} from '@jac0xb/ballista';

/** Move up to `cap` lamports to the vault without taking the payer below `reserve`. */
export const reservePreservingSweep = defineTemplate({
  inputs: { reserve: { type: 'u64' }, cap: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    payer: { signer: true, writable: true },
    vault: { writable: true },
  },
  steps: [
    step.snapshot('before', expression.accountField(account.fixed('payer'), 'lamports')),
    step.require(expression.greaterThanOrEqual(expression.snapshot('before'), expression.input('reserve'))),
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('payer'),
      to: account.fixed('vault'),
      lamports: expression.min(
        expression.subtract(expression.snapshot('before'), expression.input('reserve')),
        expression.input('cap'),
      ),
    }),
    step.require(
      expression.greaterThanOrEqual(
        expression.accountField(account.fixed('payer'), 'lamports'),
        expression.input('reserve'),
      ),
    ),
  ],
});
// #endregion template

// #region run
import type { Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '@jac0xb/ballista/kit';

export function runReservePreservingSweep(run: {
  templateAddress: Address;
  payer: Address;
  vault: Address;
  reserve: bigint;
  cap: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(reservePreservingSweep),
    templateAddress: run.templateAddress,
    inputs: { reserve: run.reserve, cap: run.cap },
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      payer: { address: run.payer },
      vault: { address: run.vault },
    },
  });
}
// #endregion run
