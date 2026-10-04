// #region run
import type { Address, Instruction } from '@solana/kit';

import { buildKitRunInstruction } from '@jac0xb/ballista/kit';
import { compiled } from '../pump-fun-buy-basket.js';
import { PUMP_FEES, PUMP_FUN } from '../shared.js';
import { SYSTEM_PROGRAM, at, pinned } from './programs.js';
import {
  PUMP_BUYBACK_FEE_RECIPIENT,
  PUMP_EVENT_AUTHORITY,
  PUMP_FEE_CONFIG,
  PUMP_GLOBAL,
  PUMP_GLOBAL_VOLUME_ACCUMULATOR,
  TOKEN_2022_PROGRAM,
  pumpCoinAccounts,
  pumpUserVolumeAccumulator,
  token2022Ata,
  type PumpCoin,
} from './pump-fun.js';

/** The template's most rows: each buy makes eight of the 64 calls a transaction holds. */
export const MAX_COINS_PER_RUN = 7;

export interface PumpBuy {
  coin: PumpCoin;
  /** Base units to buy. */
  amount: bigint;
  /** The most this buy may cost, in lamports, fees included. */
  maxSolCost: bigint;
}

/**
 * One row per coin, bought into the buyer's own associated token accounts, which must exist.
 * `budget` caps what the whole basket takes from the buyer. Put a compute-budget instruction
 * first: a buy costs pump.fun about 75,000 compute units.
 */
export async function buildPumpBuyBasketRun(input: {
  templateAddress: Address;
  buyer: Address;
  buys: readonly PumpBuy[];
  budget: bigint;
}): Promise<Instruction> {
  if (input.buys.length === 0 || input.buys.length > MAX_COINS_PER_RUN) {
    throw new Error(`A basket holds 1 to ${MAX_COINS_PER_RUN} coins, not ${input.buys.length}`);
  }
  const rows = await Promise.all(
    input.buys.map(async (buy) => {
      const coin = await pumpCoinAccounts(buy.coin);
      return {
        mint: at(coin.mint),
        bondingCurve: at(coin.bondingCurve),
        curveTokenAccount: at(coin.curveTokenAccount),
        buyerTokenAccount: at(await token2022Ata(input.buyer, coin.mint)),
        creatorVault: at(coin.creatorVault),
        bondingCurveV2: at(coin.bondingCurveV2),
        feeRecipient: at(coin.feeRecipient),
      };
    }),
  );
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: { budget: input.budget },
    accounts: {
      pumpProgram: pinned(PUMP_FUN),
      global: at(PUMP_GLOBAL),
      buyer: at(input.buyer),
      systemProgram: pinned(SYSTEM_PROGRAM),
      tokenProgram: pinned(TOKEN_2022_PROGRAM),
      eventAuthority: at(PUMP_EVENT_AUTHORITY),
      globalVolumeAccumulator: at(PUMP_GLOBAL_VOLUME_ACCUMULATOR),
      userVolumeAccumulator: at(await pumpUserVolumeAccumulator(input.buyer)),
      feeConfig: at(PUMP_FEE_CONFIG),
      feeProgram: pinned(PUMP_FEES),
      buybackFeeRecipient: at(PUMP_BUYBACK_FEE_RECIPIENT),
    },
    // One row of accounts and one of inputs per coin, in the same order.
    batchRows: rows,
    batchInputs: input.buys.map((buy) => ({ amount: buy.amount, maxSolCost: buy.maxSolCost })),
  });
}
// #endregion run
