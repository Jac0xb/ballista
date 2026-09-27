// #region template
import {
  ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES,
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  defineTemplate,
  ensureAssociatedTokenAccount,
} from '../../src/index.js';

/** Create the wallet's ATA with the ATA program's `Create`, only if it does not exist yet. */
export const conditionalAtaSetup = defineTemplate({
  accounts: {
    associatedTokenProgram: { executable: true, address: ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    mint: { owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 82 },
    payer: { signer: true, writable: true },
    wallet: {},
    ata: { writable: true },
  },
  steps: [
    ensureAssociatedTokenAccount({
      associatedTokenProgram: account.fixed('associatedTokenProgram'),
      payer: account.fixed('payer'),
      associatedTokenAccount: account.fixed('ata'),
      owner: account.fixed('wallet'),
      mint: account.fixed('mint'),
      systemProgram: account.fixed('systemProgram'),
      tokenProgram: account.fixed('tokenProgram'),
    }),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '../../src/kit.js';

const TOKEN_PROGRAM = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');
const ASSOCIATED_TOKEN_PROGRAM = address('ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL');

/** Re-sending this after the ATA exists skips `Create` instead of failing. */
export function runConditionalAtaSetup(run: {
  templateAddress: Address;
  mint: Address;
  payer: Address;
  wallet: Address;
  ata: Address;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(conditionalAtaSetup),
    templateAddress: run.templateAddress,
    accounts: {
      associatedTokenProgram: { address: ASSOCIATED_TOKEN_PROGRAM },
      tokenProgram: { address: TOKEN_PROGRAM },
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      mint: { address: run.mint },
      payer: { address: run.payer },
      wallet: { address: run.wallet },
      ata: { address: run.ata },
    },
  });
}
// #endregion run
