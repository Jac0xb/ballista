import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction } from '../../../src/kit.js';
import { compiled } from '../kamino-liquidate-with-proof.js';
import { KAMINO_LEND } from '../shared.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

export function buildKaminoLiquidateRun(input: {
  templateAddress: Address;
  liquidator: Address;
  obligation: Address;
  lendingMarket: Address;
  lendingMarketAuthority: Address;
  repayReserve: Address;
  repayReserveLiquiditySupply: Address;
  withdrawReserve: Address;
  withdrawReserveCollateralMint: Address;
  withdrawReserveLiquiditySupply: Address;
  reservePriceFeed: Address;
  userSourceLiquidity: Address;
  userDestinationCollateral: Address;
  liquidityAmount: bigint;
  minAcceptableReceived: bigint;
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
      liquidator: at(input.liquidator),
      obligation: at(input.obligation),
      lendingMarket: at(input.lendingMarket),
      lendingMarketAuthority: at(input.lendingMarketAuthority),
      repayReserve: at(input.repayReserve),
      repayReserveLiquiditySupply: at(input.repayReserveLiquiditySupply),
      withdrawReserve: at(input.withdrawReserve),
      withdrawReserveCollateralMint: at(input.withdrawReserveCollateralMint),
      withdrawReserveLiquiditySupply: at(input.withdrawReserveLiquiditySupply),
      reservePriceFeed: at(input.reservePriceFeed),
      userSourceLiquidity: at(input.userSourceLiquidity),
      userDestinationCollateral: at(input.userDestinationCollateral),
    },
  });
}
