// #region template
import { account, defineTemplate, expression, rateLimit, systemTransfer } from '@jac0xb/ballista';

/** Send SOL, at most 1 SOL at once per caller, refilling over about a day. */
export const dailyLimitPerCaller = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  registries: { limits: { spent: 'u64', lastSpend: 'i64' } },
  accounts: {
    caller: { signer: true, writable: true },
    recipient: { writable: true },
    callerLimit: account.registry('limits', { key: expression.accountKey('caller'), payer: 'caller' }),
    systemProgram: account.systemProgram(),
  },
  steps: [
    ...rateLimit({
      registry: 'callerLimit', // the entry's account, not the registry
      cap: expression.u64(1_000_000_000), // 1 SOL
      refillPerSecond: expression.u64(11_574), // 1 SOL over 86,400 seconds, rounded down
      amount: expression.input('amount'),
    }),
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('caller'),
      to: account.fixed('recipient'),
      lamports: expression.input('amount'),
    }),
  ],
});
// #endregion template

// #region run
import { type Address } from '@solana/kit';

import { compileTemplate, registryIndex } from '@jac0xb/ballista';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction, findRegistryEntryAddress } from '@jac0xb/ballista/kit';

export async function runDailyLimitPerCaller(run: {
  templateAddress: Address;
  caller: Address;
  recipient: Address;
  amount: bigint;
}) {
  const compiled = compileTemplate(dailyLimitPerCaller);
  // The caller's entry: registry `limits`, keyed by the caller's address, as the template keys it.
  const [callerLimit] = await findRegistryEntryAddress(
    run.templateAddress,
    registryIndex(compiled, 'limits'),
    run.caller,
  );
  return buildKitRunInstruction({
    compiled,
    templateAddress: run.templateAddress,
    inputs: { amount: run.amount },
    accounts: {
      caller: { address: run.caller },
      recipient: { address: run.recipient },
      callerLimit: { address: callerLimit },
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
    },
  });
}
// #endregion run
