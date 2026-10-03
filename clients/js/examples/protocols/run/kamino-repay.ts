import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction, type KitAccountBinding } from '../../../src/kit.js';
import { compiled } from '../kamino-repay-swap-output.js';
import { JUPITER_V6, KAMINO_LEND, SYSVAR_INSTRUCTIONS, splitJupiterRoute } from '../shared.js';
import { KAMINO_FARMS_PROGRAM, kaminoFarmPair, type KaminoFarm } from './kamino.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

/** Send it behind `buildKaminoRefreshes`, in the same transaction. */
export function buildKaminoRepayRun(input: {
  templateAddress: Address;
  borrower: Address;
  collateralAta: Address;
  /** The borrower's own token account: it receives the swap and funds the repayment. */
  borrowedAssetAta: Address;
  obligation: Address;
  lendingMarket: Address;
  /** The repayment's tail takes the market's authority PDA. */
  lendingMarketAuthority: Address;
  repayReserve: Address;
  reserveLiquidityMint: Address;
  reserveLiquiditySupply: Address;
  /** The reserve's debt farm, if it has one. */
  debtFarm?: KaminoFarm;
  /** The Swap API's `route` data. */
  routeData: Uint8Array;
  minimumRepayment: bigint;
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
      minimumRepayment: input.minimumRepayment,
    },
    accounts: {
      jupiter: pinned(JUPITER_V6),
      kamino: pinned(KAMINO_LEND),
      tokenProgram: pinned(TOKEN_PROGRAM),
      instructionsSysvar: pinned(SYSVAR_INSTRUCTIONS),
      borrower: at(input.borrower),
      collateralAta: at(input.collateralAta),
      borrowedAssetAta: at(input.borrowedAssetAta),
      obligation: at(input.obligation),
      lendingMarket: at(input.lendingMarket),
      repayReserve: at(input.repayReserve),
      reserveLiquidityMint: at(input.reserveLiquidityMint),
      reserveLiquiditySupply: at(input.reserveLiquiditySupply),
    },
    accountGroups: {
      routeAccounts: input.routeAccounts,
      // Kamino's v2 repayment ends in the farm pair, the lending market authority and Farms.
      farmAccounts: [
        ...kaminoFarmPair(input.debtFarm),
        { address: input.lendingMarketAuthority },
        KAMINO_FARMS_PROGRAM,
      ],
    },
  });
}
