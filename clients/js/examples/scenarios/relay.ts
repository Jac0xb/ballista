/**
 * Two small templates for scenario B's edges. Test-only; see `split-sell.ts`.
 *
 * `returnClaim` returns whatever number it is given. It is what a tampered inner result looks
 * like: a Ballista run, so its return data passes the runtime's program check, claiming a total
 * nothing measured.
 *
 * `ballistaRelay` runs the template at `next` with `nextRun` as its run data, forwarding the
 * `nextAccounts` group as that run's accounts, then returns the `u64` the run returned. Relays
 * stacked on a `returnClaim` add one call frame each, which finds the call-depth limit without
 * any other program in the way.
 */
import { BALLISTA_PROGRAM_ADDRESS, INSTRUCTION_RUN, account, data, defineTemplate, expression, step } from '@jac0xb/ballista';
import { addressBytes } from '../protocols/shared.js';

const ballista = addressBytes(BALLISTA_PROGRAM_ADDRESS);

export const returnClaim = defineTemplate({
  inputs: { claimed: { type: 'u64' } },
  accounts: {},
  steps: [step.setReturnData([data.encode('u64', expression.input('claimed'))], 'returnTheClaim')],
});

export const ballistaRelay = defineTemplate({
  inputs: {
    /** The next run's data after the `run` tag. */
    nextRun: { type: 'bytes', maxLength: 512 },
  },
  accounts: {
    ballista: { executable: true, address: ballista },
    /** The template to run. Unpinned: a probe runs whatever it is pointed at. */
    next: {},
  },
  accountGroups: ['nextAccounts'],
  steps: [
    step.invoke({
      program: account.fixed('ballista'),
      programAddress: ballista,
      accounts: [{ account: account.fixed('next'), signer: false, writable: false }],
      accountGroup: 'nextAccounts',
      data: [data.literal(Uint8Array.of(INSTRUCTION_RUN)), data.encode('bytes', expression.input('nextRun'))],
      label: 'runNext',
    }),
    step.let('returned', expression.returnData('u64'), 'readNextResult'),
    step.setReturnData([data.encode('u64', expression.variable('returned'))], 'passTheResultUp'),
  ],
});
