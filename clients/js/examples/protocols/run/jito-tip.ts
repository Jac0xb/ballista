import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction, type KitAccountBinding } from '../../../src/kit.js';
import { compiled } from '../jito-profit-guarded-tip.js';
import { JUPITER_V6 } from '../shared.js';
import { SYSTEM_PROGRAM, TOKEN_PROGRAM, at, pinned } from './programs.js';

export function buildJitoTipRun(input: {
  templateAddress: Address;
  searcher: Address;
  /** One of the eight `JITO_TIP_ACCOUNTS`. */
  jitoTip: Address;
  /** `splitJupiterRoute(swapData).args`. */
  strategyData: Uint8Array;
  tipLamports: bigint;
  minimumEdge: bigint;
  /** The route's account list from the third account on: the template passes the token program and the searcher. */
  strategyAccounts: readonly KitAccountBinding[];
}): Instruction {
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: {
      strategyData: input.strategyData,
      tipLamports: input.tipLamports,
      minimumEdge: input.minimumEdge,
    },
    accounts: {
      systemProgram: pinned(SYSTEM_PROGRAM),
      strategyProgram: pinned(JUPITER_V6),
      tokenProgram: pinned(TOKEN_PROGRAM),
      searcher: at(input.searcher),
      jitoTip: at(input.jitoTip),
    },
    accountGroups: { strategyAccounts: input.strategyAccounts },
  });
}
