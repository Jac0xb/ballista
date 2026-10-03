// #region template
import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  defineTemplate,
  expression,
  step,
  tokenTransfer,
} from '../../src/index.js';

/** SPL Token account layout: the balance is the u64 at byte 64 of a 165-byte account. */
const TOKEN_ACCOUNT_AMOUNT_OFFSET = 64;
const TOKEN_ACCOUNT_LENGTH = 165;

/** Move a token account's entire balance, read during the run, to another token account. */
export const forwardTheWholeTokenBalance = defineTemplate({
  accounts: {
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    // Token-owned and 165 bytes or more: a token account, or a multisig the transfer refuses.
    source: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: TOKEN_ACCOUNT_LENGTH },
    destination: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: TOKEN_ACCOUNT_LENGTH },
    authority: { signer: true },
  },
  steps: [
    step.let('balance', expression.accountData(account.fixed('source'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64')),
    step.require(expression.greaterThan(expression.variable('balance'), expression.u64(0))),
    tokenTransfer({
      tokenProgram: account.fixed('tokenProgram'),
      source: account.fixed('source'),
      destination: account.fixed('destination'),
      authority: account.fixed('authority'),
      amount: expression.variable('balance'),
    }),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { buildKitRunInstruction } from '../../src/kit.js';

const TOKEN_PROGRAM = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');

/** No inputs: the amount is whatever `source` holds when the run executes. */
export function runForwardTheWholeTokenBalance(run: {
  templateAddress: Address;
  source: Address;
  destination: Address;
  authority: Address;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(forwardTheWholeTokenBalance),
    templateAddress: run.templateAddress,
    accounts: {
      tokenProgram: { address: TOKEN_PROGRAM },
      source: { address: run.source },
      destination: { address: run.destination },
      authority: { address: run.authority },
    },
  });
}
// #endregion run
