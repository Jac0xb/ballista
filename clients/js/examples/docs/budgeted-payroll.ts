// #region template
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  defineTemplate,
  expression,
  step,
  systemTransfer,
} from '@jac0xb/ballista';

/** Pay `amount` to every recipient, then require the total stays within `budget`. */
export const budgetedPayroll = defineTemplate({
  inputs: { amount: { type: 'u64' }, budget: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    treasury: { signer: true, writable: true },
  },
  batch: { maxIterations: 30, minIterations: 1, row: { recipient: { writable: true } } },
  steps: [
    step.let('total', expression.u64(0)),
    step.forEach(
      [
        systemTransfer({
          systemProgram: account.fixed('systemProgram'),
          from: account.fixed('treasury'),
          to: account.iteration('recipient'),
          lamports: expression.input('amount'),
        }),
        step.assign('total', expression.add(expression.variable('total'), expression.input('amount'))),
      ],
      { carry: ['total'] },
    ),
    step.require(expression.lessThanOrEqual(expression.variable('total'), expression.input('budget')), 'withinBudget'),
  ],
});
// #endregion template

// #region run
import type { Address } from '@solana/kit';

import { compileTemplate, explainRunError } from '@jac0xb/ballista';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '@jac0xb/ballista/kit';

const compiled = compileTemplate(budgetedPayroll);

export function runBudgetedPayroll(run: {
  templateAddress: Address;
  treasury: Address;
  recipients: readonly Address[];
  amount: bigint;
  budget: bigint;
}) {
  return buildKitRunInstruction({
    compiled,
    templateAddress: run.templateAddress,
    inputs: { amount: run.amount, budget: run.budget },
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      treasury: { address: run.treasury },
    },
    batchRows: run.recipients.map((recipient) => ({ recipient: { address: recipient } })),
  });
}

/** Going over budget fails the labelled require: 'RequirementFailed at steps[2] (withinBudget)'. */
export function explainBudgetFailure(code: number) {
  return explainRunError(code, compiled)?.message;
}
// #endregion run
