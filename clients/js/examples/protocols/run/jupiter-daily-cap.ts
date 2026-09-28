import type { Address, Instruction } from '@solana/kit';

import { registryIndex } from '../../../src/index.js';
import { buildKitRunInstruction, findRegistryEntryAddress, type KitAccountBinding } from '../../../src/kit.js';
import { compiled } from '../jupiter-daily-cap-swap.js';
import { JUPITER_V6, splitJupiterRoute } from '../shared.js';
import { SYSTEM_PROGRAM, TOKEN_PROGRAM, at, pinned } from './programs.js';

/**
 * The actor's entry is its `dailySpend` entry for its own address: the actor's first run creates
 * it, and pays its rent.
 */
export async function buildDailyCapRun(input: {
  templateAddress: Address;
  /** Signs, keys the entry, and pays its rent on the first run. */
  actor: Address;
  /** The Swap API's `route` data. */
  routeData: Uint8Array;
  /** The route's account list from the third account on: the template passes the token program and the actor. */
  actionAccounts: readonly KitAccountBinding[];
}): Promise<Instruction> {
  const route = splitJupiterRoute(input.routeData);
  const [spend] = await findRegistryEntryAddress(
    input.templateAddress,
    registryIndex(compiled, 'dailySpend'),
    input.actor,
  );
  return buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: {
      routePlan: route.routePlan,
      inAmount: route.inAmount,
      quotedOutAmount: route.quotedOutAmount,
      slippageBps: route.slippageBps,
      platformFeeBps: route.platformFeeBps,
    },
    accounts: {
      actionProgram: pinned(JUPITER_V6),
      tokenProgram: pinned(TOKEN_PROGRAM),
      actor: at(input.actor),
      spend: at(spend),
      systemProgram: pinned(SYSTEM_PROGRAM),
    },
    accountGroups: { actionAccounts: input.actionAccounts },
  });
}
