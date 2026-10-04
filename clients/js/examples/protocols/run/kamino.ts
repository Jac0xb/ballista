/** Kamino's refreshes, which go before a Kamino run, and the farm accounts its v2 tails end in. */
// #region kamino-refreshes
import { AccountRole, address, type Address, type Instruction } from '@solana/kit';

import type { KitAccountBinding } from '@jac0xb/ballista/kit';
import { KAMINO_FARMS, KAMINO_LEND, KAMINO_REFRESH_OBLIGATION, KAMINO_REFRESH_RESERVE } from '../shared.js';

/** A reserve, and the Scope price account its config names. */
export interface KaminoReserve {
  reserve: Address;
  scopePrices: Address;
}

/**
 * Kamino's refreshes, which go before the run in the same transaction. Kamino's v2 deposit,
 * repayment and liquidation check only that the obligation and the reserves they price were
 * refreshed in the current slot, not where.
 *
 * - `held` is every reserve the obligation holds, deposits in its deposit order and then borrows
 *   in its borrow order. The main market prices by Scope alone, so the Pyth and Switchboard slots
 *   take the Kamino program, which Kamino reads as "none".
 * - `touched` adds any other reserve the run needs fresh. A first deposit's needs none: Kamino's
 *   deposit refreshes its own reserve.
 * - `referrerTokenStates` is empty unless the obligation has a referrer; then Kamino expects one
 *   per borrow after the reserves.
 */
export function buildKaminoRefreshes(input: {
  lendingMarket: Address;
  obligation: Address;
  held: readonly KaminoReserve[];
  touched?: readonly KaminoReserve[];
  referrerTokenStates?: readonly Address[];
}): Instruction[] {
  const kamino = address(KAMINO_LEND);
  const none = { address: kamino, role: AccountRole.READONLY };
  const refreshed = new Set<Address>();
  const instructions: Instruction[] = [];
  for (const { reserve, scopePrices } of [...input.held, ...(input.touched ?? [])]) {
    if (refreshed.has(reserve)) continue;
    refreshed.add(reserve);
    instructions.push({
      programAddress: kamino,
      accounts: [
        { address: reserve, role: AccountRole.WRITABLE },
        { address: input.lendingMarket, role: AccountRole.READONLY },
        none, // Pyth
        none, // Switchboard price
        none, // Switchboard TWAP
        { address: scopePrices, role: AccountRole.READONLY },
      ],
      data: KAMINO_REFRESH_RESERVE,
    });
  }
  instructions.push({
    programAddress: kamino,
    accounts: [
      { address: input.lendingMarket, role: AccountRole.READONLY },
      { address: input.obligation, role: AccountRole.WRITABLE },
      ...input.held.map(({ reserve }) => ({ address: reserve, role: AccountRole.WRITABLE })),
      ...(input.referrerTokenStates ?? []).map((state) => ({ address: state, role: AccountRole.WRITABLE })),
    ],
    data: KAMINO_REFRESH_OBLIGATION,
  });
  return instructions;
}

/** A reserve's farm, as a Kamino v2 tail passes it. */
export interface KaminoFarm {
  /** The obligation's user state in the farm, which `init_obligation_farms_for_reserve` creates. */
  obligationFarmUserState: Address;
  reserveFarmState: Address;
}

/**
 * One farm in a Kamino v2 account tail, both writable. When the reserve has no such farm, Kamino
 * reads its own program, twice and read-only, as "none".
 */
export function kaminoFarmPair(farm: KaminoFarm | undefined): KitAccountBinding[] {
  return farm
    ? [
        { address: farm.obligationFarmUserState, writable: true },
        { address: farm.reserveFarmState, writable: true },
      ]
    : [{ address: address(KAMINO_LEND) }, { address: address(KAMINO_LEND) }];
}

/** The Farms program, which ends every Kamino v2 tail. */
export const KAMINO_FARMS_PROGRAM: KitAccountBinding = { address: address(KAMINO_FARMS) };
// #endregion kamino-refreshes
