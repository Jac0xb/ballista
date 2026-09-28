// #region template
import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  defineTemplate,
  expression,
  step,
  tokenTransfer,
} from '../../src/index.js';

/** Move each row's whole token balance into the vault, skipping empty accounts. */
export const consolidateOnlyTheFundedAccounts = defineTemplate({
  accounts: {
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    vault: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 },
    authority: { signer: true },
  },
  batch: {
    maxIterations: 8,
    minIterations: 1,
    row: { source: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 } },
  },
  steps: [
    step.forEach([
      step.let('amount', expression.accountData(account.iteration('source'), 64, 'u64')),
      tokenTransfer({
        tokenProgram: account.fixed('tokenProgram'),
        source: account.iteration('source'),
        destination: account.fixed('vault'),
        authority: account.fixed('authority'),
        amount: expression.variable('amount'),
        when: expression.greaterThan(expression.variable('amount'), expression.u64(0)),
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

/** Pass every candidate; the run skips the empty ones. */
export function runConsolidateOnlyTheFundedAccounts(run: {
  templateAddress: Address;
  vault: Address;
  authority: Address;
  sources: readonly Address[];
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(consolidateOnlyTheFundedAccounts),
    templateAddress: run.templateAddress,
    accounts: {
      tokenProgram: { address: TOKEN_PROGRAM },
      vault: { address: run.vault },
      authority: { address: run.authority },
    },
    batchRows: run.sources.map((source) => ({ source: { address: source } })),
  });
}
// #endregion run
