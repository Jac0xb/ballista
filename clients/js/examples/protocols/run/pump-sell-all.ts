// #region run
import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction } from '@jac0xb/ballista/kit';
import { compiled } from '../pump-fun-sell-all.js';
import { PUMP_FEES, PUMP_FUN } from '../shared.js';
import { SYSTEM_PROGRAM, at, pinned } from './programs.js';
import {
  PUMP_BUYBACK_FEE_RECIPIENT,
  PUMP_EVENT_AUTHORITY,
  PUMP_FEE_CONFIG,
  PUMP_GLOBAL,
  TOKEN_2022_PROGRAM,
  pumpCoinAccounts,
  token2022Ata,
  type PumpCoin,
} from './pump-fun.js';

/**
 * Sells everything in the seller's associated token account for `coin`, and fails unless the
 * seller receives at least `minSolOut` lamports after fees.
 */
export async function buildPumpSellAllRun(input: {
  templateAddress: Address;
  seller: Address;
  coin: PumpCoin;
  minSolOut: bigint;
}): Promise<Instruction> {
  const coin = await pumpCoinAccounts(input.coin);
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: { minSolOut: input.minSolOut },
    accounts: {
      pumpProgram: pinned(PUMP_FUN),
      global: at(PUMP_GLOBAL),
      feeRecipient: at(coin.feeRecipient),
      mint: at(coin.mint),
      bondingCurve: at(coin.bondingCurve),
      curveTokenAccount: at(coin.curveTokenAccount),
      sellerTokenAccount: at(await token2022Ata(input.seller, coin.mint)),
      seller: at(input.seller),
      systemProgram: pinned(SYSTEM_PROGRAM),
      creatorVault: at(coin.creatorVault),
      tokenProgram: pinned(TOKEN_2022_PROGRAM),
      eventAuthority: at(PUMP_EVENT_AUTHORITY),
      feeConfig: at(PUMP_FEE_CONFIG),
      feeProgram: pinned(PUMP_FEES),
      bondingCurveV2: at(coin.bondingCurveV2),
      buybackFeeRecipient: at(PUMP_BUYBACK_FEE_RECIPIENT),
    },
  });
}
// #endregion run
