// #region template
import { getAddressEncoder } from '@solana/kit';

import {
  INSTRUCTION_RUN,
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  data,
  defineTemplate,
  expression,
  step,
} from '@jac0xb/ballista';
import { BALLISTA_ADDRESS } from '@jac0xb/ballista/kit';

// Stand-ins so the example runs as written: replace them with the swap and vault programs, the
// vault's deposit discriminator, and the inner template's address (`getTemplateAddress` gives it).
const SWAP_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const VAULT_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const DEPOSIT_DISCRIMINATOR = Uint8Array.of(2, 0, 0, 0);
const INNER_TEMPLATE = new Uint8Array(32).fill(7);

const BALLISTA = Uint8Array.from(getAddressEncoder().encode(BALLISTA_ADDRESS));

/** Run the inner swap template, then deposit exactly what it returned. */
export const nestedSwapThenDeposit = defineTemplate({
  inputs: {
    /** The inner run's data after the `run` tag. */
    innerRun: { type: 'bytes', maxLength: 512 },
  },
  accounts: {
    ballista: { executable: true, address: BALLISTA },
    innerTemplate: { address: INNER_TEMPLATE },
    swapProgram: { executable: true, address: SWAP_PROGRAM },
    vaultProgram: { executable: true, address: VAULT_PROGRAM },
    payer: { signer: true, writable: true },
    pool: { writable: true },
    receivedTokens: { writable: true },
  },
  steps: [
    step.invoke({
      program: account.fixed('ballista'),
      // The inner template, then the inner run's accounts in the order it declares them.
      accounts: [
        { account: account.fixed('innerTemplate'), signer: false, writable: false },
        { account: account.fixed('swapProgram'), signer: false, writable: false },
        { account: account.fixed('payer'), signer: true, writable: true },
        { account: account.fixed('pool'), signer: false, writable: true },
        { account: account.fixed('receivedTokens'), signer: false, writable: true },
      ],
      data: [data.literal(Uint8Array.of(INSTRUCTION_RUN)), data.encode('bytes', expression.input('innerRun'))],
    }),
    // Straight after the call.
    step.let('received', expression.returnData('u64')),
    step.invoke({
      program: account.fixed('vaultProgram'),
      accounts: [
        { account: account.fixed('payer'), signer: true, writable: true },
        { account: account.fixed('pool'), signer: false, writable: true },
      ],
      data: [data.literal(DEPOSIT_DISCRIMINATOR), data.encode('u64', expression.variable('received'))],
    }),
  ],
});
// #endregion template

// #region run
import { getAddressDecoder, type Address } from '@solana/kit';

import { compileTemplate, encodeRunInputs } from '@jac0xb/ballista';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '@jac0xb/ballista/kit';
import { swapAndReturnWhatArrived } from './swap-and-return-what-arrived.js';

// The same stand-ins as the template.
const INNER_TEMPLATE_ADDRESS = getAddressDecoder().decode(INNER_TEMPLATE);
const SWAP_PROGRAM_ADDRESS = SYSTEM_PROGRAM_ADDRESS;
const VAULT_PROGRAM_ADDRESS = SYSTEM_PROGRAM_ADDRESS;

/** `innerRun` is what the inner template's own run would carry: its inputs, encoded. */
export function runNestedSwapThenDeposit(run: {
  templateAddress: Address;
  payer: Address;
  pool: Address;
  receivedTokens: Address;
  minimumOut: bigint;
  swapData: Uint8Array;
}) {
  const innerRun = encodeRunInputs(compileTemplate(swapAndReturnWhatArrived), {
    minimumOut: run.minimumOut,
    swapData: run.swapData,
  });
  return buildKitRunInstruction({
    compiled: compileTemplate(nestedSwapThenDeposit),
    templateAddress: run.templateAddress,
    inputs: { innerRun },
    accounts: {
      ballista: { address: BALLISTA_ADDRESS },
      innerTemplate: { address: INNER_TEMPLATE_ADDRESS },
      swapProgram: { address: SWAP_PROGRAM_ADDRESS },
      vaultProgram: { address: VAULT_PROGRAM_ADDRESS },
      payer: { address: run.payer },
      pool: { address: run.pool },
      receivedTokens: { address: run.receivedTokens },
    },
  });
}
// #endregion run
