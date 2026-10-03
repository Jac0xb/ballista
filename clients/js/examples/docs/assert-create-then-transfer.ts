// #region template
import {
  ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES,
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  assertAta,
  defineTemplate,
  ensureAssociatedTokenAccount,
  expression,
  step,
  tokenTransfer,
} from '@jac0xb/ballista';

/** For each row: prove the destination is the recipient's ATA, create it if missing, then pay. */
export const assertCreateThenTransfer = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  accounts: {
    associatedTokenProgram: { executable: true, address: ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    mint: { owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 82 },
    payer: { signer: true, writable: true },
    authority: { signer: true },
    source: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 },
  },
  batch: {
    maxIterations: 8,
    minIterations: 1,
    // Each row is two accounts: the recipient's wallet, then its ATA.
    row: { recipient: {}, destinationAta: { writable: true } },
  },
  steps: [
    step.forEach([
      assertAta({
        associatedTokenAccount: account.iteration('destinationAta'),
        owner: account.iteration('recipient'),
        mint: account.fixed('mint'),
        tokenProgram: account.fixed('tokenProgram'),
        associatedTokenProgram: account.fixed('associatedTokenProgram'),
      }),
      ensureAssociatedTokenAccount({
        associatedTokenProgram: account.fixed('associatedTokenProgram'),
        payer: account.fixed('payer'),
        associatedTokenAccount: account.iteration('destinationAta'),
        owner: account.iteration('recipient'),
        mint: account.fixed('mint'),
        systemProgram: account.fixed('systemProgram'),
        tokenProgram: account.fixed('tokenProgram'),
      }),
      tokenTransfer({
        tokenProgram: account.fixed('tokenProgram'),
        source: account.fixed('source'),
        destination: account.iteration('destinationAta'),
        authority: account.fixed('authority'),
        amount: expression.input('amount'),
      }),
    ]),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '@jac0xb/ballista/kit';

const TOKEN_PROGRAM = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');
const ASSOCIATED_TOKEN_PROGRAM = address('ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL');

/** `recipients` pairs each wallet with its ATA for `mint`. */
export function runAssertCreateThenTransfer(run: {
  templateAddress: Address;
  mint: Address;
  payer: Address;
  authority: Address;
  source: Address;
  recipients: readonly { wallet: Address; ata: Address }[];
  amount: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(assertCreateThenTransfer),
    templateAddress: run.templateAddress,
    inputs: { amount: run.amount },
    accounts: {
      associatedTokenProgram: { address: ASSOCIATED_TOKEN_PROGRAM },
      tokenProgram: { address: TOKEN_PROGRAM },
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      mint: { address: run.mint },
      payer: { address: run.payer },
      authority: { address: run.authority },
      source: { address: run.source },
    },
    batchRows: run.recipients.map((recipient) => ({
      recipient: { address: recipient.wallet },
      destinationAta: { address: recipient.ata },
    })),
  });
}
// #endregion run
