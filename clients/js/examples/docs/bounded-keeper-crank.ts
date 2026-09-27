// #region template
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, data, defineTemplate, expression, step } from '../../src/index.js';

// Stand-ins so the example runs as written: replace them with the protocol's address and its
// crank instruction data.
const PROTOCOL_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const CRANK_DISCRIMINATOR = Uint8Array.of(2, 0, 0, 0);
const CRANK_ARGUMENT = 1_000n;

/** Call the crank instruction once for each (market, queue) row. */
export const boundedKeeperCrank = defineTemplate({
  accounts: {
    protocolProgram: { executable: true, address: PROTOCOL_PROGRAM },
    keeper: { signer: true, writable: true },
  },
  batch: {
    maxIterations: 24,
    minIterations: 1,
    // Each row is two accounts: a market, then its queue.
    row: { market: { writable: true }, queue: { writable: true } },
  },
  steps: [
    step.forEach([
      step.invoke({
        program: account.fixed('protocolProgram'),
        accounts: [
          { account: account.fixed('keeper'), signer: true, writable: true },
          { account: account.iteration('market'), writable: true, signer: false },
          { account: account.iteration('queue'), writable: true, signer: false },
        ],
        data: [data.literal(CRANK_DISCRIMINATOR), data.encode('u64', expression.u64(CRANK_ARGUMENT))],
      }),
    ]),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { buildKitRunInstruction } from '../../src/kit.js';

const PROTOCOL_PROGRAM_ADDRESS = address('11111111111111111111111111111111'); // the same stand-in

export function runBoundedKeeperCrank(run: {
  templateAddress: Address;
  keeper: Address;
  rows: readonly { market: Address; queue: Address }[];
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(boundedKeeperCrank),
    templateAddress: run.templateAddress,
    accounts: {
      protocolProgram: { address: PROTOCOL_PROGRAM_ADDRESS },
      keeper: { address: run.keeper },
    },
    batchRows: run.rows.map((row) => ({ market: { address: row.market }, queue: { address: row.queue } })),
  });
}
// #endregion run
