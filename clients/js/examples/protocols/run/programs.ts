/** Program accounts the run files bind by address. */
import { address, type Address } from '@solana/kit';

import type { KitAccountBinding } from '@jac0xb/ballista/kit';

export const TOKEN_PROGRAM = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';
export const SYSTEM_PROGRAM = '11111111111111111111111111111111';

/** A binding for a program the template pins: the run must pass exactly this address. */
export function pinned(program: string): KitAccountBinding {
  return { address: address(program) };
}

/** A binding for an account the caller chooses. */
export function at(account: Address): KitAccountBinding {
  return { address: account };
}
