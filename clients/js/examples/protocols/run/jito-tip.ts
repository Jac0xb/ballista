import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction } from '../../../src/kit.js';
import { compiled } from '../jito-profit-guarded-tip.js';
import { JUPITER_V6, joinRoundTrip, splitJupiterRoute, type JupiterLeg } from '../shared.js';
import { SYSTEM_PROGRAM, TOKEN_PROGRAM, at, pinned } from './programs.js';

/**
 * The strategy is one Jupiter `route` from the searcher's wrapped SOL back to it. The Swap API
 * quotes it as two legs, SOL to another token and back, and `joinRoundTrip` joins them.
 */
export function buildJitoTipRun(input: {
  templateAddress: Address;
  searcher: Address;
  /** The searcher's wrapped-SOL token account: where the first leg starts and the second ends. */
  wsolAccount: Address;
  /** One of the eight `JITO_TIP_ACCOUNTS`. */
  jitoTip: Address;
  /** Each leg's quote mints and the Swap API's `swapInstruction`: one step each. */
  legs: readonly [JupiterLeg, JupiterLeg];
  tipLamports: bigint;
  minimumEdge: bigint;
}): Instruction {
  const { routeData, strategyAccounts } = joinRoundTrip(...input.legs);
  const route = splitJupiterRoute(routeData);
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: {
      routePlan: route.routePlan,
      inAmount: route.inAmount,
      quotedOutAmount: route.quotedOutAmount,
      slippageBps: route.slippageBps,
      platformFeeBps: route.platformFeeBps,
      tipLamports: input.tipLamports,
      minimumEdge: input.minimumEdge,
    },
    accounts: {
      systemProgram: pinned(SYSTEM_PROGRAM),
      strategyProgram: pinned(JUPITER_V6),
      tokenProgram: pinned(TOKEN_PROGRAM),
      searcher: at(input.searcher),
      wsolAccount: at(input.wsolAccount),
      jitoTip: at(input.jitoTip),
    },
    accountGroups: { strategyAccounts },
  });
}
