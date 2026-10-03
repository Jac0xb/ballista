// #region template
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, data, defineTemplate, expression, step } from '../../src/index.js';

// Stand-ins so the example runs as written: replace them with your lending program's address,
// its repay discriminator, and the offset of the debt in its loan account.
const LENDING_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const REPAY_DISCRIMINATOR = Uint8Array.of(2, 0, 0, 0);
const DEBT_OFFSET = 8;

/** Repay the debt read from the loan account, capped by the borrower's balance. */
export const repayExactlyWhatIsOwed = defineTemplate({
  accounts: {
    lendingProgram: { executable: true, address: LENDING_PROGRAM },
    loan: { owner: LENDING_PROGRAM, minDataLength: 128 },
    borrower: { signer: true, writable: true },
    pool: { writable: true },
  },
  steps: [
    step.let('owed', expression.accountData(account.fixed('loan'), DEBT_OFFSET, 'u64')),
    step.let('available', expression.accountField(account.fixed('borrower'), 'lamports')),
    step.invoke({
      program: account.fixed('lendingProgram'),
      accounts: [
        { account: account.fixed('borrower'), signer: true, writable: true },
        { account: account.fixed('pool'), signer: false, writable: true },
      ],
      data: [
        data.literal(REPAY_DISCRIMINATOR),
        data.encode('u64', expression.min(expression.variable('owed'), expression.variable('available'))),
      ],
    }),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { buildKitRunInstruction } from '../../src/kit.js';

const LENDING_PROGRAM_ADDRESS = address('11111111111111111111111111111111'); // the same stand-in

/** No inputs: the template reads the debt and the borrower's balance itself. */
export function runRepayExactlyWhatIsOwed(run: {
  templateAddress: Address;
  loan: Address;
  borrower: Address;
  pool: Address;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(repayExactlyWhatIsOwed),
    templateAddress: run.templateAddress,
    accounts: {
      lendingProgram: { address: LENDING_PROGRAM_ADDRESS },
      loan: { address: run.loan },
      borrower: { address: run.borrower },
      pool: { address: run.pool },
    },
  });
}
// #endregion run
