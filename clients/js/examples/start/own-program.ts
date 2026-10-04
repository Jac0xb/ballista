/**
 * Getting started, step 6: call your own program. It compiles to the same bytes as the Rust
 * builder's `own-program` region in `clients/rust/examples/docs_start.rs`; `start.test.ts` writes
 * them to `clients/rust/tests/fixtures/own-program.hex` and `clients/rust/tests/docs_start.rs`
 * checks the Rust side against that file.
 */
// #region own-program
import { address } from '@solana/kit';

import {
  account,
  addressBytes,
  anchorDiscriminator,
  compileTemplate,
  data,
  defineTemplate,
  expression,
  step,
} from '@jac0xb/ballista';

// Your program's address. This one is a placeholder.
const MY_PROGRAM = address('MyProgram1111111111111111111111111111111111');

const deposit = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  accounts: {
    myProgram: { executable: true, address: addressBytes(MY_PROGRAM) },
    vault: { writable: true },
    authority: { signer: true },
  },
  steps: [
    step.invoke({
      program: account.fixed('myProgram'),
      accounts: [
        { account: account.fixed('vault'), writable: true, signer: false },
        { account: account.fixed('authority'), writable: false, signer: true },
      ],
      // An Anchor instruction's data: its 8-byte discriminator, then its arguments.
      data: [
        data.literal(anchorDiscriminator('deposit')),
        data.encode('u64', expression.input('amount')),
      ],
    }),
  ],
});

const compiled = compileTemplate(deposit);
// #endregion own-program

export { compiled, deposit, MY_PROGRAM };
