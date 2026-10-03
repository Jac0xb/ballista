import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction, type KitAccountBinding } from '../../../src/kit.js';
import { compiled } from '../jupiter-deposit-exact-output.js';
import { JUPITER_V6, KAMINO_LEND, SYSVAR_INSTRUCTIONS, splitJupiterRoute } from '../shared.js';
import { KAMINO_FARMS_PROGRAM, kaminoFarmPair, type KaminoFarm } from './kamino.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

/** Send it behind `buildKaminoRefreshes`, in the same transaction. */
export function buildJupiterDepositRun(input: {
  templateAddress: Address;
  owner: Address;
  sourceAta: Address;
  destinationAta: Address;
  obligation: Address;
  lendingMarket: Address;
  lendingMarketAuthority: Address;
  reserve: Address;
  reserveLiquidityMint: Address;
  reserveLiquiditySupply: Address;
  reserveCollateralMint: Address;
  reserveDestinationDepositCollateral: Address;
  /** The reserve's collateral farm, if it has one. */
  collateralFarm?: KaminoFarm;
  /** The Swap API's `route` data. */
  routeData: Uint8Array;
  minimumOut: bigint;
  /** The route's account list from the fifth account on; the template passes the first four. */
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
      minimumOut: input.minimumOut,
    },
    accounts: {
      jupiter: pinned(JUPITER_V6),
      kamino: pinned(KAMINO_LEND),
      tokenProgram: pinned(TOKEN_PROGRAM),
      instructionsSysvar: pinned(SYSVAR_INSTRUCTIONS),
      owner: at(input.owner),
      sourceAta: at(input.sourceAta),
      destinationAta: at(input.destinationAta),
      obligation: at(input.obligation),
      lendingMarket: at(input.lendingMarket),
      lendingMarketAuthority: at(input.lendingMarketAuthority),
      reserve: at(input.reserve),
      reserveLiquidityMint: at(input.reserveLiquidityMint),
      reserveLiquiditySupply: at(input.reserveLiquiditySupply),
      reserveCollateralMint: at(input.reserveCollateralMint),
      reserveDestinationDepositCollateral: at(input.reserveDestinationDepositCollateral),
    },
    accountGroups: {
      routeAccounts: input.routeAccounts,
      // Kamino's v2 deposit ends in the farm pair and the Farms program.
      farmAccounts: [...kaminoFarmPair(input.collateralFarm), KAMINO_FARMS_PROGRAM],
    },
  });
}
