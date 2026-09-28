// #region template
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, data, defineTemplate, expression, step } from '../../src/index.js';

// Stand-ins so the example runs as written: replace them with your protocol's address and the
// instruction's discriminator.
const PROTOCOL_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const INSTRUCTION_DISCRIMINATOR = Uint8Array.of(2, 0, 0, 0);

/** Call one program, with the program address and the position's owner pinned in the schema. */
export const pinnedProgramAndOwner = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  accounts: {
    protocolProgram: { executable: true, address: PROTOCOL_PROGRAM },
    payer: { signer: true, writable: true },
    position: { writable: true, owner: PROTOCOL_PROGRAM, minDataLength: 128 },
  },
  steps: [
    step.invoke({
      program: account.fixed('protocolProgram'),
      accounts: [
        { account: account.fixed('payer'), signer: true, writable: true },
        { account: account.fixed('position'), signer: false, writable: true },
      ],
      data: [data.literal(INSTRUCTION_DISCRIMINATOR), data.encode('u64', expression.input('amount'))],
    }),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { buildKitRunInstruction } from '../../src/kit.js';

const PROTOCOL_PROGRAM_ADDRESS = address('11111111111111111111111111111111'); // the same stand-in

/** A different program, or a position with another owner, fails the run before its first step. */
export function runPinnedProgramAndOwner(run: {
  templateAddress: Address;
  payer: Address;
  position: Address;
  amount: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(pinnedProgramAndOwner),
    templateAddress: run.templateAddress,
    inputs: { amount: run.amount },
    accounts: {
      protocolProgram: { address: PROTOCOL_PROGRAM_ADDRESS },
      payer: { address: run.payer },
      position: { address: run.position },
    },
  });
}
// #endregion run
