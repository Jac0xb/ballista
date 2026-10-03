// #region template
import { ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES, account, assertAta, defineTemplate } from '../../src/index.js';

/** Require `destinationAta` to be the associated token account of the recipient and mint. */
export const assertRecipientAta = defineTemplate({
  accounts: {
    associatedTokenProgram: { executable: true, address: ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES },
    tokenProgram: {},
    recipient: {},
    mint: {},
    destinationAta: { writable: true },
  },
  steps: [
    assertAta({
      associatedTokenAccount: account.fixed('destinationAta'),
      owner: account.fixed('recipient'),
      mint: account.fixed('mint'),
      tokenProgram: account.fixed('tokenProgram'),
      associatedTokenProgram: account.fixed('associatedTokenProgram'),
    }),
  ],
});
// #endregion template

// #region run
import { address, getAddressEncoder, getProgramDerivedAddress, type Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { buildKitRunInstruction } from '../../src/kit.js';

const ASSOCIATED_TOKEN_PROGRAM = address('ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL');

/** Derive the ATA as the template does, and pass the accounts in the order it declares them. */
export async function runAssertRecipientAta(run: {
  templateAddress: Address;
  recipient: Address;
  mint: Address;
  tokenProgram: Address;
}) {
  const encoder = getAddressEncoder();
  const [destinationAta] = await getProgramDerivedAddress({
    programAddress: ASSOCIATED_TOKEN_PROGRAM,
    seeds: [encoder.encode(run.recipient), encoder.encode(run.tokenProgram), encoder.encode(run.mint)],
  });
  return buildKitRunInstruction({
    compiled: compileTemplate(assertRecipientAta),
    templateAddress: run.templateAddress,
    accounts: {
      associatedTokenProgram: { address: ASSOCIATED_TOKEN_PROGRAM },
      tokenProgram: { address: run.tokenProgram },
      recipient: { address: run.recipient },
      mint: { address: run.mint },
      destinationAta: { address: destinationAta },
    },
  });
}
// #endregion run
