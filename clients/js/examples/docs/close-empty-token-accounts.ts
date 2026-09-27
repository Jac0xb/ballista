// #region template
import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  data,
  defineTemplate,
  expression,
  step,
} from '../../src/index.js';

/** SPL Token `CloseAccount`: a single discriminator byte, no arguments. */
const CLOSE_ACCOUNT = Uint8Array.of(9);

/** Close each row's token account whose balance is zero; skip the others. */
export const closeEmptyTokenAccounts = defineTemplate({
  accounts: {
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    rentDestination: { writable: true },
    authority: { signer: true },
  },
  batch: {
    maxIterations: 16,
    minIterations: 1,
    row: { tokenAccount: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 } },
  },
  steps: [
    step.forEach([
      step.invoke({
        program: account.fixed('tokenProgram'),
        programAddress: TOKEN_PROGRAM_ADDRESS_BYTES,
        accounts: [
          { account: account.iteration('tokenAccount'), writable: true, signer: false },
          { account: account.fixed('rentDestination'), writable: true, signer: false },
          { account: account.fixed('authority'), writable: false, signer: true },
        ],
        data: [data.literal(CLOSE_ACCOUNT)],
        when: expression.equal(expression.accountData(account.iteration('tokenAccount'), 64, 'u64'), expression.u64(0)),
      }),
    ]),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { buildKitRunInstruction } from '../../src/kit.js';

const TOKEN_PROGRAM = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');

/** Pass every candidate; the run closes only those holding zero tokens. */
export function runCloseEmptyTokenAccounts(run: {
  templateAddress: Address;
  rentDestination: Address;
  authority: Address;
  tokenAccounts: readonly Address[];
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(closeEmptyTokenAccounts),
    templateAddress: run.templateAddress,
    accounts: {
      tokenProgram: { address: TOKEN_PROGRAM },
      rentDestination: { address: run.rentDestination },
      authority: { address: run.authority },
    },
    batchRows: run.tokenAccounts.map((tokenAccount) => ({ tokenAccount: { address: tokenAccount } })),
  });
}
// #endregion run
