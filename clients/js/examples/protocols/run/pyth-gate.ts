import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction, type KitAccountBinding } from '../../../src/kit.js';
import { compiled } from '../pyth-fresh-price-gate.js';
import { JUPITER_V6, splitJupiterRoute } from '../shared.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

export function buildPythGateRun(input: {
  templateAddress: Address;
  /** The Pyth `PriceUpdateV2` account. */
  priceUpdate: Address;
  /**
   * The feed the price must be, as 32 bytes: SOL/USD's is
   * `ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d`.
   */
  feedId: Uint8Array;
  /** The exponent the bounds below are in units of: SOL/USD's is -8. */
  exponent: bigint;
  actor: Address;
  /** Seconds. */
  maximumAge: bigint;
  maximumConfidence: bigint;
  floorPrice: bigint;
  ceilingPrice: bigint;
  /** The Swap API's `route` data. */
  routeData: Uint8Array;
  /** The route's account list from the third account on: the template passes the token program and the actor. */
  actionAccounts: readonly KitAccountBinding[];
}): Instruction {
  const route = splitJupiterRoute(input.routeData);
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: {
      feedId: input.feedId,
      exponent: input.exponent,
      maximumAge: input.maximumAge,
      maximumConfidence: input.maximumConfidence,
      floorPrice: input.floorPrice,
      ceilingPrice: input.ceilingPrice,
      routePlan: route.routePlan,
      inAmount: route.inAmount,
      quotedOutAmount: route.quotedOutAmount,
      slippageBps: route.slippageBps,
      platformFeeBps: route.platformFeeBps,
    },
    accounts: {
      priceUpdate: at(input.priceUpdate),
      actionProgram: pinned(JUPITER_V6),
      tokenProgram: pinned(TOKEN_PROGRAM),
      actor: at(input.actor),
    },
    accountGroups: { actionAccounts: input.actionAccounts },
  });
}
