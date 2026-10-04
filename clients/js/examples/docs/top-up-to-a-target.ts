// #region template
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, defineTemplate, expression, step, systemTransfer } from '@jac0xb/ballista';

/** Bring the bot's balance up to `target` lamports; send nothing when it already has that much. */
export const topUpToATarget = defineTemplate({
  inputs: { target: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    funder: { signer: true, writable: true },
    bot: { writable: true },
  },
  steps: [
    step.let('botBalance', expression.accountField(account.fixed('bot'), 'lamports')),
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('funder'),
      to: account.fixed('bot'),
      // target - botBalance. The amount is worked out even when `when` skips the transfer, so the
      // `min` keeps it from going below zero, which would fail the run.
      lamports: expression.subtract(
        expression.input('target'),
        expression.min(expression.variable('botBalance'), expression.input('target')),
      ),
      when: expression.lessThan(expression.variable('botBalance'), expression.input('target')),
    }),
  ],
});
// #endregion template

// #region run
import type { Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '@jac0xb/ballista/kit';

export function runTopUpToATarget(run: { templateAddress: Address; funder: Address; bot: Address; target: bigint }) {
  return buildKitRunInstruction({
    compiled: compileTemplate(topUpToATarget),
    templateAddress: run.templateAddress,
    inputs: { target: run.target },
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      funder: { address: run.funder },
      bot: { address: run.bot },
    },
  });
}
// #endregion run
