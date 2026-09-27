/**
 * Deleverage: swap collateral into the borrowed asset and repay exactly what the swap produced.
 *
 * `repay_obligation_liquidity_v2(liquidity_amount: u64)` takes the amount as an argument, and the
 * amount worth repaying is what the swap returns, which nobody knows at signing.
 *
 * The template swaps, measures what landed in the borrowed-asset account, and repays exactly
 * that. Kamino's `refresh_reserve` runs just before the repayment, because a repayment is priced
 * against a refreshed reserve.
 */
import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  data,
  defineTemplate,
  expression,
  step,
} from '../../src/index.js';
import {
  JUPITER_ROUTE,
  JUPITER_V6,
  KAMINO_LEND,
  KAMINO_REFRESH_RESERVE,
  KAMINO_REPAY,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  addressBytes,
} from './shared.js';

export const kaminoRepaySwapOutput = defineTemplate({
  inputs: {
    /** Jupiter's `route` arguments: the Swap API's instruction data after the discriminator. */
    routeArgs: { type: 'bytes', maxLength: 512 },
    /** Repaying dust costs more in fees than it saves in interest. */
    minimumRepayment: { type: 'u64' },
  },
  accounts: {
    jupiter: { executable: true, address: addressBytes(JUPITER_V6) },
    kamino: { executable: true, address: addressBytes(KAMINO_LEND) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    borrower: { signer: true, writable: true },
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
    reserveLiquiditySupply: { writable: true },
    reservePriceFeed: {},
  },
  accountGroups: ['routeAccounts'],
  steps: [
    step.snapshot(
      'balanceBefore',
      expression.accountData(account.fixed('borrowedAssetAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
      'readBalanceBeforeSwap',
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
      data: [data.literal(JUPITER_ROUTE), data.encode('bytes', expression.input('routeArgs'))],
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

    // Interest is priced off a refreshed reserve, so refresh inside the same transaction.
    step.invoke({
      program: account.fixed('kamino'),
      accounts: [
        { account: account.fixed('repayReserve'), signer: false, writable: true },
        { account: account.fixed('lendingMarket'), signer: false, writable: false },
        { account: account.fixed('reservePriceFeed'), signer: false, writable: false },
      ],
      data: [data.literal(KAMINO_REFRESH_RESERVE)],
      label: 'refreshReserve',
    }),

    step.invoke({
      program: account.fixed('kamino'),
      accounts: [
        { account: account.fixed('borrower'), signer: true, writable: true },
        { account: account.fixed('obligation'), signer: false, writable: true },
        { account: account.fixed('lendingMarket'), signer: false, writable: false },
        { account: account.fixed('repayReserve'), signer: false, writable: true },
        { account: account.fixed('reserveLiquiditySupply'), signer: false, writable: true },
        { account: account.fixed('borrowedAssetAta'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
      ],
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
