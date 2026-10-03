import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction } from '../../../src/kit.js';
import { compiled } from '../kamino-liquidate-with-proof.js';
import { KAMINO_LEND, SYSVAR_INSTRUCTIONS } from '../shared.js';
import { KAMINO_FARMS_PROGRAM, kaminoFarmPair, type KaminoFarm } from './kamino.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

/** Send it behind `buildKaminoRefreshes`, in the same transaction. */
export function buildKaminoLiquidateRun(input: {
  templateAddress: Address;
  liquidator: Address;
  obligation: Address;
  lendingMarket: Address;
  lendingMarketAuthority: Address;
  repayReserve: Address;
  repayReserveLiquidityMint: Address;
  repayReserveLiquiditySupply: Address;
  withdrawReserve: Address;
  withdrawReserveLiquidityMint: Address;
  withdrawReserveCollateralMint: Address;
  withdrawReserveCollateralSupply: Address;
  withdrawReserveLiquiditySupply: Address;
  /** The withdrawn reserve's fee vault, which takes Kamino's fee on the seized collateral. */
  withdrawReserveFeeReceiver: Address;
  /** Pays the repayment. */
  userSourceLiquidity: Address;
  /** The liquidator's own accounts for the seized cTokens and for what they redeem to. */
  userDestinationCollateral: Address;
  userDestinationLiquidity: Address;
  /** The withdrawn reserve's collateral farm and the repaid reserve's debt farm, if they exist. */
  collateralFarm?: KaminoFarm;
  debtFarm?: KaminoFarm;
  liquidityAmount: bigint;
  minAcceptableReceived: bigint;
  /** In the seized collateral's own units: lamports for SOL collateral. */
  minimumBounty: bigint;
}): Instruction {
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: {
      liquidityAmount: input.liquidityAmount,
      minAcceptableReceived: input.minAcceptableReceived,
      minimumBounty: input.minimumBounty,
    },
    accounts: {
      kamino: pinned(KAMINO_LEND),
      tokenProgram: pinned(TOKEN_PROGRAM),
      instructionsSysvar: pinned(SYSVAR_INSTRUCTIONS),
      liquidator: at(input.liquidator),
      obligation: at(input.obligation),
      lendingMarket: at(input.lendingMarket),
      lendingMarketAuthority: at(input.lendingMarketAuthority),
      repayReserve: at(input.repayReserve),
      repayReserveLiquidityMint: at(input.repayReserveLiquidityMint),
      repayReserveLiquiditySupply: at(input.repayReserveLiquiditySupply),
      withdrawReserve: at(input.withdrawReserve),
      withdrawReserveLiquidityMint: at(input.withdrawReserveLiquidityMint),
      withdrawReserveCollateralMint: at(input.withdrawReserveCollateralMint),
      withdrawReserveCollateralSupply: at(input.withdrawReserveCollateralSupply),
      withdrawReserveLiquiditySupply: at(input.withdrawReserveLiquiditySupply),
      withdrawReserveFeeReceiver: at(input.withdrawReserveFeeReceiver),
      userSourceLiquidity: at(input.userSourceLiquidity),
      userDestinationCollateral: at(input.userDestinationCollateral),
      userDestinationLiquidity: at(input.userDestinationLiquidity),
    },
    accountGroups: {
      // Kamino's v2 liquidation ends in both farm pairs and Farms.
      farmAccounts: [
        ...kaminoFarmPair(input.collateralFarm),
        ...kaminoFarmPair(input.debtFarm),
        KAMINO_FARMS_PROGRAM,
      ],
    },
  });
}
