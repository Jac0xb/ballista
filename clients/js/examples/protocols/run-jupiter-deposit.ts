/**
 * Build and send the Jupiter-into-Kamino run.
 *
 * This is the run side of `jupiter-deposit-exact-output.ts`, and it shows the two parts that are
 * not just account binding: handing Jupiter's own instruction through as a `bytes` input, and
 * handing Jupiter's variable-length account list through as an account group.
 *
 * Nothing here needs an RPC connection. Point `SOLANA_RPC_URL` and `BALLISTA_KEYPAIR` at a
 * cluster and feed `swapInstruction` from the Jupiter Swap API to actually send it.
 */
import { address, getAddressDecoder, type Address, type Instruction } from '@solana/kit';

import { explainRunError } from '../../src/index.js';
import { BALLISTA_ADDRESS, buildKitRunInstruction, getTemplateAddress } from '../../src/kit.js';
import { compiled } from './jupiter-deposit-exact-output.js';
import { JUPITER_V6, KAMINO_LEND } from './shared.js';

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
 * The owner appears twice on purpose: once as a declared account the template constrains and
 * passes to both programs, and once at the head of Jupiter's own list. Group members are
 * forwarded with the transaction's writable flag and never sign, so any signature Jupiter needs
 * comes from the declared slot.
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
  const routeData = Uint8Array.from(Buffer.from(input.swap.data, 'base64'));
  if (routeData.length > 512) {
    throw new Error(`Route data is ${routeData.length} bytes; the template declares 512`);
  }

  // Jupiter's accounts, minus the owner, which the template already declares and passes first.
  const routeAccounts = input.swap.accounts
    .filter((entry) => entry.pubkey !== input.kamino.owner)
    .map((entry) => ({ address: address(entry.pubkey), writable: entry.isWritable }));

  const [templateAddress] = await getTemplateAddress(input.creator, input.templateId);
  return buildKitRunInstruction({
    compiled,
    programAddress: BALLISTA_ADDRESS,
    templateAddress,
    inputs: { routeData, minimumOut: input.minimumOut },
    accounts: {
      jupiter: { address: address(JUPITER_V6) },
      kamino: { address: address(KAMINO_LEND) },
      tokenProgram: { address: address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA') },
      owner: { address: input.kamino.owner },
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
  void buildJupiterDepositRun({
    creator: owner,
    templateId: 0,
    swap: {
      programId: JUPITER_V6,
      accounts: [{ pubkey: placeholder(8), isSigner: false, isWritable: true }],
      data: Buffer.from(Uint8Array.of(1, 2, 3, 4)).toString('base64'),
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
