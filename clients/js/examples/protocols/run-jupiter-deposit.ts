/**
 * Build and send the Jupiter-into-Kamino run.
 *
 * This is the run side of `jupiter-deposit-exact-output.ts`, and it shows the two parts that are
 * not just account binding: handing Jupiter's `route` arguments through as a `bytes` input, and
 * handing the variable-length tail of Jupiter's account list through as an account group.
 *
 * Nothing here needs an RPC connection. Point `SOLANA_RPC_URL` and `BALLISTA_KEYPAIR` at a
 * cluster and feed `swapInstruction` from the Jupiter Swap API to actually send it.
 */
import { address, getAddressDecoder, type Address, type Instruction } from '@solana/kit';

import { explainRunError } from '../../src/index.js';
import { BALLISTA_ADDRESS, buildKitRunInstruction, getTemplateAddress } from '../../src/kit.js';
import { compiled } from './jupiter-deposit-exact-output.js';
import {
  JUPITER_ROUTE,
  JUPITER_ROUTE_FIXED_ACCOUNTS,
  JUPITER_V6,
  KAMINO_LEND,
  splitJupiterRoute,
} from './shared.js';

const TOKEN_PROGRAM = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';

/** The shape the Jupiter Swap API returns for `swapInstruction`. */
export interface JupiterSwapInstruction {
  programId: string;
  accounts: { pubkey: string; isSigner: boolean; isWritable: boolean }[];
  data: string;
}

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
  reserveLiquiditySupply: Address;
  reserveCollateralMint: Address;
  reserveDestinationDepositCollateral: Address;
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
  if (route.args.length > 512) {
    throw new Error(`Route arguments are ${route.args.length} bytes; the template declares 512`);
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

  const [templateAddress] = await getTemplateAddress(input.creator, input.templateId);
  return buildKitRunInstruction({
    compiled,
    programAddress: BALLISTA_ADDRESS,
    templateAddress,
    inputs: { routeArgs: route.args, minimumOut: input.minimumOut },
    accounts: {
      jupiter: { address: address(JUPITER_V6) },
      kamino: { address: address(KAMINO_LEND) },
      tokenProgram: { address: address(TOKEN_PROGRAM) },
      owner: { address: input.kamino.owner },
      sourceAta: { address: address(source.pubkey) },
      destinationAta: { address: input.kamino.destinationAta },
      obligation: { address: input.kamino.obligation },
      lendingMarket: { address: input.kamino.lendingMarket },
      lendingMarketAuthority: { address: input.kamino.lendingMarketAuthority },
      reserve: { address: input.kamino.reserve },
      reserveLiquiditySupply: { address: input.kamino.reserveLiquiditySupply },
      reserveCollateralMint: { address: input.kamino.reserveCollateralMint },
      reserveDestinationDepositCollateral: { address: input.kamino.reserveDestinationDepositCollateral },
    },
    accountGroups: { routeAccounts },
  });
}

/**
 * Turn a failed run's custom error code into the step that raised it. `swapMetItsFloor` means the
 * route came in under the floor and nothing was deposited; the whole transaction rolled back.
 */
export function describeFailure(code: number): string {
  const explanation = explainRunError(code, compiled);
  return explanation ? explanation.message : `code ${code} came from Jupiter or Kamino, not Ballista`;
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
    },
    minimumOut: 1_000_000n,
  }).then((instruction) => {
    console.log(`accounts: ${instruction.accounts?.length ?? 0}`);
    console.log(`data bytes: ${instruction.data?.length ?? 0}`);
  });
}
