/**
 * Getting started, step 1 (`docs/guide/getting-started.md`): connect to the local validator, fund
 * new wallets, and send transactions. The other files in this directory use these helpers; the page
 * shows the region.
 */
// #region connect
import {
  airdropFactory,
  appendTransactionMessageInstructions,
  assertIsTransactionWithBlockhashLifetime,
  createSolanaRpc,
  createSolanaRpcSubscriptions,
  createTransactionMessage,
  generateKeyPairSigner,
  lamports,
  pipe,
  sendAndConfirmTransactionFactory,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signTransactionMessageWithSigners,
  type Instruction,
  type TransactionSigner,
} from '@solana/kit';

// The local validator's default RPC and WebSocket addresses.
const rpc = createSolanaRpc('http://127.0.0.1:8899');
const rpcSubscriptions = createSolanaRpcSubscriptions('ws://127.0.0.1:8900');
const airdrop = airdropFactory({ rpc, rpcSubscriptions });
const sendAndConfirm = sendAndConfirmTransactionFactory({ rpc, rpcSubscriptions });

// A new wallet with 1 SOL from the validator's faucet.
async function fundedSigner(): Promise<TransactionSigner> {
  const signer = await generateKeyPairSigner();
  await airdrop({
    recipientAddress: signer.address,
    lamports: lamports(1_000_000_000n),
    commitment: 'confirmed',
  });
  return signer;
}

// The transaction `send` builds, before its instructions: version 0, signed and paid for
// by `feePayer`.
async function emptyMessage(feePayer: TransactionSigner) {
  const { value: blockhash } = await rpc.getLatestBlockhash().send();
  return pipe(
    createTransactionMessage({ version: 0 }),
    (m) => setTransactionMessageFeePayerSigner(feePayer, m),
    (m) => setTransactionMessageLifetimeUsingBlockhash(blockhash, m),
  );
}

// Send instructions in one transaction that `feePayer` signs and pays for.
async function send(feePayer: TransactionSigner, instructions: Instruction[]) {
  const empty = await emptyMessage(feePayer);
  const message = appendTransactionMessageInstructions(instructions, empty);
  const transaction = await signTransactionMessageWithSigners(message);
  assertIsTransactionWithBlockhashLifetime(transaction);
  await sendAndConfirm(transaction, { commitment: 'confirmed' });
}
// #endregion connect

export { emptyMessage, fundedSigner, rpc, send };
