// #region template
import { SYSTEM_PROGRAM_ADDRESS_BYTES, account, data, defineTemplate, expression, step } from '@jac0xb/ballista';

// Stand-ins so the example runs as written: replace them with the rewards program's address, its
// claim instruction data, and the offset of the pending amount in its rewards account.
const REWARDS_PROGRAM = SYSTEM_PROGRAM_ADDRESS_BYTES;
const CLAIM_DISCRIMINATOR = Uint8Array.of(2, 0, 0, 0);
const CLAIM_ARGUMENT = 10_000n;
const PENDING_OFFSET = 8;

/** Call the claim instruction only when the rewards account shows a pending amount. */
export const claimOnlyWhenThereIsSomething = defineTemplate({
  accounts: {
    rewardsProgram: { executable: true, address: REWARDS_PROGRAM },
    rewards: { owner: REWARDS_PROGRAM, minDataLength: 128 },
    claimant: { signer: true, writable: true },
    destination: { writable: true },
  },
  steps: [
    step.invoke({
      program: account.fixed('rewardsProgram'),
      accounts: [
        { account: account.fixed('claimant'), signer: true, writable: true },
        { account: account.fixed('destination'), signer: false, writable: true },
      ],
      data: [data.literal(CLAIM_DISCRIMINATOR), data.encode('u64', expression.u64(CLAIM_ARGUMENT))],
      when: expression.greaterThan(
        expression.accountData(account.fixed('rewards'), PENDING_OFFSET, 'u64'),
        expression.u64(0),
      ),
    }),
  ],
});
// #endregion template

// #region run
import { address, type Address } from '@solana/kit';

import { compileTemplate } from '@jac0xb/ballista';
import { buildKitRunInstruction } from '@jac0xb/ballista/kit';

const REWARDS_PROGRAM_ADDRESS = address('11111111111111111111111111111111'); // the same stand-in

/** Safe to send on a schedule: with nothing pending, the run succeeds without calling claim. */
export function runClaimOnlyWhenThereIsSomething(run: {
  templateAddress: Address;
  rewards: Address;
  claimant: Address;
  destination: Address;
}) {
  return buildKitRunInstruction({
    compiled: compileTemplate(claimOnlyWhenThereIsSomething),
    templateAddress: run.templateAddress,
    accounts: {
      rewardsProgram: { address: REWARDS_PROGRAM_ADDRESS },
      rewards: { address: run.rewards },
      claimant: { address: run.claimant },
      destination: { address: run.destination },
    },
  });
}
// #endregion run
