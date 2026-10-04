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
  // The caller picks the reserve, in lamports, on every run.
  inputs: { reserve: { type: 'u64' } },
  accounts: {
    // Must be exactly the System program, so a caller can't swap in another.
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    // The account swept. It signs, so only its owner can run the sweep.
    vault: { signer: true, writable: true },
    // Where the swept lamports go.
    destination: { writable: true },
  },
  steps: [
    // Read the vault's balance while the transaction runs.
    step.let('balance', expression.accountField(account.fixed('vault'), 'lamports')),
    // Stop the whole run unless there is something above the reserve.
    step.require(
      expression.greaterThan(expression.variable('balance'), expression.input('reserve')),
      'aboveReserve',
    ),
    // Send everything above the reserve: balance minus reserve.
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

// Compile to the bytes you upload in step 3.
const compiled = compileTemplate(sweep);
// #endregion define

export { compiled, sweep };
