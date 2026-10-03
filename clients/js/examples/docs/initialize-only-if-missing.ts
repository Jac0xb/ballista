// #region template
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, data, defineTemplate, expression, step } from '@jac0xb/ballista';

// Stand-ins so the example runs as written: replace them with your program's address and its
// initialize instruction data.
const PROTOCOL_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const INITIALIZE_DISCRIMINATOR = Uint8Array.of(2, 0, 0, 0);
const INITIALIZE_ARGUMENT = 10_000n;

/** Call the initialize instruction only when the position account holds no data yet. */
export const initializeOnlyIfMissing = defineTemplate({
  accounts: {
    protocolProgram: { executable: true, address: PROTOCOL_PROGRAM },
    payer: { signer: true, writable: true },
    position: { writable: true },
  },
  steps: [
    step.invoke({
      program: account.fixed('protocolProgram'),
      accounts: [
        { account: account.fixed('payer'), signer: true, writable: true },
        { account: account.fixed('position'), signer: false, writable: true },
      ],
      data: [data.literal(INITIALIZE_DISCRIMINATOR), data.encode('u64', expression.u64(INITIALIZE_ARGUMENT))],
      when: expression.accountField(account.fixed('position'), 'isEmpty'),
    }),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { buildKitRunInstruction } from '@jac0xb/ballista/kit';

const PROTOCOL_PROGRAM_ADDRESS = address('11111111111111111111111111111111'); // the same stand-in

/** The same instruction whether or not the position exists yet. */
export function runInitializeOnlyIfMissing(run: { templateAddress: Address; payer: Address; position: Address }) {
  return buildKitRunInstruction({
    compiled: compileTemplate(initializeOnlyIfMissing),
    templateAddress: run.templateAddress,
    accounts: {
      protocolProgram: { address: PROTOCOL_PROGRAM_ADDRESS },
      payer: { address: run.payer },
      position: { address: run.position },
    },
  });
}
// #endregion run
