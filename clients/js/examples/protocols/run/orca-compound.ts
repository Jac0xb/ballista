// #region run
import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction } from '@jac0xb/ballista/kit';
import { compiled } from '../orca-compound-fees.js';
import { MEMO_PROGRAM, ORCA_WHIRLPOOL } from '../shared.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

export function buildOrcaCompoundRun(input: {
  templateAddress: Address;
  /** Signs for the position: the holder of its NFT, or a delegate approved on it. */
  positionAuthority: Address;
  whirlpool: Address;
  position: Address;
  /** The token account holding the position's NFT. Its owner is the position's holder. */
  positionTokenAccount: Address;
  tokenMintA: Address;
  tokenMintB: Address;
  /** The holder's own token accounts: the fees go there and are reinvested from there. */
  tokenOwnerAccountA: Address;
  tokenOwnerAccountB: Address;
  tokenVaultA: Address;
  tokenVaultB: Address;
  /** The tick arrays holding the position's lower and upper ticks. */
  tickArrayLower: Address;
  tickArrayUpper: Address;
  /** Fees at or below this, in either token's base units, are not collected. Not safe at 0. */
  dustFloor: bigint;
  /** The pool sqrt prices (Q64.64) the deposit accepts: Orca's `get_sqrt_price_slippage_bounds`. */
  minSqrtPrice: bigint;
  maxSqrtPrice: bigint;
}): Instruction {
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: {
      dustFloor: input.dustFloor,
      minSqrtPrice: input.minSqrtPrice,
      maxSqrtPrice: input.maxSqrtPrice,
    },
    accounts: {
      whirlpoolProgram: pinned(ORCA_WHIRLPOOL),
      tokenProgram: pinned(TOKEN_PROGRAM),
      memoProgram: pinned(MEMO_PROGRAM),
      positionAuthority: at(input.positionAuthority),
      whirlpool: at(input.whirlpool),
      position: at(input.position),
      positionTokenAccount: at(input.positionTokenAccount),
      tokenMintA: at(input.tokenMintA),
      tokenMintB: at(input.tokenMintB),
      tokenOwnerAccountA: at(input.tokenOwnerAccountA),
      tokenOwnerAccountB: at(input.tokenOwnerAccountB),
      tokenVaultA: at(input.tokenVaultA),
      tokenVaultB: at(input.tokenVaultB),
      tickArrayLower: at(input.tickArrayLower),
      tickArrayUpper: at(input.tickArrayUpper),
    },
  });
}
// #endregion run
