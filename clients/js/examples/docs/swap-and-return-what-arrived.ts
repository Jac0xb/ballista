// #region template
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  data,
  defineTemplate,
  expression,
  step,
} from '@jac0xb/ballista';

// A stand-in so the example runs as written: replace it with the swap program.
const SWAP_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;

const receivedBalance = expression.accountData(account.fixed('receivedTokens'), 64, 'u64');

/** Swap, require at least `minimumOut` arrived, and return what arrived as a `u64`. */
export const swapAndReturnWhatArrived = defineTemplate({
  inputs: {
    minimumOut: { type: 'u64' },
    swapData: { type: 'bytes', maxLength: 256 },
  },
  accounts: {
    swapProgram: { executable: true, address: SWAP_PROGRAM },
    payer: { signer: true, writable: true },
    pool: { writable: true },
    receivedTokens: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 165 },
  },
  steps: [
    step.snapshot('before', receivedBalance),
    step.invoke({
      program: account.fixed('swapProgram'),
      accounts: [
        { account: account.fixed('payer'), signer: true, writable: true },
        { account: account.fixed('pool'), signer: false, writable: true },
      ],
      data: [data.encode('bytes', expression.input('swapData'))],
    }),
    step.let('received', expression.subtract(receivedBalance, expression.snapshot('before'))),
    step.require(expression.greaterThanOrEqual(expression.variable('received'), expression.input('minimumOut'))),
    // Last, after every call: a call would clear it.
    step.setReturnData([data.encode('u64', expression.variable('received'))]),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { buildKitRunInstruction } from '@jac0xb/ballista/kit';

const SWAP_PROGRAM_ADDRESS = address('11111111111111111111111111111111'); // the same stand-in

/** Run on its own, the template returns what arrived to whoever reads the transaction's return data. */
export function runSwapAndReturnWhatArrived(run: {
  templateAddress: Address;
  payer: Address;
  pool: Address;
  receivedTokens: Address;
  minimumOut: bigint;
  swapData: Uint8Array;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(swapAndReturnWhatArrived),
    templateAddress: run.templateAddress,
    inputs: { minimumOut: run.minimumOut, swapData: run.swapData },
    accounts: {
      swapProgram: { address: SWAP_PROGRAM_ADDRESS },
      payer: { address: run.payer },
      pool: { address: run.pool },
      receivedTokens: { address: run.receivedTokens },
    },
  });
}
// #endregion run
