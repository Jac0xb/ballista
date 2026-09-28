import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction, type KitAccountBinding } from '../../../src/kit.js';
import { compiled } from '../jupiter-deposit-exact-output.js';
import { JUPITER_V6, KAMINO_LEND } from '../shared.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

export function buildJupiterDepositRun(input: {
  templateAddress: Address;
  owner: Address;
  sourceAta: Address;
  destinationAta: Address;
  obligation: Address;
  lendingMarket: Address;
  lendingMarketAuthority: Address;
  reserve: Address;
  reserveLiquiditySupply: Address;
  reserveCollateralMint: Address;
  reserveDestinationDepositCollateral: Address;
  /** `splitJupiterRoute(swapData).args`: the Swap API's `route` data after the discriminator. */
  routeArgs: Uint8Array;
  minimumOut: bigint;
  /** The route's account list from the fifth account on; the template passes the first four. */
  routeAccounts: readonly KitAccountBinding[];
}): Instruction {
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: { routeArgs: input.routeArgs, minimumOut: input.minimumOut },
    accounts: {
      jupiter: pinned(JUPITER_V6),
      kamino: pinned(KAMINO_LEND),
      tokenProgram: pinned(TOKEN_PROGRAM),
      owner: at(input.owner),
      sourceAta: at(input.sourceAta),
      destinationAta: at(input.destinationAta),
      obligation: at(input.obligation),
      lendingMarket: at(input.lendingMarket),
      lendingMarketAuthority: at(input.lendingMarketAuthority),
      reserve: at(input.reserve),
      reserveLiquiditySupply: at(input.reserveLiquiditySupply),
      reserveCollateralMint: at(input.reserveCollateralMint),
      reserveDestinationDepositCollateral: at(input.reserveDestinationDepositCollateral),
    },
    accountGroups: { routeAccounts: input.routeAccounts },
  });
}
