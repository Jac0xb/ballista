/**
 * Build and send the Jupiter-into-Kamino run.
 *
 * This is the run side of `jupiter-deposit-exact-output.ts`, and it shows the two parts that are
 * not just account binding: handing Jupiter's `route` arguments through in parts, split by
 * `splitJupiterRoute` so that the template can cap the platform fee, and
 * handing the variable-length tail of Jupiter's account list, and of Kamino's, through as account
 * groups.
 *
 * Send it after Kamino's refreshes, in the same transaction: `refresh_reserve` for each reserve the
 * obligation holds, then `refresh_obligation`. See `kamino_refreshes` in
 * `clients/rust/examples/protocol_runs.rs`.
 *
 * Nothing here needs an RPC connection. Point `SOLANA_RPC_URL` and `BALLISTA_KEYPAIR` at a
 * cluster and feed `swapInstruction` from the Jupiter Swap API to actually send it.
 */
import { address, getAddressDecoder, type Address, type Instruction } from '@solana/kit';

import { explainRunError, failedProgram } from '../../src/index.js';
import { BALLISTA_ADDRESS, buildKitRunInstruction, getTemplateAddress } from '../../src/kit.js';
import { compiled } from './jupiter-deposit-exact-output.js';
import {
  JUPITER_ROUTE,
  JUPITER_ROUTE_FIXED_ACCOUNTS,
  JUPITER_V6,
  KAMINO_FARMS,
  KAMINO_LEND,
  SYSVAR_INSTRUCTIONS,
  splitJupiterRoute,
  type JupiterSwapInstruction,
} from './shared.js';

const TOKEN_PROGRAM = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';

const decoder = getAddressDecoder();
const placeholder = (byte: number): Address => decoder.decode(new Uint8Array(32).fill(byte));

/** The Kamino reserve and obligation accounts this template declares, by name. */
export interface KaminoDepositAccounts {
  owner: Address;
  destinationAta: Address;
  obligation: Address;
  lendingMarket: Address;
  lendingMarketAuthority: Address;
  reserve: Address;
  reserveLiquidityMint: Address;
  reserveLiquiditySupply: Address;
  reserveCollateralMint: Address;
  reserveDestinationDepositCollateral: Address;
  /** The reserve's collateral farm and the obligation's user state in it, when the reserve has one. */
  farm?: { reserveFarmState: Address; obligationFarmUserState: Address };
}

/**
 * Turns a Jupiter API response plus a Kamino reserve into one Ballista instruction.
 *
 * `route` starts its account list with the token program, the signing owner, and the owner's
 * source and destination token accounts. The template passes those four itself, in that order,
 * so this checks the API put them there and forwards only the rest as the group. Group members
 * are forwarded with the transaction's writable flag and never sign, so the signature Jupiter
 * needs comes from the template's declared `owner` slot.
 */
export async function buildJupiterDepositRun(input: {
  creator: Address;
  templateId: number;
  swap: JupiterSwapInstruction;
  kamino: KaminoDepositAccounts;
  minimumOut: bigint;
}): Promise<Instruction> {
  if (input.swap.programId !== JUPITER_V6) {
    throw new Error(`Template pins Jupiter v6; the API returned ${input.swap.programId}`);
  }
  const route = splitJupiterRoute(Uint8Array.from(Buffer.from(input.swap.data, 'base64')));
  if (route.routePlan.length > 512) {
    throw new Error(`The route plan is ${route.routePlan.length} bytes; the template declares 512`);
  }

  const [tokenProgram, authority, source, destination] = input.swap.accounts;
  if (
    tokenProgram?.pubkey !== TOKEN_PROGRAM ||
    authority?.pubkey !== input.kamino.owner ||
    !authority.isSigner ||
    source === undefined ||
    destination?.pubkey !== input.kamino.destinationAta
  ) {
    throw new Error(
      'Expected `route` to start with the token program, the owner as signer, the source, and the destination the deposit draws from',
    );
  }
  const routeAccounts = input.swap.accounts
    .slice(JUPITER_ROUTE_FIXED_ACCOUNTS)
    .map((entry) => ({ address: address(entry.pubkey), writable: entry.isWritable }));
  // Kamino's v2 tail: the farm pair, writable, when the reserve has a collateral farm, and the
  // Kamino program for each when it does not; then the Farms program.
  const farmAccounts = input.kamino.farm
    ? [
        { address: input.kamino.farm.obligationFarmUserState, writable: true },
        { address: input.kamino.farm.reserveFarmState, writable: true },
        { address: address(KAMINO_FARMS) },
      ]
    : [{ address: address(KAMINO_LEND) }, { address: address(KAMINO_LEND) }, { address: address(KAMINO_FARMS) }];

  const [templateAddress] = await getTemplateAddress(input.creator, input.templateId);
  return buildKitRunInstruction({
    compiled,
    programAddress: BALLISTA_ADDRESS,
    templateAddress,
    inputs: {
      routePlan: route.routePlan,
      inAmount: route.inAmount,
      quotedOutAmount: route.quotedOutAmount,
      slippageBps: route.slippageBps,
      platformFeeBps: route.platformFeeBps,
      minimumOut: input.minimumOut,
    },
    accounts: {
      jupiter: { address: address(JUPITER_V6) },
      kamino: { address: address(KAMINO_LEND) },
      tokenProgram: { address: address(TOKEN_PROGRAM) },
      instructionsSysvar: { address: address(SYSVAR_INSTRUCTIONS) },
      owner: { address: input.kamino.owner },
      sourceAta: { address: address(source.pubkey) },
      destinationAta: { address: input.kamino.destinationAta },
      obligation: { address: input.kamino.obligation },
      lendingMarket: { address: input.kamino.lendingMarket },
      lendingMarketAuthority: { address: input.kamino.lendingMarketAuthority },
      reserve: { address: input.kamino.reserve },
      reserveLiquidityMint: { address: input.kamino.reserveLiquidityMint },
      reserveLiquiditySupply: { address: input.kamino.reserveLiquiditySupply },
      reserveCollateralMint: { address: input.kamino.reserveCollateralMint },
      reserveDestinationDepositCollateral: { address: input.kamino.reserveDestinationDepositCollateral },
    },
    accountGroups: { routeAccounts, farmAccounts },
  });
}

const PROGRAM_NAMES: Readonly<Record<string, string>> = {
  [BALLISTA_ADDRESS]: 'Ballista',
  [JUPITER_V6]: 'Jupiter',
  [KAMINO_LEND]: 'Kamino',
};

/**
 * Turn a failed run's custom error code and logs into the step that raised it. `swapMetItsFloor`
 * means the route came in under the floor and nothing was deposited; the whole transaction rolled
 * back. Jupiter and Kamino number their errors from 6000 too, so the code is read as Ballista's
 * only when the logs show Ballista failed: Jupiter's slippage error, 6001, is not Ballista's
 * `InvalidTemplateAccount`.
 */
export function describeFailure(code: number | bigint, logs: readonly string[]): string {
  const explanation = explainRunError(code, compiled, { logs });
  if (explanation) return explanation.message;
  const program = failedProgram(logs);
  if (program === undefined) return `code ${code}; the logs name no program that failed`;
  return `code ${code} came from ${PROGRAM_NAMES[program] ?? program}`;
}

if (process.argv[1]?.endsWith('run-jupiter-deposit.ts')) {
  const owner = placeholder(7);
  // A `route` with an empty plan and a zeroed tail: the discriminator, a u32 zero, then 19 bytes.
  const emptyRoute = Uint8Array.from([...JUPITER_ROUTE, 0, 0, 0, 0, ...new Uint8Array(19)]);
  void buildJupiterDepositRun({
    creator: owner,
    templateId: 0,
    swap: {
      programId: JUPITER_V6,
      accounts: [
        { pubkey: TOKEN_PROGRAM, isSigner: false, isWritable: false },
        { pubkey: owner, isSigner: true, isWritable: true },
        { pubkey: placeholder(8), isSigner: false, isWritable: true },
        { pubkey: placeholder(9), isSigner: false, isWritable: true },
        { pubkey: placeholder(17), isSigner: false, isWritable: true },
      ],
      data: Buffer.from(emptyRoute).toString('base64'),
    },
    kamino: {
      owner,
      destinationAta: placeholder(9),
      obligation: placeholder(10),
      lendingMarket: placeholder(11),
      lendingMarketAuthority: placeholder(12),
      reserve: placeholder(13),
      reserveLiquiditySupply: placeholder(14),
      reserveCollateralMint: placeholder(15),
      reserveDestinationDepositCollateral: placeholder(16),
      reserveLiquidityMint: placeholder(18),
    },
    minimumOut: 1_000_000n,
  }).then((instruction) => {
    console.log(`accounts: ${instruction.accounts?.length ?? 0}`);
    console.log(`data bytes: ${instruction.data?.length ?? 0}`);
  });
}
