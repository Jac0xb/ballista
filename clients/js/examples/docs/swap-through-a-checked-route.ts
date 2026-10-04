// #region template
import {
  TOKEN_2022_PROGRAM_ADDRESS_BYTES,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  data,
  defineTemplate,
  expression,
  step,
} from '@jac0xb/ballista';
import { JUPITER_V6, addressBytes } from '../protocols/shared.js';

/** One swap over a caller-supplied route that may hold none of the user's other token accounts. */
export const swapThroughACheckedRoute = defineTemplate({
  inputs: { route: { type: 'bytes', maxLength: 256 } },
  accounts: {
    jupiter: { executable: true, address: addressBytes(JUPITER_V6) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    user: { signer: true },
    source: { writable: true },
    destination: { writable: true },
  },
  accountGroups: ['amm'],
  steps: [
    step.require(
      expression.not(
        expression.groupAny('amm', {
          // A Token or Token-2022 account whose owner, the pubkey at byte 32, is the user...
          programs: [TOKEN_PROGRAM_ADDRESS_BYTES, TOKEN_2022_PROGRAM_ADDRESS_BYTES],
          match: [{ offset: 32, equals: expression.accountKey('user') }],
          // ...other than the two this swap is meant to touch.
          exceptKeys: [expression.accountKey('source'), expression.accountKey('destination')],
        }),
      ),
      'noOtherUserTokenAccount',
    ),
    step.invoke({
      program: account.fixed('jupiter'),
      accounts: [
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('user'), signer: true, writable: false },
        { account: account.fixed('source'), signer: false, writable: true },
        { account: account.fixed('destination'), signer: false, writable: true },
      ],
      accountGroup: 'amm',
      data: [data.encode('bytes', expression.input('route'))],
    }),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { buildKitRunInstruction, type KitAccountBinding } from '@jac0xb/ballista/kit';

const TOKEN_PROGRAM = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');

export function runSwapThroughACheckedRoute(run: {
  templateAddress: Address;
  user: Address;
  source: Address;
  destination: Address;
  /** The quote's route data and its pool accounts, `{ address, writable }` as the quote lists them. */
  route: Uint8Array;
  pools: readonly KitAccountBinding[];
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(swapThroughACheckedRoute),
    templateAddress: run.templateAddress,
    accounts: {
      jupiter: { address: address(JUPITER_V6) },
      tokenProgram: { address: TOKEN_PROGRAM },
      user: { address: run.user },
      source: { address: run.source },
      destination: { address: run.destination },
    },
    inputs: { route: run.route },
    accountGroups: { amm: run.pools },
  });
}
// #endregion run
