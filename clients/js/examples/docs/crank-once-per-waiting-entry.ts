// #region template
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, data, defineTemplate, expression, step } from '@jac0xb/ballista';

// Stand-ins so the example runs as written: replace them with the queue program's address, its
// crank instruction data, and the offset of the waiting count in its queue account.
const QUEUE_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const CRANK_DATA = Uint8Array.of(2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0);
const WAITING_OFFSET = 8;

/** Crank the queue once for each waiting entry, at most eight times. */
export const crankOncePerWaitingEntry = defineTemplate({
  accounts: {
    queueProgram: { executable: true, address: QUEUE_PROGRAM },
    keeper: { signer: true, writable: true },
    queue: { writable: true, owner: QUEUE_PROGRAM },
  },
  steps: [
    step.repeat(
      expression.min(expression.accountData(account.fixed('queue'), WAITING_OFFSET, 'u64'), expression.u64(8)),
      [
        step.invoke({
          program: account.fixed('queueProgram'),
          accounts: [
            { account: account.fixed('keeper'), signer: true, writable: true },
            { account: account.fixed('queue'), signer: false, writable: true },
          ],
          data: [data.literal(CRANK_DATA)],
        }),
      ],
      { max: 8 },
    ),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { buildKitRunInstruction } from '@jac0xb/ballista/kit';

const QUEUE_PROGRAM_ADDRESS = address('11111111111111111111111111111111'); // the same stand-in

/** The run reads the count itself, so the keeper passes only the accounts. */
export function runCrankOncePerWaitingEntry(run: { templateAddress: Address; keeper: Address; queue: Address }) {
  return buildKitRunInstruction({
    compiled: compileTemplate(crankOncePerWaitingEntry),
    templateAddress: run.templateAddress,
    accounts: {
      queueProgram: { address: QUEUE_PROGRAM_ADDRESS },
      keeper: { address: run.keeper },
      queue: { address: run.queue },
    },
  });
}
// #endregion run
