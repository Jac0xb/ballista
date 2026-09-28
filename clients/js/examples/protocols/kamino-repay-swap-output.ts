/**
 * Deleverage: swap collateral into the borrowed asset and repay exactly what the swap produced.
 *
 * `repay_obligation_liquidity_v2(liquidity_amount: u64)` takes the amount as an argument, and the
 * amount worth repaying is what the swap returns, which nobody knows at signing.
 *
 * The template swaps, measures what landed in the borrowed-asset account, and repays exactly
 * that.
 *
 * The run names that account, and Kamino repays from any account the borrower may spend,
 * including one whose owner approved the borrower as a delegate. It repays at most the debt, and
 * the rest of the swap stays in the account. So the borrower must own it (`swapPaysTheBorrower`).
 *
 * The repayment is Kamino's `_v2` handler: the v1 handler refuses every caller but Kamino itself
 * and a short whitelist. v2 takes the 9 accounts declared below, then `farmAccounts`, its tail:
 * - the obligation's farm user state and the reserve's debt farm, or the Kamino program for each
 *   when the reserve has no debt farm (the main market's SOL and USDC reserves have none);
 * - the lending market authority;
 * - the Farms program.
 * A group carries them so each keeps its own writable flag.
 *
 * Kamino takes a repayment only against a reserve and an obligation refreshed in the same slot,
 * and does not care where in the transaction that happened. Put `refresh_reserve` for each
 * reserve the obligation holds, then `refresh_obligation` with them, before this run. The swap in
 * between does not touch Kamino.
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
  KAMINO_REPAY,
  SYSVAR_INSTRUCTIONS,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  TOKEN_ACCOUNT_OWNER_OFFSET,
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
    instructionsSysvar: { address: addressBytes(SYSVAR_INSTRUCTIONS) },
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
    reserveLiquidityMint: {},
    reserveLiquiditySupply: { writable: true },
  },
  /**
   * `routeAccounts`: Jupiter's own list, whose length depends on the route. `farmAccounts`:
   * Kamino's v2 tail, described above.
   */
  accountGroups: ['routeAccounts', 'farmAccounts'],
  steps: [
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

    step.invoke({
      program: account.fixed('kamino'),
      accounts: [
        { account: account.fixed('borrower'), signer: true, writable: true },
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
