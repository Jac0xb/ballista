/** Repay what a swap produced: docs/examples/protocols/kamino-repay.md. */
// #region template
import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  data,
  defineTemplate,
  expression,
  step,
} from '@jac0xb/ballista';
import {
  JUPITER_ROUTE,
  JUPITER_V6,
  KAMINO_LEND,
  KAMINO_REPAY,
  SYSVAR_INSTRUCTIONS,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  TOKEN_ACCOUNT_OWNER_OFFSET,
  addressBytes,
} from './shared.js';

/** The route's platform fee account and rate are chosen by whoever builds the run: cap the rate. */
export const MAX_PLATFORM_FEE_BPS = 0n;

export const kaminoRepaySwapOutput = defineTemplate({
  inputs: {
    /** `route_plan` as the Swap API encoded it: the bytes between the discriminator and `in_amount`. */
    routePlan: { type: 'bytes', maxLength: 512 },
    /** The route's `in_amount`. */
    inAmount: { type: 'u64' },
    /** The quote's `quoted_out_amount`. */
    quotedOutAmount: { type: 'u64' },
    /** The quote's `slippage_bps`. */
    slippageBps: { type: 'u64' },
    /** The quote's `platform_fee_bps`, at most `MAX_PLATFORM_FEE_BPS`. */
    platformFeeBps: { type: 'u64' },
    /** Repaying dust costs more in fees than it saves in interest. */
    minimumRepayment: { type: 'u64' },
  },
  accounts: {
    jupiter: { executable: true, address: addressBytes(JUPITER_V6) },
    kamino: { executable: true, address: addressBytes(KAMINO_LEND) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    instructionsSysvar: { address: addressBytes(SYSVAR_INSTRUCTIONS) },
    /** Signs the swap and the repayment; Kamino declares it a bare signer, so it is read-only. */
    borrower: { signer: true },
    /** The collateral the route sells. */
    collateralAta: { writable: true },
    /** Receives the swap output and funds the repayment. */
    borrowedAssetAta: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
    obligation: { writable: true },
    lendingMarket: {},
    repayReserve: { writable: true },
    reserveLiquidityMint: {},
    reserveLiquiditySupply: { writable: true },
  },
  /**
   * `routeAccounts`: Jupiter's own list, whose length depends on the route. `farmAccounts`: the end
   * of Kamino's v2 repayment, its own writable flags kept: the debt farm pair, the lending market
   * authority and the Farms program.
   */
  accountGroups: ['routeAccounts', 'farmAccounts'],
  steps: [
    // The swap pays into `borrowedAssetAta` and Kamino repays from it. Kamino accepts any account
    // the borrower may spend, even another owner's, and what the debt doesn't take stays there.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('borrowedAssetAta'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('borrower'), 'key'),
      ),
      'swapPaysTheBorrower',
    ),

    step.snapshot(
      'balanceBefore',
      expression.accountData(account.fixed('borrowedAssetAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
      'readBalanceBeforeSwap',
    ),

    // The fee account sits in the route's own accounts: any nonzero rate pays whoever chose it.
    step.require(
      expression.lessThanOrEqual(expression.input('platformFeeBps'), expression.u64(MAX_PLATFORM_FEE_BPS)),
      'platformFeeWithinCap',
    ),
    step.invoke({
      program: account.fixed('jupiter'),
      // `route` takes the token program, the signer, and the user's source and destination token
      // accounts first; the route's own accounts follow as the group.
      accounts: [
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('borrower'), signer: true, writable: false },
        { account: account.fixed('collateralAta'), signer: false, writable: true },
        { account: account.fixed('borrowedAssetAta'), signer: false, writable: true },
      ],
      accountGroup: 'routeAccounts',
      data: [
        data.literal(JUPITER_ROUTE),
        data.encode('bytes', expression.input('routePlan')),
        data.encode('u64', expression.input('inAmount')),
        data.encode('u64', expression.input('quotedOutAmount')),
        data.encode('u16', expression.input('slippageBps')),
        data.encode('u8', expression.input('platformFeeBps')),
      ],
      label: 'swapCollateralIntoDebtAsset',
    }),

    step.let(
      'swapped',
      expression.subtract(
        expression.accountData(account.fixed('borrowedAssetAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
        expression.snapshot('balanceBefore'),
      ),
      'measureSwapOutput',
    ),

    step.require(
      expression.greaterThanOrEqual(expression.variable('swapped'), expression.input('minimumRepayment')),
      'swapWorthRepaying',
    ),

    // v2: the v1 handler refuses every caller but Kamino itself and a short whitelist.
    step.invoke({
      program: account.fixed('kamino'),
      accounts: [
        { account: account.fixed('borrower'), signer: true, writable: false },
        { account: account.fixed('obligation'), signer: false, writable: true },
        { account: account.fixed('lendingMarket'), signer: false, writable: false },
        { account: account.fixed('repayReserve'), signer: false, writable: true },
        { account: account.fixed('reserveLiquidityMint'), signer: false, writable: false },
        { account: account.fixed('reserveLiquiditySupply'), signer: false, writable: true },
        // The repayment draws from the account the swap paid into.
        { account: account.fixed('borrowedAssetAta'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('instructionsSysvar'), signer: false, writable: false },
      ],
      accountGroup: 'farmAccounts',
      data: [
        data.literal(KAMINO_REPAY),
        // Exactly what the swap produced, measured a moment ago.
        data.encode('u64', expression.variable('swapped')),
      ],
      label: 'repayWhatTheSwapProduced',
    }),
  ],
});

export const compiled = compileTemplate(kaminoRepaySwapOutput);
// #endregion template
