// #region template
import { account, defineTemplate, expression, step } from '@jac0xb/ballista';

/** Count each caller's runs, in an entry of their own. */
export const countRuns = defineTemplate({
  registries: { runs: { count: 'u64' } },
  accounts: {
    caller: { signer: true, writable: true },
    // The caller's entry in `runs`, keyed by the caller's address. The caller pays to create it.
    callerRuns: account.registry('runs', { key: expression.accountKey('caller'), payer: 'caller' }),
    // Creating an entry calls the System program.
    systemProgram: account.systemProgram(),
  },
  steps: [
    step.setRegistry(
      'callerRuns',
      'count',
      expression.add(expression.registry('callerRuns', 'count'), expression.u64(1)),
    ),
  ],
});
// #endregion template
