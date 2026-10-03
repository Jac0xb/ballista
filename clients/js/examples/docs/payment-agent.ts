// #region template
import { account, defineTemplate, expression, rateLimit, step, systemTransfer } from '@jac0xb/ballista';

/** An agent pays at most 0.1 SOL at once and 1 SOL a day, and keeps 0.05 SOL in its wallet. */
export const paymentAgent = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  registries: { limits: { spent: 'u64', lastSpend: 'i64' } },
  accounts: {
    agent: { signer: true, writable: true },
    recipient: { writable: true },
    agentLimit: account.registry('limits', { key: expression.accountKey('agent'), payer: 'agent' }),
    systemProgram: account.systemProgram(),
  },
  steps: [
    step.require(
      expression.lessThanOrEqual(expression.input('amount'), expression.u64(100_000_000)), // 0.1 SOL
      'perPaymentCap',
    ),
    ...rateLimit({
      registry: 'agentLimit', // the entry's account, not the registry
      name: 'dailyCap', // labels the check `withinDailyCap`
      cap: expression.u64(1_000_000_000), // 1 SOL
      refillPerSecond: expression.u64(11_574), // 1 SOL over 86,400 seconds, rounded down
      amount: expression.input('amount'),
    }),
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('agent'),
      to: account.fixed('recipient'),
      lamports: expression.input('amount'),
    }),
    step.require(
      expression.greaterThanOrEqual(
        expression.accountField(account.fixed('agent'), 'lamports'),
        expression.u64(50_000_000), // 0.05 SOL
      ),
      'keepsReserve',
    ),
  ],
});
// #endregion template

// #region run
import { type Address } from '@solana/kit';

import { compileTemplate, registryIndex } from '@jac0xb/ballista';
import { SYSTEM_PROGRAM_ADDRESS, buildKitRunInstruction, findRegistryEntryAddress } from '@jac0xb/ballista/kit';

export async function runPaymentAgent(run: {
  templateAddress: Address;
  agent: Address;
  recipient: Address;
  amount: bigint;
}) {
  const compiled = compileTemplate(paymentAgent);
  // The agent's entry: registry `limits`, keyed by the agent's address, as the template keys it.
  const [agentLimit] = await findRegistryEntryAddress(
    run.templateAddress,
    registryIndex(compiled, 'limits'),
    run.agent,
  );
  return buildKitRunInstruction({
    compiled,
    templateAddress: run.templateAddress,
    inputs: { amount: run.amount },
    accounts: {
      agent: { address: run.agent },
      recipient: { address: run.recipient },
      agentLimit: { address: agentLimit },
      systemProgram: { address: SYSTEM_PROGRAM_ADDRESS },
    },
  });
}
// #endregion run
