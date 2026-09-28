import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction } from '../../../src/kit.js';
import { compiled } from '../marginfi-to-kamino-rebalance.js';
import { KAMINO_LEND, MARGINFI_V2, SYSVAR_INSTRUCTIONS } from '../shared.js';
import { KAMINO_FARMS_PROGRAM, kaminoFarmPair, type KaminoFarm } from './kamino.js';
import { marginfiHealthAccounts, type MarginfiBalance } from './marginfi.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

/** Send it behind `buildKaminoRefreshes`, in the same transaction. */
export function buildMarginfiToKaminoRun(input: {
  templateAddress: Address;
  owner: Address;
  /** The owner's token account the assets pass through. */
  walletAta: Address;
  marginfiGroup: Address;
  marginfiAccount: Address;
  marginfiBank: Address;
  marginfiVault: Address;
  marginfiVaultAuthority: Address;
  /** Every balance the marginfi account still holds after this one is emptied. */
  remainingBalances: readonly MarginfiBalance[];
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
  minimumMoved: bigint;
}): Instruction {
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: { minimumMoved: input.minimumMoved },
    accounts: {
      marginfi: pinned(MARGINFI_V2),
      kamino: pinned(KAMINO_LEND),
      tokenProgram: pinned(TOKEN_PROGRAM),
      instructionsSysvar: pinned(SYSVAR_INSTRUCTIONS),
      owner: at(input.owner),
      walletAta: at(input.walletAta),
      marginfiGroup: at(input.marginfiGroup),
      marginfiAccount: at(input.marginfiAccount),
      marginfiBank: at(input.marginfiBank),
      marginfiVault: at(input.marginfiVault),
      marginfiVaultAuthority: at(input.marginfiVaultAuthority),
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
      healthAccounts: marginfiHealthAccounts(input.remainingBalances),
      // Kamino's v2 deposit ends in the farm pair and the Farms program.
      farmAccounts: [...kaminoFarmPair(input.collateralFarm), KAMINO_FARMS_PROGRAM],
    },
  });
}
