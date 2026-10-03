// #region template
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  defineTemplate,
  expression,
  step,
  systemTransfer,
} from '@jac0xb/ballista';

/** Transfer `amount` lamports, then require the sender's balance fell by exactly that much. */
export const exactLamportDelta = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    sender: { signer: true, writable: true },
    recipient: { writable: true },
  },
  steps: [
    // A run accepts one account in both slots, so require two different accounts.
    step.require(
      expression.notEqual(expression.accountKey('sender'), expression.accountKey('recipient')),
      'distinctAccounts',
    ),
    step.snapshot('before', expression.accountField(account.fixed('sender'), 'lamports')),
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('sender'),
      to: account.fixed('recipient'),
      lamports: expression.input('amount'),
    }),
    step.require(
      expression.equal(
        expression.accountField(account.fixed('sender'), 'lamports'),
        expression.subtract(expression.snapshot('before'), expression.input('amount')),
      ),
    ),
  ],
});
// #endregion template

// #region run
import type { Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '@jac0xb/ballista/kit';

/** If the balance does not fall by exactly `amount`, the run fails and the transfer is undone. */
export function runExactLamportDelta(run: {
  templateAddress: Address;
  sender: Address;
  recipient: Address;
  amount: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(exactLamportDelta),
    templateAddress: run.templateAddress,
    inputs: { amount: run.amount },
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      sender: { address: run.sender },
      recipient: { address: run.recipient },
    },
  });
}
// #endregion run
