// #region template
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  assertPda,
  data,
  defineTemplate,
  expression,
  step,
} from '@jac0xb/ballista';

// Stand-ins so the example runs as written: replace them with your protocol's address and the
// instruction to call on the position.
const PROTOCOL_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const INSTRUCTION_DISCRIMINATOR = Uint8Array.of(2, 0, 0, 0);
const INSTRUCTION_ARGUMENT = 10_000n;

/** Require `position` to be the PDA the protocol derives from ("position", owner, positionId). */
export const canonicalPositionAccount = defineTemplate({
  inputs: { positionId: { type: 'bytes', maxLength: 8 } },
  accounts: {
    protocolProgram: { executable: true, address: PROTOCOL_PROGRAM },
    owner: { signer: true, writable: true },
    position: { writable: true, owner: PROTOCOL_PROGRAM, minDataLength: 128 },
  },
  steps: [
    assertPda({
      account: account.fixed('position'),
      program: account.fixed('protocolProgram'),
      seeds: [
        expression.bytes(new TextEncoder().encode('position')),
        expression.accountField(account.fixed('owner'), 'key'),
        expression.input('positionId'),
      ],
    }),
    step.invoke({
      program: account.fixed('protocolProgram'),
      accounts: [
        { account: account.fixed('owner'), signer: true, writable: true },
        { account: account.fixed('position'), signer: false, writable: true },
      ],
      data: [data.literal(INSTRUCTION_DISCRIMINATOR), data.encode('u64', expression.u64(INSTRUCTION_ARGUMENT))],
    }),
  ],
});
// #endregion template

// #region run
import { address, getAddressEncoder, getProgramDerivedAddress, type Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { buildKitRunInstruction } from '@jac0xb/ballista/kit';

const PROTOCOL_PROGRAM_ADDRESS = address('11111111111111111111111111111111'); // the same stand-in

/** Derives the position the same way the template checks it. */
export async function runCanonicalPositionAccount(run: {
  templateAddress: Address;
  owner: Address;
  positionId: bigint;
}) {
  const positionId = new Uint8Array(8);
  new DataView(positionId.buffer).setBigUint64(0, run.positionId, true);
  const [position] = await getProgramDerivedAddress({
    programAddress: PROTOCOL_PROGRAM_ADDRESS,
    seeds: ['position', getAddressEncoder().encode(run.owner), positionId],
  });
  return buildKitRunInstruction({
    compiled: compileTemplate(canonicalPositionAccount),
    templateAddress: run.templateAddress,
    inputs: { positionId }, // a bytes input
    accounts: {
      protocolProgram: { address: PROTOCOL_PROGRAM_ADDRESS },
      owner: { address: run.owner },
      position: { address: position },
    },
  });
}
// #endregion run
