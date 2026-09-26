/**
 * Build the Orca harvest run: the other run-side shape, batch rows.
 *
 * `orca-harvest-many-positions.ts` declares a row of two accounts and up to twelve iterations.
 * The caller passes one record per position and the iteration count follows from how many were
 * passed — there is no count in the instruction data to get wrong.
 *
 * Every other example on this page binds accounts by name and needs nothing beyond
 * `buildKitRunInstruction`; this one and `run-jupiter-deposit.ts` are the two that do not.
 */
import { address, type Address, type Instruction } from '@solana/kit';

import { explainRunError } from '../../src/index.js';
import { BALLISTA_ADDRESS, buildKitRunInstruction, getTemplateAddress } from '../../src/kit.js';
import { compiled } from './orca-harvest-many-positions.js';
import { ORCA_WHIRLPOOL } from './shared.js';

/** One position and the token account that proves ownership of it. */
export interface HarvestRow {
  position: Address;
  positionTokenAccount: Address;
}

export interface HarvestAccounts {
  positionAuthority: Address;
  whirlpool: Address;
  tokenOwnerAccountA: Address;
  tokenOwnerAccountB: Address;
  tokenVaultA: Address;
  tokenVaultB: Address;
}

/** The template's declared ceiling; more positions than this need a second run. */
export const MAX_POSITIONS_PER_RUN = 12;

export async function buildOrcaHarvestRun(input: {
  creator: Address;
  templateId: number;
  accounts: HarvestAccounts;
  positions: readonly HarvestRow[];
  dustFloor: bigint;
}): Promise<Instruction> {
  if (input.positions.length === 0) {
    throw new Error('The template declares minIterations 1; pass at least one position');
  }
  if (input.positions.length > MAX_POSITIONS_PER_RUN) {
    throw new Error(
      `${input.positions.length} positions exceeds the template's ${MAX_POSITIONS_PER_RUN}; split the run`,
    );
  }

  const [templateAddress] = await getTemplateAddress(input.creator, input.templateId);
  return buildKitRunInstruction({
    compiled,
    programAddress: BALLISTA_ADDRESS,
    templateAddress,
    inputs: { dustFloor: input.dustFloor },
    accounts: {
      whirlpoolProgram: { address: address(ORCA_WHIRLPOOL) },
      tokenProgram: { address: address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA') },
      positionAuthority: { address: input.accounts.positionAuthority },
      whirlpool: { address: input.accounts.whirlpool },
      tokenOwnerAccountA: { address: input.accounts.tokenOwnerAccountA },
      tokenOwnerAccountB: { address: input.accounts.tokenOwnerAccountB },
      tokenVaultA: { address: input.accounts.tokenVaultA },
      tokenVaultB: { address: input.accounts.tokenVaultB },
    },
    // One record per row, in order. The run's iteration count is derived from the account list.
    batchRows: input.positions.map((row) => ({
      position: { address: row.position },
      positionTokenAccount: { address: row.positionTokenAccount },
    })),
  });
}

/**
 * A harvest cannot fail because a position had earned nothing — that row is skipped. A failure
 * here is a real one: a position that is not owned by Whirlpools, or a wrong vault.
 */
export function describeFailure(code: number): string {
  const explanation = explainRunError(code, compiled);
  return explanation ? explanation.message : `code ${code} came from Whirlpools, not Ballista`;
}
