/**
 * Pay a Jito tip only out of an arbitrage that actually worked.
 *
 * Jito's own advice is to keep the tip in the same transaction as the strategy, "this way if the
 * transaction fails, you don't pay the Jito tip". That only helps when the strategy *fails*. An
 * arbitrage that succeeds but comes out thinner than the tip still pays the tip in full, and a
 * transaction cannot compare its own profit against its own tip.
 *
 * This template can. The strategy is a Jupiter `route` that starts and ends in the searcher's
 * wrapped-SOL account: a round trip such as SOL → USDC → SOL. The template reads that account's
 * balance, runs the route, and refuses to continue unless the balance grew by the tip plus a
 * margin. A run that misses the bar reverts before the tip is transferred.
 *
 * Profit is measured on the wrapped-SOL account, not on the searcher's lamports, because that is
 * where it lands. `route` moves token accounts only: the Swap API wraps SOL before it and unwraps
 * it after, in instructions of their own, so the searcher's lamports do not move while the route
 * runs. Wrapped SOL is counted in lamports, the tip's own unit, so the two compare without a
 * price.
 *
 * The check adds instead of subtracting: it requires `after ≥ before + tip + margin`. A round trip
 * that lost SOL fails that same requirement, rather than underflowing on the way to it.
 *
 * Jupiter's API refuses a quote whose input and output mints are the same, so a round trip is
 * quoted as two legs, SOL for USDC and then that output back to SOL, and joined into one `route`.
 * The second leg's plan steps follow the first's, renumbered to start from the first leg's output
 * index, so that the second leg spends exactly what the first produced; its accounts follow the
 * first leg's. Jupiter does not check that its steps move the source and destination accounts it
 * is given: it requires only that the source hold at least `in_amount` and that the destination
 * hold the output mint. So the route must be built to start and end in this account, and the
 * template relies only on the balance it measures.
 *
 * The tip amount is a run input, so it is decided before signing exactly like an ordinary tip.
 * Ballista could instead compute it as a share of realized profit — the tip is a plain SOL
 * transfer and Jito accepts one made by CPI — but the block engine is closed source and the
 * public write-ups disagree on whether a runtime-computed amount is scored at its simulated
 * value in the auction or read from the instruction. Until that is settled, bidding a fixed
 * amount and guaranteeing you only pay it when you earned it is the version that cannot
 * misbehave.
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
