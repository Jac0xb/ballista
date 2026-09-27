/**
 * Build the Orca harvest run: the other run-side shape, batch rows.
 *
 * `orca-harvest-many-positions.ts` declares a row of four accounts and up to twelve iterations.
 * The caller passes one record per position and the iteration count follows from how many were
 * passed — there is no count in the instruction data to get wrong.
 *
 * Every other example on this page binds accounts by name and needs nothing beyond
 * `buildKitRunInstruction`; this one and `run-jupiter-deposit.ts` are the two that do not.
 */
import {
  address,
  getAddressEncoder,
  getProgramDerivedAddress,
  type Address,
  type Instruction,
} from '@solana/kit';

import { explainRunError } from '../../src/index.js';
import { BALLISTA_ADDRESS, buildKitRunInstruction, getTemplateAddress } from '../../src/kit.js';
import { compiled } from './orca-harvest-many-positions.js';
import { ORCA_WHIRLPOOL } from './shared.js';

/** One position, the token account holding its NFT, and the tick arrays holding its two bounds. */
export interface HarvestRow {
  position: Address;
  positionTokenAccount: Address;
  /** The tick array holding the position's lower tick: `getOrcaTickArrayAddress`. */
  tickArrayLower: Address;
  /** The tick array holding the position's upper tick. */
  tickArrayUpper: Address;
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

/** Ticks in one Whirlpool tick array. */
const TICKS_PER_ARRAY = 88;

/**
 * The tick array holding `tickIndex` in a pool whose tick spacing is `tickSpacing`: the PDA
 * `["tick_array", whirlpool, start]`, where `start` is the array's first tick as a decimal string.
 */
export async function getOrcaTickArrayAddress(
  whirlpool: Address,
  tickIndex: number,
  tickSpacing: number,
): Promise<Address> {
  const span = TICKS_PER_ARRAY * tickSpacing;
  const start = Math.floor(tickIndex / span) * span;
  const [tickArray] = await getProgramDerivedAddress({
    programAddress: address(ORCA_WHIRLPOOL),
    seeds: ['tick_array', getAddressEncoder().encode(whirlpool), String(start)],
  });
  return tickArray;
}

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
      tickArrayLower: { address: row.tickArrayLower },
      tickArrayUpper: { address: row.tickArrayUpper },
    })),
  });
}

/** The program named by the first `Program <id> failed: …` log line: the innermost that failed. */
export function failedProgram(logs: readonly string[]): string | undefined {
  for (const line of logs) {
    const match = /^Program ([1-9A-HJ-NP-Za-km-z]{32,44}) failed: /.exec(line);
    if (match) return match[1];
  }
  return undefined;
}

/**
 * Says which program refused a harvest, and where, from the failed transaction's code and logs.
 *
 * Whirlpools numbers its errors from 6000, as Ballista does, so the code alone cannot say who
 * refused: 6019 is Whirlpools' `MissingOrInvalidDelegate` and Ballista's `ReturnDataMismatch`. The
 * logs can. A failing CPI logs `Program <id> failed` first and every caller repeats the code after
 * it, so the first such line names the program the code belongs to.
 */
export function describeFailure(code: number, logs: readonly string[]): string {
  const program = failedProgram(logs);
  if (program === undefined) return `code ${code}; the logs name no program that failed`;
  if (program !== BALLISTA_ADDRESS) {
    return `code ${code} came from ${program === ORCA_WHIRLPOOL ? 'Whirlpools' : program}, not Ballista`;
  }
  const explanation = explainRunError(code, compiled);
  return explanation ? explanation.message : `code ${code} came from Ballista`;
}
