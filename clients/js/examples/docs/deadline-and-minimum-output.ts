// #region template
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, data, defineTemplate, expression, step } from '@jac0xb/ballista';

// Stand-in so the example runs as written: replace it with the swap program's address.
const SWAP_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;

/** Forward the client's swap only if the quote is unexpired and promises at least `minimumOut`. */
export const deadlineAndMinimumOutput = defineTemplate({
  inputs: {
    deadline: { type: 'i64' },
    quotedOut: { type: 'u64' },
    minimumOut: { type: 'u64' },
    routeData: { type: 'bytes', maxLength: 256 },
  },
  accounts: {
    swapProgram: { executable: true, address: SWAP_PROGRAM },
    payer: { signer: true, writable: true },
    pool: { writable: true },
  },
  steps: [
    step.require(
      expression.and(
        expression.lessThanOrEqual(expression.clockUnixTimestamp(), expression.input('deadline')),
        expression.greaterThanOrEqual(expression.input('quotedOut'), expression.input('minimumOut')),
      ),
    ),
    step.invoke({
      program: account.fixed('swapProgram'),
      accounts: [
        { account: account.fixed('payer'), signer: true, writable: true },
        { account: account.fixed('pool'), signer: false, writable: true },
      ],
      data: [data.encode('bytes', expression.input('routeData'))],
    }),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { buildKitRunInstruction } from '@jac0xb/ballista/kit';

const SWAP_PROGRAM_ADDRESS = address('11111111111111111111111111111111'); // the same stand-in

/** `routeData` is the swap instruction's data from your quote. */
export function runDeadlineAndMinimumOutput(run: {
  templateAddress: Address;
  payer: Address;
  pool: Address;
  deadline: bigint;
  quotedOut: bigint;
  minimumOut: bigint;
  routeData: Uint8Array;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(deadlineAndMinimumOutput),
    templateAddress: run.templateAddress,
    inputs: {
      deadline: run.deadline,
      quotedOut: run.quotedOut,
      minimumOut: run.minimumOut,
      routeData: run.routeData,
    },
    accounts: {
      swapProgram: { address: SWAP_PROGRAM_ADDRESS },
      payer: { address: run.payer },
      pool: { address: run.pool },
    },
  });
}
// #endregion run
