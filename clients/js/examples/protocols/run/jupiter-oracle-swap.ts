import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction, type KitAccountBinding } from '../../../src/kit.js';
import { compiled } from '../jupiter-oracle-checked-swap.js';
import { JUPITER_V6, splitJupiterRoute } from '../shared.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

export function buildOracleSwapRun(input: {
  templateAddress: Address;
  /** The Pyth `PriceUpdateV2` account that prices the token sold in the token bought. */
  priceUpdate: Address;
  /** That price's Pyth feed id, as 32 bytes: SOL/USD's is `ef0d8b6f…c280b56d`. */
  feedId: Uint8Array;
  trader: Address;
  /** The trader's own token accounts, holding the two mints below. */
  sourceAta: Address;
  destinationAta: Address;
  /** The template reads both mints' decimals, and the price's exponent, itself. */
  sourceMint: Address;
  destinationMint: Address;
  /** The Swap API's `route` data. */
  routeData: Uint8Array;
  toleranceBps: bigint;
  /** The route's account list from the fifth account on. */
  routeAccounts: readonly KitAccountBinding[];
}): Instruction {
  const route = splitJupiterRoute(input.routeData);
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: {
      feedId: input.feedId,
      routePlan: route.routePlan,
      inAmount: route.inAmount,
      quotedOutAmount: route.quotedOutAmount,
      slippageBps: route.slippageBps,
      platformFeeBps: route.platformFeeBps,
      toleranceBps: input.toleranceBps,
    },
    accounts: {
      jupiter: pinned(JUPITER_V6),
      tokenProgram: pinned(TOKEN_PROGRAM),
      priceUpdate: at(input.priceUpdate),
      trader: at(input.trader),
      sourceAta: at(input.sourceAta),
      destinationAta: at(input.destinationAta),
      sourceMint: at(input.sourceMint),
      destinationMint: at(input.destinationMint),
    },
    accountGroups: { routeAccounts: input.routeAccounts },
  });
}
