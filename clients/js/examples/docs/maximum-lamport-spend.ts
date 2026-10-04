// #region template
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, data, defineTemplate, expression, step } from '@jac0xb/ballista';

// Stand-ins so the example runs as written: replace them with the protocol call to protect.
const PROTOCOL_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const INSTRUCTION_DISCRIMINATOR = Uint8Array.of(2, 0, 0, 0);
const INSTRUCTION_ARGUMENT = 10_000n;

/** Make the call, then fail the run if the payer's balance fell by more than `maximumSpend`. */
export const maximumLamportSpend = defineTemplate({
  inputs: { maximumSpend: { type: 'u64' } },
  accounts: {
    protocolProgram: { executable: true, address: PROTOCOL_PROGRAM },
    payer: { signer: true, writable: true },
    pool: { writable: true },
  },
  steps: [
    step.snapshot('before', expression.accountField(account.fixed('payer'), 'lamports')),
    step.invoke({
      program: account.fixed('protocolProgram'),
      accounts: [
        { account: account.fixed('payer'), signer: true, writable: true },
        { account: account.fixed('pool'), signer: false, writable: true },
      ],
      data: [data.literal(INSTRUCTION_DISCRIMINATOR), data.encode('u64', expression.u64(INSTRUCTION_ARGUMENT))],
    }),
    step.require(
      expression.lessThanOrEqual(
        expression.subtract(expression.snapshot('before'), expression.accountField(account.fixed('payer'), 'lamports')),
        expression.input('maximumSpend'),
      ),
    ),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { buildKitRunInstruction } from '@jac0xb/ballista/kit';

const PROTOCOL_PROGRAM_ADDRESS = address('11111111111111111111111111111111'); // the same stand-in

export function runMaximumLamportSpend(run: {
  templateAddress: Address;
  payer: Address;
  pool: Address;
  maximumSpend: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(maximumLamportSpend),
    templateAddress: run.templateAddress,
    inputs: { maximumSpend: run.maximumSpend },
    accounts: {
      protocolProgram: { address: PROTOCOL_PROGRAM_ADDRESS },
      payer: { address: run.payer },
      pool: { address: run.pool },
    },
  });
}
// #endregion run
