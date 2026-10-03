/**
 * Errors and events, "Run events and output": a template that reports back in all three ways. The
 * page shows the `template` region; `simulate-run.ts` holds the code that reads the outputs back.
 */
// #region template
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  data,
  defineTemplate,
  expression,
  step,
  systemTransfer,
} from '@jac0xb/ballista';

/** Sweep everything above `reserve`, and report how much moved. */
export const reportedSweep = defineTemplate({
  emitEvent: true, // log one run event after every successful run
  inputs: { reserve: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    vault: { signer: true, writable: true },
    destination: { writable: true },
  },
  steps: [
    step.let(
      'swept',
      expression.subtract(expression.accountField(account.fixed('vault'), 'lamports'), expression.input('reserve')),
    ),
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('vault'),
      to: account.fixed('destination'),
      lamports: expression.variable('swept'),
    }),
    // A Program data: line of its own, starting with a 4-byte tag.
    step.emit([data.literal(new TextEncoder().encode('SWP1')), data.encode('u64', expression.variable('swept'))]),
    // The bytes the caller gets back.
    step.setReturnData([data.encode('u64', expression.variable('swept'))]),
  ],
});
// #endregion template
