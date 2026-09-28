import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction } from '../../../src/kit.js';
import { compiled } from '../drift-settle-when-profitable.js';
import { DRIFT_V2 } from '../shared.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

export function buildDriftSettleRun(input: {
  templateAddress: Address;
  owner: Address;
  driftState: Address;
  driftUser: Address;
  driftUserStats: Address;
  driftSpotMarketVault: Address;
  driftSigner: Address;
  perpMarket: Address;
  spotMarket: Address;
  destinationAta: Address;
  minimumSettled: bigint;
}): Instruction {
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: { minimumSettled: input.minimumSettled },
    accounts: {
      drift: pinned(DRIFT_V2),
      tokenProgram: pinned(TOKEN_PROGRAM),
      owner: at(input.owner),
      driftState: at(input.driftState),
      driftUser: at(input.driftUser),
      driftUserStats: at(input.driftUserStats),
      driftSpotMarketVault: at(input.driftSpotMarketVault),
      driftSigner: at(input.driftSigner),
      perpMarket: at(input.perpMarket),
      spotMarket: at(input.spotMarket),
      destinationAta: at(input.destinationAta),
    },
  });
}
