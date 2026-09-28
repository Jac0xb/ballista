// #region template
import { TOKEN_PROGRAM_ADDRESS_BYTES, account, defineTemplate, expression, tokenTransfer } from '../../src/index.js';

/** One SPL Token transfer: the Token Program pinned, both token accounts pinned by owner and size. */
export const tokenTransferTemplate = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  accounts: {
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    authority: { signer: true },
    source: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 },
    destination: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 },
  },
  steps: [
    tokenTransfer({
      tokenProgram: account.fixed('tokenProgram'),
      source: account.fixed('source'),
      destination: account.fixed('destination'),
      authority: account.fixed('authority'),
      amount: expression.input('amount'),
    }),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { buildKitRunInstruction } from '../../src/kit.js';

const TOKEN_PROGRAM = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');

export function runTokenTransfer(run: {
  templateAddress: Address;
  authority: Address;
  source: Address;
  destination: Address;
  amount: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(tokenTransferTemplate),
    templateAddress: run.templateAddress,
    inputs: { amount: run.amount },
    accounts: {
      tokenProgram: { address: TOKEN_PROGRAM },
      authority: { address: run.authority },
      source: { address: run.source },
      destination: { address: run.destination },
    },
  });
}
// #endregion run
