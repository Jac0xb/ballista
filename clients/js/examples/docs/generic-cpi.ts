// #region template
import { account, data, defineTemplate, expression, step } from '@jac0xb/ballista';

// Placeholders: replace them with your program's address and its instruction discriminator.
const MY_PROGRAM = new Uint8Array(32).fill(7);
const MY_DISCRIMINATOR = Uint8Array.of(1, 2, 3, 4, 5, 6, 7, 8);

/** A CPI built from parts: literal bytes, an encoded `u64`, and caller bytes, only when `enabled`. */
export const genericCpi = defineTemplate({
  inputs: {
    amount: { type: 'u64' },
    clientPayload: { type: 'bytes', maxLength: 128 },
    enabled: { type: 'bool' },
  },
  accounts: {
    program: { executable: true, address: MY_PROGRAM },
    vault: { writable: true },
    authority: { signer: true },
  },
  steps: [
    step.invoke({
      program: account.fixed('program'),
      accounts: [
        { account: account.fixed('vault'), writable: true, signer: false },
        { account: account.fixed('authority'), writable: false, signer: true },
      ],
      data: [
        data.literal(MY_DISCRIMINATOR),
        data.encode('u64', expression.input('amount')),
        data.encode('bytes', expression.input('clientPayload')),
      ],
      when: expression.input('enabled'),
    }),
  ],
});
// #endregion template

// #region run
import { getAddressDecoder, type Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { buildKitRunInstruction } from '@jac0xb/ballista/kit';

const MY_PROGRAM_ADDRESS = getAddressDecoder().decode(new Uint8Array(32).fill(7)); // the placeholder

export function runGenericCpi(run: {
  templateAddress: Address;
  vault: Address;
  authority: Address;
  amount: bigint;
  clientPayload: Uint8Array;
  enabled: boolean;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(genericCpi),
    templateAddress: run.templateAddress,
    inputs: { amount: run.amount, clientPayload: run.clientPayload, enabled: run.enabled },
    accounts: {
      program: { address: MY_PROGRAM_ADDRESS },
      vault: { address: run.vault },
      authority: { address: run.authority },
    },
  });
}
// #endregion run
