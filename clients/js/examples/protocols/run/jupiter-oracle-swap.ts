import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction, type KitAccountBinding } from '../../../src/kit.js';
import { compiled } from '../jupiter-oracle-checked-swap.js';
import { JUPITER_V6 } from '../shared.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

export function buildOracleSwapRun(input: {
  templateAddress: Address;
  /** The Pyth `PriceUpdateV2` account for the pair. */
  priceUpdate: Address;
  trader: Address;
  sourceAta: Address;
  destinationAta: Address;
  /** `splitJupiterRoute(swapData).args`. */
  routeArgs: Uint8Array;
  /** The feed's exponent, such as -8. */
  priceExponent: bigint;
  /** `10n ** BigInt(sourceDecimals - destinationDecimals - priceExponent)`. */
  scaleDivisor: bigint;
  toleranceBps: bigint;
  /** The route's account list from the fifth account on. */
  routeAccounts: readonly KitAccountBinding[];
}): Instruction {
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: {
      routeArgs: input.routeArgs,
      priceExponent: input.priceExponent,
      scaleDivisor: input.scaleDivisor,
      toleranceBps: input.toleranceBps,
    },
    accounts: {
      jupiter: pinned(JUPITER_V6),
      tokenProgram: pinned(TOKEN_PROGRAM),
      priceUpdate: at(input.priceUpdate),
      trader: at(input.trader),
      sourceAta: at(input.sourceAta),
      destinationAta: at(input.destinationAta),
    },
    accountGroups: { routeAccounts: input.routeAccounts },
  });
}
