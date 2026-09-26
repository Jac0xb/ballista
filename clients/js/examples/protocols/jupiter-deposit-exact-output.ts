/**
 * Deposit into Kamino exactly what a Jupiter swap produced.
 *
 * Jupiter v6's `route` carries the input amount, the *quoted* output, `slippageBps` and
 * `platformFeeBps`. What actually comes out is reported as an Anchor `SwapEvent` emitted through
 * a self-CPI — an event, not return data — so a caller cannot read it back with
 * `get_return_data`. The destination token account is the only reliable source, and it can only
 * be read after the route has run.
 *
 * Kamino's `deposit_reserve_liquidity_and_obligation_collateral_v2(liquidity_amount: u64)` needs
 * that number. A plain transaction has to write it before the swap has happened: quote it high
 * and the deposit fails, quote it low and the remainder is stranded in the ATA.
 *
 * Route account lists vary in length, so the route's accounts arrive as an account group rather
 * than as declared schema slots. Group members are forwarded with the transaction's own writable
 * flag and never sign.
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
  JUPITER_V6,
  KAMINO_DEPOSIT,
  KAMINO_LEND,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  addressBytes,
} from './shared.js';

export const jupiterDepositExactOutput = defineTemplate({
  inputs: {
    /** The whole Jupiter instruction, as returned by the Swap API. */
    routeData: { type: 'bytes', maxLength: 512 },
    /** Below this the route is not worth depositing and the run fails instead. */
    minimumOut: { type: 'u64' },
  },
  accounts: {
    jupiter: { executable: true, address: addressBytes(JUPITER_V6) },
    kamino: { executable: true, address: addressBytes(KAMINO_LEND) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    owner: { signer: true, writable: true },
    /** The route's destination, and the account the deposit draws from. */
    destinationAta: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
    obligation: { writable: true },
    lendingMarket: {},
    lendingMarketAuthority: {},
    reserve: { writable: true },
    reserveLiquiditySupply: { writable: true },
    reserveCollateralMint: { writable: true },
    reserveDestinationDepositCollateral: { writable: true },
  },
  /** Jupiter's own account list, whose length depends on the route the API returned. */
  accountGroups: ['routeAccounts'],
  steps: [
    step.snapshot(
      'balanceBefore',
      expression.accountData(account.fixed('destinationAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
      'readBalanceBeforeSwap',
    ),

    step.invoke({
      program: account.fixed('jupiter'),
      accounts: [{ account: account.fixed('owner'), signer: true, writable: true }],
      accountGroup: 'routeAccounts',
      data: [data.encode('bytes', expression.input('routeData'))],
      label: 'swap',
    }),

    step.let(
      'received',
      expression.subtract(
        expression.accountData(account.fixed('destinationAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
        expression.snapshot('balanceBefore'),
      ),
      'measureSwapOutput',
    ),

    step.require(
      expression.greaterThanOrEqual(expression.variable('received'), expression.input('minimumOut')),
      'swapMetItsFloor',
    ),

    step.invoke({
      program: account.fixed('kamino'),
      accounts: [
        { account: account.fixed('owner'), signer: true, writable: true },
        { account: account.fixed('obligation'), signer: false, writable: true },
        { account: account.fixed('lendingMarket'), signer: false, writable: false },
        { account: account.fixed('lendingMarketAuthority'), signer: false, writable: false },
        { account: account.fixed('reserve'), signer: false, writable: true },
        { account: account.fixed('reserveLiquiditySupply'), signer: false, writable: true },
        { account: account.fixed('reserveCollateralMint'), signer: false, writable: true },
        { account: account.fixed('reserveDestinationDepositCollateral'), signer: false, writable: true },
        { account: account.fixed('destinationAta'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
      ],
      data: [
        data.literal(KAMINO_DEPOSIT),
        // Exactly what the swap produced, measured a moment ago.
        data.encode('u64', expression.variable('received')),
      ],
      label: 'depositSwapOutput',
    }),
  ],
});

export const compiled = compileTemplate(jupiterDepositExactOutput);
