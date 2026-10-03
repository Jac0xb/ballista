// #region template
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  defineTemplate,
  expression,
  step,
  systemTransfer,
} from '@jac0xb/ballista';

/** Pay each holder `weightBps` of the vault's balance above `reserve`. */
export const distributeARuntimePotProRata = defineTemplate({
  inputs: { reserve: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    vault: { signer: true, writable: true },
  },
  batch: {
    maxIterations: 8,
    minIterations: 1,
    row: { holder: { writable: true } },
    rowInputs: { weightBps: { type: 'u64' } },
  },
  steps: [
    step.let(
      'pot',
      expression.subtract(expression.accountField(account.fixed('vault'), 'lamports'), expression.input('reserve')),
    ),
    step.forEach([
      systemTransfer({
        systemProgram: account.fixed('systemProgram'),
        from: account.fixed('vault'),
        to: account.iteration('holder'),
        lamports: expression.divide(
          expression.multiply(expression.variable('pot'), expression.rowInput('weightBps')),
          expression.u64(10_000),
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

/** `holders` with each one's weight in basis points. */
export function runDistributeARuntimePotProRata(run: {
  templateAddress: Address;
  vault: Address;
  reserve: bigint;
  holders: readonly { address: Address; weightBps: bigint }[];
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(distributeARuntimePotProRata),
    templateAddress: run.templateAddress,
    inputs: { reserve: run.reserve },
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      vault: { address: run.vault },
    },
    batchRows: run.holders.map((holder) => ({ holder: { address: holder.address } })),
    batchInputs: run.holders.map((holder) => ({ weightBps: holder.weightBps })),
  });
}
// #endregion run
