// #region template
import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  defineTemplate,
  expression,
  step,
  tokenTransfer,
} from '../../src/index.js';

/** Transfer `amount` tokens, then require the source fell by exactly that much. */
export const exactTokenDebit = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  accounts: {
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    source: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 },
    destination: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 },
    authority: { signer: true },
  },
  steps: [
    step.snapshot('before', expression.accountData(account.fixed('source'), 64, 'u64')),
    tokenTransfer({
      tokenProgram: account.fixed('tokenProgram'),
      source: account.fixed('source'),
      destination: account.fixed('destination'),
      authority: account.fixed('authority'),
      amount: expression.input('amount'),
    }),
    step.require(
      expression.equal(
        expression.accountData(account.fixed('source'), 64, 'u64'),
        expression.subtract(expression.snapshot('before'), expression.input('amount')),
      ),
    ),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { buildKitRunInstruction } from '../../src/kit.js';

const TOKEN_PROGRAM = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');

export function runExactTokenDebit(run: {
  templateAddress: Address;
  source: Address;
  destination: Address;
  authority: Address;
  amount: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(exactTokenDebit),
    templateAddress: run.templateAddress,
    inputs: { amount: run.amount },
    accounts: {
      tokenProgram: { address: TOKEN_PROGRAM },
      source: { address: run.source },
      destination: { address: run.destination },
      authority: { address: run.authority },
    },
  });
}
// #endregion run
