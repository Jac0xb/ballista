// #region template
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, data, defineTemplate, expression, step } from '../../src/index.js';

// Stand-ins so the example runs as written: replace them with your governance program's address
// and the offsets of the two fields in its proposal account.
const GOVERNANCE_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const APPROVED_OFFSET = 0;
const TIME_OFFSET = 8;

const approved = expression.accountData(account.fixed('proposal'), APPROVED_OFFSET, 'bool');
const executableAfter = expression.accountData(account.fixed('proposal'), TIME_OFFSET, 'i64');

/** Forward the execute instruction only once the proposal is approved and its time has come. */
export const timeGatedGovernanceExecution = defineTemplate({
  inputs: { executeData: { type: 'bytes', maxLength: 256 } },
  accounts: {
    governanceProgram: { executable: true, address: GOVERNANCE_PROGRAM },
    proposal: { owner: GOVERNANCE_PROGRAM, minDataLength: 128 },
    payer: { signer: true, writable: true },
    target: { writable: true },
  },
  steps: [
    step.require(
      expression.and(approved, expression.greaterThanOrEqual(expression.clockUnixTimestamp(), executableAfter)),
    ),
    step.invoke({
      program: account.fixed('governanceProgram'),
      accounts: [
        { account: account.fixed('payer'), signer: true, writable: true },
        { account: account.fixed('target'), signer: false, writable: true },
      ],
      data: [data.encode('bytes', expression.input('executeData'))],
    }),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '../../src/index.js';
import { buildKitRunInstruction } from '../../src/kit.js';

const GOVERNANCE_PROGRAM_ADDRESS = address('11111111111111111111111111111111'); // the same stand-in

export function runTimeGatedGovernanceExecution(run: {
  templateAddress: Address;
  proposal: Address;
  payer: Address;
  target: Address;
  executeData: Uint8Array;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(timeGatedGovernanceExecution),
    templateAddress: run.templateAddress,
    inputs: { executeData: run.executeData },
    accounts: {
      governanceProgram: { address: GOVERNANCE_PROGRAM_ADDRESS },
      proposal: { address: run.proposal },
      payer: { address: run.payer },
      target: { address: run.target },
    },
  });
}
// #endregion run
