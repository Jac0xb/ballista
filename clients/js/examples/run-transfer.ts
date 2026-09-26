/**
 * Run a template from TypeScript: bind inputs and accounts, build the Kit instruction, and explain
 * a failure code with the compiled template's source map.
 *
 * No RPC connection is needed to build the instruction. Set `SOLANA_RPC_URL`, `SOLANA_WS_URL`, and
 * `BALLISTA_KEYPAIR` to actually send it; see `ensure-usdc-ata.ts` for a complete devnet flow.
 */
import { address, getAddressDecoder } from '@solana/kit';

import { explainRunError } from '../src/index.js';
import { BALLISTA_ADDRESS, SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction, getTemplateAddress } from '../src/kit.js';
import { compiledTransfer } from './transfer.js';

const creator = address('11111111111111111111111111111111');
const decoder = getAddressDecoder();
const sender = decoder.decode(new Uint8Array(32).fill(2));
const recipient = decoder.decode(new Uint8Array(32).fill(3));

export async function buildTransferRun(lamports: bigint) {
  const [templateAddress] = await getTemplateAddress(creator, 0);
  return buildKitRunInstruction({
    compiled: compiledTransfer,
    programAddress: BALLISTA_ADDRESS,
    templateAddress,
    inputs: { lamports },
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      sender: { address: sender },
      recipient: { address: recipient },
    },
  });
}

/** Turn a custom error code from a failed run into the step that raised it. */
export function describeFailure(code: number): string {
  const explanation = explainRunError(code, compiledTransfer);
  return explanation ? explanation.message : `code ${code} came from an invoked program`;
}

if (process.argv[1]?.endsWith('run-transfer.ts')) {
  const instruction = await buildTransferRun(55_000n);
  console.log(`program: ${instruction.programAddress}`);
  console.log(`accounts: ${instruction.accounts?.length ?? 0}`);
  console.log(`data: ${Buffer.from(instruction.data ?? new Uint8Array()).toString('hex')}`);
  // A failed require at instruction 1 would surface as RequirementFailed with the program counter
  // in the high 16 bits; the source map turns that into the labelled step.
  console.log(describeFailure((1 << 16) | 6015));
  console.log(describeFailure(1));
}
