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

// Stand-ins so the example runs as written: replace them with the swap and vault programs.
const SWAP_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const VAULT_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;

const receivedBalance = expression.accountData(account.fixed('receivedTokens'), 64, 'u64');

/** Swap, require at least `minimumOut` arrived, then deposit. */
export const swapThenDeposit = defineTemplate({
  inputs: {
    minimumOut: { type: 'u64' },
    swapData: { type: 'bytes', maxLength: 256 },
    depositData: { type: 'bytes', maxLength: 256 },
  },
  accounts: {
    swapProgram: { executable: true, address: SWAP_PROGRAM },
    vaultProgram: { executable: true, address: VAULT_PROGRAM },
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
    // `receivedBalance` is read again here, after the swap.
    step.require(
      expression.greaterThanOrEqual(
        expression.subtract(receivedBalance, expression.snapshot('before')),
        expression.input('minimumOut'),
      ),
    ),
    step.invoke({
      program: account.fixed('vaultProgram'),
      accounts: [
        { account: account.fixed('payer'), signer: true, writable: true },
        { account: account.fixed('pool'), signer: false, writable: true },
      ],
      data: [data.encode('bytes', expression.input('depositData'))],
    }),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { buildKitRunInstruction } from '@jac0xb/ballista/kit';

const SWAP_PROGRAM_ADDRESS = address('11111111111111111111111111111111'); // the same stand-ins
const VAULT_PROGRAM_ADDRESS = address('11111111111111111111111111111111');

/** `swapData` and `depositData` are the two instructions' data, built by your client. */
export function runSwapThenDeposit(run: {
  templateAddress: Address;
  payer: Address;
  pool: Address;
  receivedTokens: Address;
  minimumOut: bigint;
  swapData: Uint8Array;
  depositData: Uint8Array;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(swapThenDeposit),
    templateAddress: run.templateAddress,
    inputs: { minimumOut: run.minimumOut, swapData: run.swapData, depositData: run.depositData },
    accounts: {
      swapProgram: { address: SWAP_PROGRAM_ADDRESS },
      vaultProgram: { address: VAULT_PROGRAM_ADDRESS },
      payer: { address: run.payer },
      pool: { address: run.pool },
      receivedTokens: { address: run.receivedTokens },
    },
  });
}
// #endregion run
