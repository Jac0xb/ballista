import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction, type KitAccountBinding } from '../../../src/kit.js';
import { compiled } from '../jupiter-oracle-checked-swap.js';
import { JUPITER_V6, USDC_MINT, WRAPPED_SOL_MINT, splitJupiterRoute } from '../shared.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

/** The feed, the two mints and the tolerance are the template's own constants, not run inputs. */
export function buildOracleSwapRun(input: {
  templateAddress: Address;
  /** A `PriceUpdateV2` for SOL/USD, the feed the template pins. */
  priceUpdate: Address;
  trader: Address;
  /** The trader's own wrapped SOL and USDC accounts. */
  sourceAta: Address;
  destinationAta: Address;
  /** The Swap API's `route` data. */
  routeData: Uint8Array;
  /** The route's account list from the fifth account on. */
  routeAccounts: readonly KitAccountBinding[];
}): Instruction {
  const route = splitJupiterRoute(input.routeData);
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: {
      routePlan: route.routePlan,
      inAmount: route.inAmount,
      quotedOutAmount: route.quotedOutAmount,
      slippageBps: route.slippageBps,
      platformFeeBps: route.platformFeeBps,
    },
    accounts: {
      jupiter: pinned(JUPITER_V6),
      tokenProgram: pinned(TOKEN_PROGRAM),
      priceUpdate: at(input.priceUpdate),
      trader: at(input.trader),
      sourceAta: at(input.sourceAta),
      destinationAta: at(input.destinationAta),
      sourceMint: pinned(WRAPPED_SOL_MINT),
      destinationMint: pinned(USDC_MINT),
    },
    accountGroups: { routeAccounts: input.routeAccounts },
  });
}
