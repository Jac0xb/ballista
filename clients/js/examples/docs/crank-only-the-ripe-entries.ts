// #region template
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, data, defineTemplate, expression, step } from '../../src/index.js';

// Stand-ins so the example runs as written: replace them with the queue program's address, its
// settle instruction data, and the offset of the deadline in its entry account.
const QUEUE_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const SETTLE_DISCRIMINATOR = Uint8Array.of(2, 0, 0, 0);
const SETTLE_ARGUMENT = 10_000n;
const DEADLINE_OFFSET = 8;

/** Settle each queue entry whose deadline has passed; skip the rest. */
export const crankOnlyTheRipeEntries = defineTemplate({
  accounts: {
    queueProgram: { executable: true, address: QUEUE_PROGRAM },
    keeper: { signer: true, writable: true },
  },
  batch: {
    maxIterations: 8,
    minIterations: 1,
    row: { entry: { writable: true, owner: QUEUE_PROGRAM, minDataLength: 128 } },
  },
  steps: [
    step.forEach([
      step.invoke({
        program: account.fixed('queueProgram'),
        accounts: [
          { account: account.fixed('keeper'), signer: true, writable: true },
          { account: account.iteration('entry'), signer: false, writable: true },
        ],
        data: [data.literal(SETTLE_DISCRIMINATOR), data.encode('u64', expression.u64(SETTLE_ARGUMENT))],
        when: expression.lessThanOrEqual(
          expression.accountData(account.iteration('entry'), DEADLINE_OFFSET, 'i64'),
          expression.clockUnixTimestamp(),
        ),
      }),
    ]),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { buildKitRunInstruction } from '../../src/kit.js';

const QUEUE_PROGRAM_ADDRESS = address('11111111111111111111111111111111'); // the same stand-in

/** Pass the whole queue; the run settles the entries that are due when it executes. */
export function runCrankOnlyTheRipeEntries(run: {
  templateAddress: Address;
  keeper: Address;
  entries: readonly Address[];
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(crankOnlyTheRipeEntries),
    templateAddress: run.templateAddress,
    accounts: {
      queueProgram: { address: QUEUE_PROGRAM_ADDRESS },
      keeper: { address: run.keeper },
    },
    batchRows: run.entries.map((entry) => ({ entry: { address: entry } })),
  });
}
// #endregion run
