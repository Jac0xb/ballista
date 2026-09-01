import { readFile } from 'node:fs/promises';

import {
  address,
  appendTransactionMessageInstruction,
  assertIsTransactionWithBlockhashLifetime,
  createKeyPairSignerFromBytes,
  createSolanaRpc,
  createSolanaRpcSubscriptions,
  createTransactionMessage,
  devnet,
  getAddressEncoder,
  getProgramDerivedAddress,
  getSignatureFromTransaction,
  pipe,
  sendAndConfirmTransactionFactory,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signTransactionMessageWithSigners,
  type Address,
  type Instruction,
} from '@solana/kit';

import {
  account,
  compileTemplate,
  defineTemplate,
  ensureAssociatedTokenAccount,
} from '../src/index.js';
import {
  BALLISTA_ADDRESS,
  buildKitRunInstruction,
  buildKitTemplateUploadPlan,
  getTemplateAddress,
} from '../src/kit.js';

export const SYSTEM_PROGRAM = address('11111111111111111111111111111111');
export const TOKEN_PROGRAM = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');
export const ASSOCIATED_TOKEN_PROGRAM = address('ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL');
export const DEVNET_USDC_MINT = address('4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU');

const addressEncoder = getAddressEncoder();
const addressBytes = (value: Address): Uint8Array<ArrayBuffer> => {
  const bytes = new Uint8Array(32);
  bytes.set(addressEncoder.encode(value));
  return bytes;
};

export const ensureUsdcAtaTemplate = defineTemplate({
  accounts: {
    associatedTokenProgram: {
      address: addressBytes(ASSOCIATED_TOKEN_PROGRAM),
      executable: true,
    },
    tokenProgram: {
      address: addressBytes(TOKEN_PROGRAM),
      executable: true,
    },
    systemProgram: {
      address: addressBytes(SYSTEM_PROGRAM),
      executable: true,
    },
    usdcMint: {
      address: addressBytes(DEVNET_USDC_MINT),
      owner: addressBytes(TOKEN_PROGRAM),
      minDataLength: 82,
    },
    payer: {
      signer: true,
      writable: true,
      owner: addressBytes(SYSTEM_PROGRAM),
    },
    wallet: {},
    usdcAta: { writable: true },
  },
  steps: [
    ensureAssociatedTokenAccount({
      associatedTokenProgram: account.fixed('associatedTokenProgram'),
      payer: account.fixed('payer'),
      associatedTokenAccount: account.fixed('usdcAta'),
      owner: account.fixed('wallet'),
      mint: account.fixed('usdcMint'),
      systemProgram: account.fixed('systemProgram'),
      tokenProgram: account.fixed('tokenProgram'),
    }),
  ],
});

export const compiledEnsureUsdcAta = compileTemplate(ensureUsdcAtaTemplate);

export async function getUsdcAta(wallet: Address): Promise<Address> {
  const [associatedTokenAccount] = await getProgramDerivedAddress({
    programAddress: ASSOCIATED_TOKEN_PROGRAM,
    seeds: [
      addressEncoder.encode(wallet),
      addressEncoder.encode(TOKEN_PROGRAM),
      addressEncoder.encode(DEVNET_USDC_MINT),
    ],
  });
  return associatedTokenAccount;
}

export async function buildEnsureUsdcAtaRunInstruction(input: {
  payer: Address;
  wallet: Address;
  templateAddress: Address;
}): Promise<{ instruction: Instruction; usdcAta: Address }> {
  const usdcAta = await getUsdcAta(input.wallet);
  const instruction = buildKitRunInstruction({
    compiled: compiledEnsureUsdcAta,
    programAddress: BALLISTA_ADDRESS,
    templateAddress: input.templateAddress,
    accounts: {
      associatedTokenProgram: { address: ASSOCIATED_TOKEN_PROGRAM },
      tokenProgram: { address: TOKEN_PROGRAM },
      systemProgram: { address: SYSTEM_PROGRAM },
      usdcMint: { address: DEVNET_USDC_MINT },
      payer: { address: input.payer },
      wallet: { address: input.wallet },
      usdcAta: { address: usdcAta },
    },
  });
  return { instruction, usdcAta };
}

async function main(): Promise<void> {
  const action = process.argv[2];
  if (action !== 'upload' && action !== 'run') {
    throw new Error('Usage: tsx examples/ensure-usdc-ata.ts <upload|run>');
  }

  const keypairPath = process.env.BALLISTA_KEYPAIR;
  if (!keypairPath) throw new Error('BALLISTA_KEYPAIR must point to a Solana keypair JSON file');
  const keypairBytes = new Uint8Array(JSON.parse(await readFile(keypairPath, 'utf8')) as number[]);
  const payer = await createKeyPairSignerFromBytes(keypairBytes);
  const wallet = process.env.BALLISTA_WALLET ? address(process.env.BALLISTA_WALLET) : payer.address;
  const templateId = Number(process.env.BALLISTA_TEMPLATE_ID ?? '21843');
  if (!Number.isInteger(templateId) || templateId < 0 || templateId > 0xffff) {
    throw new RangeError('BALLISTA_TEMPLATE_ID must be a u16');
  }

  const rpcUrl = process.env.SOLANA_RPC_URL ?? 'https://api.devnet.solana.com';
  const subscriptionsUrl = process.env.SOLANA_WS_URL ?? 'wss://api.devnet.solana.com';
  const rpc = createSolanaRpc(devnet(rpcUrl));
  const rpcSubscriptions = createSolanaRpcSubscriptions(devnet(subscriptionsUrl));
  const sendAndConfirm = sendAndConfirmTransactionFactory({ rpc, rpcSubscriptions });

  const sendInstruction = async (instruction: Instruction): Promise<string> => {
    const { value: latestBlockhash } = await rpc.getLatestBlockhash({ commitment: 'confirmed' }).send();
    const message = pipe(
      createTransactionMessage({ version: 0 }),
      (value) => setTransactionMessageFeePayerSigner(payer, value),
      (value) => setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, value),
      (value) => appendTransactionMessageInstruction(instruction, value),
    );
    const transaction = await signTransactionMessageWithSigners(message);
    assertIsTransactionWithBlockhashLifetime(transaction);
    await sendAndConfirm(transaction, { commitment: 'confirmed' });
    return getSignatureFromTransaction(transaction);
  };

  const [templateAddress] = await getTemplateAddress(payer.address, templateId);
  if (action === 'upload') {
    const upload = await buildKitTemplateUploadPlan({
      compiled: compiledEnsureUsdcAta,
      creator: payer.address,
      templateId,
      programAddress: BALLISTA_ADDRESS,
    });
    for (const item of upload.instructions) {
      const signature = await sendInstruction(item.instruction);
      console.log(`${item.kind}: ${signature}`);
    }
    console.log(`template: ${upload.templateAddress}`);
    console.log(`payload bytes: ${compiledEnsureUsdcAta.bytes.length}`);
    return;
  }

  const { instruction, usdcAta } = await buildEnsureUsdcAtaRunInstruction({
    payer: payer.address,
    wallet,
    templateAddress,
  });
  const signature = await sendInstruction(instruction);
  console.log(`run: ${signature}`);
  console.log(`wallet: ${wallet}`);
  console.log(`usdc ata: ${usdcAta}`);
}

if (process.argv[1]?.endsWith('ensure-usdc-ata.ts')) {
  void main().catch((error: unknown) => {
    console.error(error);
    process.exitCode = 1;
  });
}
