// #region template
import { account, defineTemplate, expression, step } from '@jac0xb/ballista';

/** Count each caller's runs, in an entry of their own. */
export const countRuns = defineTemplate({
  // Each registry and its fields: `runs`, with one u64 field, `count`.
  registries: { runs: { count: 'u64' } },
  accounts: {
    caller: { signer: true, writable: true },
    // The account holding the caller's entry in `runs`. Before the first step, every run checks
    // it, or creates it.
    callerRuns: account.registry('runs', {
      // Keyed by the caller, who must sign, so a caller opens only their own entry.
      // Leave `key` out for one entry that every run shares.
      key: expression.accountKey('caller'),
      // Pays the rent when a run creates the entry, and nothing after that.
      payer: 'caller',
    }),
    // Creating an entry calls the System program.
    systemProgram: account.systemProgram(),
  },
  steps: [
    // Read and write name the entry's account, not the registry, so a template can open two
    // entries of one registry. The write lands at once; if the run fails, Solana undoes it.
    step.setRegistry(
      'callerRuns',
      'count',
      expression.add(expression.registry('callerRuns', 'count'), expression.u64(1)),
    ),
  ],
});
// #endregion template
