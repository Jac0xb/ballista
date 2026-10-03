// #region template
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, data, defineTemplate, expression, step } from '@jac0xb/ballista';

// Stand-ins so the example runs as written: replace them with the two routes' programs.
const PRIMARY_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const FALLBACK_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;

const usePrimary = expression.input('usePrimary');

/** Call exactly one of two routes, chosen by `usePrimary`. */
export const primaryOrFallbackRoute = defineTemplate({
  inputs: {
    usePrimary: { type: 'bool' },
    primaryData: { type: 'bytes', maxLength: 256 },
    fallbackData: { type: 'bytes', maxLength: 256 },
  },
  accounts: {
    primaryProgram: { executable: true, address: PRIMARY_PROGRAM },
    fallbackProgram: { executable: true, address: FALLBACK_PROGRAM },
    payer: { signer: true, writable: true },
    pool: { writable: true },
  },
  steps: [
    step.invoke({
      program: account.fixed('primaryProgram'),
      accounts: [
        { account: account.fixed('payer'), signer: true, writable: true },
        { account: account.fixed('pool'), signer: false, writable: true },
      ],
      data: [data.encode('bytes', expression.input('primaryData'))],
      when: usePrimary,
    }),
    step.invoke({
      program: account.fixed('fallbackProgram'),
      accounts: [
        { account: account.fixed('payer'), signer: true, writable: true },
        { account: account.fixed('pool'), signer: false, writable: true },
      ],
      data: [data.encode('bytes', expression.input('fallbackData'))],
      when: expression.not(usePrimary),
    }),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { buildKitRunInstruction } from '@jac0xb/ballista/kit';

const PRIMARY_PROGRAM_ADDRESS = address('11111111111111111111111111111111'); // the same stand-ins
const FALLBACK_PROGRAM_ADDRESS = address('11111111111111111111111111111111');

/** Both routes' data travel in every run; `usePrimary` picks which call happens. */
export function runPrimaryOrFallbackRoute(run: {
  templateAddress: Address;
  payer: Address;
  pool: Address;
  usePrimary: boolean;
  primaryData: Uint8Array;
  fallbackData: Uint8Array;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(primaryOrFallbackRoute),
    templateAddress: run.templateAddress,
    inputs: { usePrimary: run.usePrimary, primaryData: run.primaryData, fallbackData: run.fallbackData },
    accounts: {
      primaryProgram: { address: PRIMARY_PROGRAM_ADDRESS },
      fallbackProgram: { address: FALLBACK_PROGRAM_ADDRESS },
      payer: { address: run.payer },
      pool: { address: run.pool },
    },
  });
}
// #endregion run
