import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction } from '../../../src/kit.js';
import { compiled } from '../orca-harvest-many-positions.js';
import { ORCA_WHIRLPOOL } from '../shared.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

export function buildOrcaHarvestRun(input: {
  templateAddress: Address;
  positionAuthority: Address;
  whirlpool: Address;
  tokenOwnerAccountA: Address;
  tokenOwnerAccountB: Address;
  tokenVaultA: Address;
  tokenVaultB: Address;
  /** 1 to 12 positions. The row count comes from this list; there is no count to pass. */
  positions: readonly { position: Address; positionTokenAccount: Address }[];
  dustFloor: bigint;
}): Instruction {
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: { dustFloor: input.dustFloor },
    accounts: {
      whirlpoolProgram: pinned(ORCA_WHIRLPOOL),
      tokenProgram: pinned(TOKEN_PROGRAM),
      positionAuthority: at(input.positionAuthority),
      whirlpool: at(input.whirlpool),
      tokenOwnerAccountA: at(input.tokenOwnerAccountA),
      tokenOwnerAccountB: at(input.tokenOwnerAccountB),
      tokenVaultA: at(input.tokenVaultA),
      tokenVaultB: at(input.tokenVaultB),
    },
    batchRows: input.positions.map((row) => ({
      position: at(row.position),
      positionTokenAccount: at(row.positionTokenAccount),
    })),
  });
}
