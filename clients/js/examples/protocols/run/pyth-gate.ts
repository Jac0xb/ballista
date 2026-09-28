import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction, type KitAccountBinding } from '../../../src/kit.js';
import { compiled } from '../pyth-fresh-price-gate.js';
import { JUPITER_V6 } from '../shared.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

export function buildPythGateRun(input: {
  templateAddress: Address;
  /** The Pyth `PriceUpdateV2` account. */
  priceUpdate: Address;
  /** The feed the price must be, as 32 bytes: SOL/USD's is `ef0d8b6f…c280b56d`. */
  feedId: Uint8Array;
  /** The exponent the bounds below are in units of: SOL/USD's is -8. */
  exponent: bigint;
  actor: Address;
  /** Seconds. */
  maximumAge: bigint;
  maximumConfidence: bigint;
  floorPrice: bigint;
  ceilingPrice: bigint;
  /** `splitJupiterRoute(swapData).args`. */
  actionData: Uint8Array;
  /** The route's account list from the third account on: the template passes the token program and the actor. */
  actionAccounts: readonly KitAccountBinding[];
}): Instruction {
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
      actionData: input.actionData,
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
