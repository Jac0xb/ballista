// #region template
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, defineTemplate, expression, systemTransfer } from '@jac0xb/ballista';

/** Send `topUp` lamports to the bot only while its balance is below `floor`. */
export const topUpOnlyWhenLow = defineTemplate({
  inputs: { floor: { type: 'u64' }, topUp: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    funder: { signer: true, writable: true },
    bot: { writable: true },
  },
  steps: [
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('funder'),
      to: account.fixed('bot'),
      lamports: expression.input('topUp'),
      when: expression.lessThan(expression.accountField(account.fixed('bot'), 'lamports'), expression.input('floor')),
    }),
  ],
});
// #endregion template

// #region run
import type { Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction } from '@jac0xb/ballista/kit';

export function runTopUpOnlyWhenLow(run: {
  templateAddress: Address;
  funder: Address;
  bot: Address;
  floor: bigint;
  topUp: bigint;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(topUpOnlyWhenLow),
    templateAddress: run.templateAddress,
    inputs: { floor: run.floor, topUp: run.topUp },
    accounts: {
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
      funder: { address: run.funder },
      bot: { address: run.bot },
    },
  });
}
// #endregion run
