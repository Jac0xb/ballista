import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction, type KitAccountBinding } from '../../../src/kit.js';
import { compiled } from '../jito-profit-guarded-tip.js';
import { JUPITER_V6, splitJupiterRoute } from '../shared.js';
import { SYSTEM_PROGRAM, TOKEN_PROGRAM, at, pinned } from './programs.js';

/**
 * The strategy is one Jupiter `route` from the searcher's wrapped SOL back to it. The Swap API
 * quotes the two legs separately, so they are joined: the second leg's step goes after the first's
 * with its token indices moved up by one, and the joined list is the second leg's fixed accounts
 * with the first leg's source, then both legs' step accounts. `round_trip` in
 * `tests/protocols/tests/jito_tip.rs` joins single-step legs.
 */
export function buildJitoTipRun(input: {
  templateAddress: Address;
  searcher: Address;
  /** The searcher's wrapped-SOL token account: the round trip's source and its destination. */
  wsolAccount: Address;
  /** One of the eight `JITO_TIP_ACCOUNTS`. */
  jitoTip: Address;
  /** The joined `route` data, discriminator included. */
  routeData: Uint8Array;
  tipLamports: bigint;
  minimumEdge: bigint;
  /**
   * The joined route's account list from the fifth account on: the template passes the token
   * program, the searcher, and the wrapped-SOL account twice.
   */
  strategyAccounts: readonly KitAccountBinding[];
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
    accountGroups: { strategyAccounts: input.strategyAccounts },
  });
}
