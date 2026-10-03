/** What marginfi's health check reads after a withdrawal. */
import { getAddressEncoder, type Address } from '@solana/kit';

import type { KitAccountBinding } from '@jac0xb/ballista/kit';

/** A balance the marginfi account still holds: its bank, and the bank's oracle. */
export interface MarginfiBalance {
  bank: Address;
  oracle: Address;
}

const encoder = getAddressEncoder();

/** Orders two addresses by their bytes, which base58 text does not preserve. */
function compareAddresses(left: Address, right: Address): number {
  const [a, b] = [encoder.encode(left), encoder.encode(right)];
  for (let index = 0; index < a.length; index += 1) {
    const difference = (a[index] ?? 0) - (b[index] ?? 0);
    if (difference !== 0) return difference;
  }
  return 0;
}

/**
 * marginfi's `healthAccounts`: for every balance the account still holds after the withdrawal,
 * its bank and then the bank's oracle, read-only, by bank address from highest to lowest. Empty
 * when the withdrawn balance was the only one. A bank priced by more than one account (staked,
 * Kamino) takes more than this passes.
 */
export function marginfiHealthAccounts(remainingBalances: readonly MarginfiBalance[]): KitAccountBinding[] {
  return [...remainingBalances]
    .sort((left, right) => compareAddresses(right.bank, left.bank))
    .flatMap(({ bank, oracle }) => [{ address: bank }, { address: oracle }]);
}
