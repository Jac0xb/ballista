/**
 * Scenario B's outer run: a Ballista run that calls Ballista to run `splitSellInner`, reads the
 * total it returns, and pays the rows out of it. Test-only; see `split-sell.ts`.
 *
 * The runtime's return-data read takes the data only from the program the invoke just before it
 * called, here Ballista, and fails with `ReturnDataMismatch` otherwise; the template cannot read
 * that program itself. That says a Ballista run returned the data, not which template: any
 * template can return any number. So the inner template is pinned by address. A finalized
 * template cannot be rewritten or closed, so its address fixes its bytes.
 *
 * The outer run forwards the inner run's fixed accounts in the inner's own order, the seller as
 * the one signer, then its `routeAccounts` group as the inner run's group. `innerRun` is the
 * inner run's instruction data after the `run` tag: its group length, then its inputs.
 */
import {
  BALLISTA_PROGRAM_ADDRESS,
  INSTRUCTION_RUN,
  account,
  data,
  defineTemplate,
  expression,
  step,
} from '../../src/index.js';
import { addressBytes } from '../protocols/shared.js';
import { payoutBatch, payoutSteps, splitSellAccounts } from './split-sell.js';

/** The protocol tests' template creator: the key `keypair(b"ballista-protocol-tests-creator1")` makes. */
export const TEST_CREATOR = '2WGAww33k4mjMVckFknzzv62LcCSY7Q57mA75n1vwDmB';
/** The id the protocol tests upload `splitSellInner` under. */
export const INNER_TEMPLATE_ID = 31;
/** `splitSellInner` as the protocol tests upload it: the template address of `TEST_CREATOR` and `INNER_TEMPLATE_ID`. */
export const INNER_TEMPLATE_ADDRESS = 'HKA6yk5NTxosVJU5bchRvvf5D5eMrBu3ywjNE465RCDm';
/** ASCII `PAID`: the tag of the outer run's event. */
export const PAYOUT_EVENT_TAG = new TextEncoder().encode('PAID');

const ballista = addressBytes(BALLISTA_PROGRAM_ADDRESS);

export const nestedSplitSellPayout = defineTemplate({
  inputs: {
    /** `splitSellInner`'s run data after the `run` tag. */
    innerRun: { type: 'bytes', maxLength: 512 },
  },
  accounts: {
    ballista: { executable: true, address: ballista },
    innerTemplate: { address: addressBytes(INNER_TEMPLATE_ADDRESS) },
    ...splitSellAccounts,
  },
  batch: payoutBatch,
  accountGroups: ['routeAccounts'],
  steps: [
    step.invoke({
      program: account.fixed('ballista'),
      programAddress: ballista,
      accounts: [
        { account: account.fixed('innerTemplate'), signer: false, writable: false },
        ...Object.entries(splitSellAccounts).map(([name, constraint]) => ({
          account: account.fixed(name),
          signer: 'signer' in constraint && constraint.signer,
          writable: 'writable' in constraint && constraint.writable,
        })),
      ],
      accountGroup: 'routeAccounts',
      data: [data.literal(Uint8Array.of(INSTRUCTION_RUN)), data.encode('bytes', expression.input('innerRun'))],
      label: 'runInnerSplitSell',
    }),
    // Straight after the call: the verifier refuses a read anywhere else.
    step.let('innerTotal', expression.returnData('u64'), 'readInnerTotal'),
    ...payoutSteps({ source: 'destinationAta', payer: 'seller', bound: 'innerTotal', label: 'payoutsWithinInnerTotal' }),
    step.emit(
      [
        data.literal(PAYOUT_EVENT_TAG),
        data.encode('u64', expression.variable('innerTotal')),
        data.encode('u64', expression.variable('paid')),
      ],
      'logPayout',
    ),
  ],
});
