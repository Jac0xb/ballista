import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction, type KitAccountBinding } from '@jac0xb/ballista/kit';
import { compiled } from '../token-sweep-into-swap.js';
import { JUPITER_V6, splitJupiterRoute } from '../shared.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

export function buildTokenSweepRun(input: {
  templateAddress: Address;
  seller: Address;
  /** The seller's own token accounts: what is sold, and where the proceeds land. */
  sourceAta: Address;
  destinationAta: Address;
  /** The Swap API's `route` data, quoted for any amount; the template rescales it. */
  routeData: Uint8Array;
  dustFloor: bigint;
  /** The route's account list from the fifth account on. */
  routeAccounts: readonly KitAccountBinding[];
}): Instruction {
  const route = splitJupiterRoute(input.routeData);
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: {
      routePlan: route.routePlan,
      quotedInAmount: route.inAmount,
      quotedOutAmount: route.quotedOutAmount,
      slippageBps: route.slippageBps,
      platformFeeBps: route.platformFeeBps,
      dustFloor: input.dustFloor,
    },
    accounts: {
      jupiter: pinned(JUPITER_V6),
      tokenProgram: pinned(TOKEN_PROGRAM),
      seller: at(input.seller),
      sourceAta: at(input.sourceAta),
      destinationAta: at(input.destinationAta),
    },
    accountGroups: { routeAccounts: input.routeAccounts },
  });
}
