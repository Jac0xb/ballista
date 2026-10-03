/**
 * Pay a Jito tip only out of an arbitrage that actually worked.
 *
 * Jito's own advice is to keep the tip in the same transaction as the strategy, "this way if the
 * transaction fails, you don't pay the Jito tip". But a strategy that succeeds thinner than the
 * tip still pays it in full, and a transaction cannot compare its own profit to its own tip. This
 * template can: the strategy is a Jupiter `route` that starts and ends in the searcher's
 * wrapped-SOL account, measured instead of the searcher's lamports because `route` never moves
 * them: the Swap API wraps and unwraps SOL in instructions of its own, outside `route`. The
 * template refuses to pay the tip unless the balance grew by the tip plus `minimumEdge`.
 *
 * Jupiter's API refuses a quote whose input and output mints are the same, so a round trip is
 * quoted as two legs, which the caller joins into one `route` before calling this template
 * (`round_trip` in the test suite is the only implementation). Jupiter does not tie a route's
 * source and destination to the accounts its steps move, so the route must be built to start and
 * end in them regardless; only single-step legs can be joined this way, since a step's two indices
 * come after its percent, which comes after a `Swap` enum whose variants differ in length.
 *
 * The tip is a fixed run input, not a share of profit computed at run time: the block engine is
 * closed source, and it is unsettled whether a runtime-computed amount is scored at its simulated
 * value or read from the instruction. Bidding fixed and paying only when you earned it cannot
 * misbehave either way.
 */
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  data,
  defineTemplate,
  expression,
  step,
  systemTransfer,
} from '../../src/index.js';
import {
  JITO_TIP_PAYMENT,
  JUPITER_ROUTE,
  JUPITER_V6,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  TOKEN_ACCOUNT_MINT_OFFSET,
  TOKEN_ACCOUNT_OWNER_OFFSET,
  WRAPPED_SOL_MINT,
  addressBytes,
} from './shared.js';

const wrappedSolBalance = expression.accountData(account.fixed('wsolAccount'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64');

export const jitoProfitGuardedTip = defineTemplate({
  inputs: {
    /** The round trip's Jupiter `route` arguments: the instruction data after the discriminator. */
    strategyData: { type: 'bytes', maxLength: 512 },
    /** The bid, fixed before signing. Jito's floor is 1,000 lamports. */
    tipLamports: { type: 'u64' },
    /** What the searcher insists on keeping after the tip. */
    minimumEdge: { type: 'u64' },
  },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    strategyProgram: { executable: true, address: addressBytes(JUPITER_V6) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    searcher: { signer: true, writable: true },
    /** The searcher's wrapped-SOL token account, where the round trip starts and ends. */
    wsolAccount: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
    /** One of the eight Jito tip accounts, all of which the Tip Payment program owns. */
    jitoTip: { writable: true, owner: addressBytes(JITO_TIP_PAYMENT) },
  },
  accountGroups: ['strategyAccounts'],
  steps: [
    // Wrapped SOL is counted in lamports, so the profit is in the tip's own unit.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('wsolAccount'), TOKEN_ACCOUNT_MINT_OFFSET, 'pubkey'),
        expression.pubkey(addressBytes(WRAPPED_SOL_MINT)),
      ),
      'wsolAccountHoldsWrappedSol',
    ),
    // The searcher pays the tip, so only profit that reaches the searcher may cover it.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('wsolAccount'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('searcher'), 'key'),
      ),
      'searcherOwnsTheWsolAccount',
    ),

    step.snapshot('balanceBefore', wrappedSolBalance, 'readBalanceBeforeStrategy'),

    step.invoke({
      program: account.fixed('strategyProgram'),
      // `route` takes the token program, the signer, and the user's source and destination token
      // accounts first. A round trip's source and destination are both the account measured here;
      // the route's own accounts follow as the group.
      accounts: [
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('searcher'), signer: true, writable: false },
        { account: account.fixed('wsolAccount'), signer: false, writable: true },
        { account: account.fixed('wsolAccount'), signer: false, writable: true },
      ],
      accountGroup: 'strategyAccounts',
      data: [data.literal(JUPITER_ROUTE), data.encode('bytes', expression.input('strategyData'))],
      label: 'runStrategy',
    }),

    // Reverts the whole bundle if the opportunity evaporated, before any tip is paid. Adding to
    // the balance before, instead of subtracting it from the balance after, keeps a loss from
    // underflowing: it fails here like any profit too thin to cover the tip.
    step.require(
      expression.greaterThanOrEqual(
        wrappedSolBalance,
        expression.add(
          expression.add(expression.snapshot('balanceBefore'), expression.input('tipLamports')),
          expression.input('minimumEdge'),
        ),
      ),
      'profitCoversTheTip',
    ),

    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('searcher'),
      to: account.fixed('jitoTip'),
      lamports: expression.input('tipLamports'),
      label: 'payJitoTip',
    }),
  ],
});

export const compiled = compileTemplate(jitoProfitGuardedTip);
