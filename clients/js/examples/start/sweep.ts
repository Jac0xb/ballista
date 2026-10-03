/**
 * Getting started, step 2: the sweep template. It compiles to the same bytes as the guide's
 * `sweep-above-a-reserve` example and as the Rust builder's `define` region in
 * `clients/rust/examples/docs_start.rs`; `start.test.ts` and `clients/rust/tests/docs_start.rs`
 * check both.
 */
// #region define
import {
  account,
  compileTemplate,
  defineTemplate,
  expression,
  step,
  systemTransfer,
  SYSTEM_PROGRAM_ADDRESS_BYTES,
} from '@jac0xb/ballista';

const sweep = defineTemplate({
  inputs: { reserve: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    vault: { signer: true, writable: true },
    destination: { writable: true },
  },
  steps: [
    step.let('balance', expression.accountField(account.fixed('vault'), 'lamports')),
    step.require(
      expression.greaterThan(expression.variable('balance'), expression.input('reserve')),
      'aboveReserve',
    ),
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('vault'),
      to: account.fixed('destination'),
      lamports: expression.subtract(
        expression.variable('balance'),
        expression.input('reserve'),
      ),
    }),
  ],
});

const compiled = compileTemplate(sweep);
// #endregion define

export { compiled, sweep };
