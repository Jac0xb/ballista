import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction } from '../../../src/kit.js';
import { compiled } from '../orca-compound-fees.js';
import { ORCA_WHIRLPOOL } from '../shared.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

export function buildOrcaCompoundRun(input: {
  templateAddress: Address;
  positionAuthority: Address;
  whirlpool: Address;
  position: Address;
  positionTokenAccount: Address;
  tokenOwnerAccountA: Address;
  tokenOwnerAccountB: Address;
  tokenVaultA: Address;
  tokenVaultB: Address;
  tickArrayLower: Address;
  tickArrayUpper: Address;
  liquidityAmount: bigint;
  dustFloor: bigint;
}): Instruction {
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: { liquidityAmount: input.liquidityAmount, dustFloor: input.dustFloor },
    accounts: {
      whirlpoolProgram: pinned(ORCA_WHIRLPOOL),
      tokenProgram: pinned(TOKEN_PROGRAM),
      positionAuthority: at(input.positionAuthority),
      whirlpool: at(input.whirlpool),
      position: at(input.position),
      positionTokenAccount: at(input.positionTokenAccount),
      tokenOwnerAccountA: at(input.tokenOwnerAccountA),
      tokenOwnerAccountB: at(input.tokenOwnerAccountB),
      tokenVaultA: at(input.tokenVaultA),
      tokenVaultB: at(input.tokenVaultB),
      tickArrayLower: at(input.tickArrayLower),
      tickArrayUpper: at(input.tickArrayUpper),
    },
  });
}
