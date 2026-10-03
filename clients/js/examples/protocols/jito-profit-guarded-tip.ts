/** Pay a Jito tip only from a Jupiter round trip's profit: docs/examples/protocols/jito-tip.md. */
// #region template
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

/** The route's platform fee account and rate are chosen by whoever builds the run: cap the rate. */
export const MAX_PLATFORM_FEE_BPS = 0n;

export const jitoProfitGuardedTip = defineTemplate({
  inputs: {
    /** The round trip's `route_plan`, as `joinRoundTrip` joined it. */
    routePlan: { type: 'bytes', maxLength: 512 },
    /** The route's `in_amount`. */
    inAmount: { type: 'u64' },
    /** The quote's `quoted_out_amount`. */
    quotedOutAmount: { type: 'u64' },
    /** The quote's `slippage_bps`. */
    slippageBps: { type: 'u64' },
    /** The quote's `platform_fee_bps`, at most `MAX_PLATFORM_FEE_BPS`. */
    platformFeeBps: { type: 'u64' },
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

    // The fee account sits in the route's own accounts: any nonzero rate pays whoever chose it.
    step.require(
      expression.lessThanOrEqual(expression.input('platformFeeBps'), expression.u64(MAX_PLATFORM_FEE_BPS)),
      'platformFeeWithinCap',
    ),
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
      data: [
        data.literal(JUPITER_ROUTE),
        data.encode('bytes', expression.input('routePlan')),
        data.encode('u64', expression.input('inAmount')),
        data.encode('u64', expression.input('quotedOutAmount')),
        data.encode('u16', expression.input('slippageBps')),
        data.encode('u8', expression.input('platformFeeBps')),
      ],
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
// #endregion template

export const compiled = compileTemplate(jitoProfitGuardedTip);
