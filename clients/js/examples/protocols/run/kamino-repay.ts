import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction, type KitAccountBinding } from '../../../src/kit.js';
import { compiled } from '../kamino-repay-swap-output.js';
import { JUPITER_V6, KAMINO_LEND } from '../shared.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

export function buildKaminoRepayRun(input: {
  templateAddress: Address;
  borrower: Address;
  collateralAta: Address;
  borrowedAssetAta: Address;
  obligation: Address;
  lendingMarket: Address;
  repayReserve: Address;
  reserveLiquiditySupply: Address;
  reservePriceFeed: Address;
  /** `splitJupiterRoute(swapData).args`. */
  routeArgs: Uint8Array;
  minimumRepayment: bigint;
  /** The route's account list from the fifth account on. */
  routeAccounts: readonly KitAccountBinding[];
}): Instruction {
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: { routeArgs: input.routeArgs, minimumRepayment: input.minimumRepayment },
    accounts: {
      jupiter: pinned(JUPITER_V6),
      kamino: pinned(KAMINO_LEND),
      tokenProgram: pinned(TOKEN_PROGRAM),
      borrower: at(input.borrower),
      collateralAta: at(input.collateralAta),
      borrowedAssetAta: at(input.borrowedAssetAta),
      obligation: at(input.obligation),
      lendingMarket: at(input.lendingMarket),
      repayReserve: at(input.repayReserve),
      reserveLiquiditySupply: at(input.reserveLiquiditySupply),
      reservePriceFeed: at(input.reservePriceFeed),
    },
    accountGroups: { routeAccounts: input.routeAccounts },
  });
}
