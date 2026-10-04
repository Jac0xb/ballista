// #region template
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, defineTemplate, expression, systemTransfer } from '@jac0xb/ballista';

/** Refund the customer only if the run executes at or before `deadline`. */
export const deadlineRefund = defineTemplate({
  inputs: { refundAmount: { type: 'u64' }, deadline: { type: 'i64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    escrowAuthority: { signer: true, writable: true },
    customer: { writable: true },
  },
  steps: [
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('escrowAuthority'),
      to: account.fixed('customer'),
      lamports: expression.input('refundAmount'),
      when: expression.lessThanOrEqual(expression.clockUnixTimestamp(), expression.input('deadline')),
    }),
  ],
});
// #endregion template

// #region run
import type { Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '@jac0xb/ballista/kit';

export function runDeadlineRefund(run: {
  templateAddress: Address;
  escrowAuthority: Address;
  customer: Address;
  refundAmount: bigint;
  /** Unix timestamp, in seconds. */
  deadline: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(deadlineRefund),
    templateAddress: run.templateAddress,
    inputs: { refundAmount: run.refundAmount, deadline: run.deadline },
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      escrowAuthority: { address: run.escrowAuthority },
      customer: { address: run.customer },
    },
  });
}
// #endregion run
