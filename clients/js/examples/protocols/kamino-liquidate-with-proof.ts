/** Liquidate with a minimum payout: docs/examples/protocols/kamino-liquidate.md. */
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
  KAMINO_LEND,
  KAMINO_LIQUIDATE,
  SYSVAR_INSTRUCTIONS,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  TOKEN_ACCOUNT_OWNER_OFFSET,
  addressBytes,
} from './shared.js';

export const kaminoLiquidateWithProof = defineTemplate({
  inputs: {
    /**
     * The most debt to repay, in the repaid token's base units. Kamino repays less if one
     * liquidation may not take that much of the debt.
     */
    liquidityAmount: { type: 'u64' },
    /** Kamino's own floor on the collateral it pays, net of its fee, by its own count; 0 for none. */
    minAcceptableReceived: { type: 'u64' },
    /**
     * The least `userDestinationLiquidity` must grow by, in the collateral's base units: what the
     * liquidator receives, with the repayment not subtracted.
     */
    minimumBounty: { type: 'u64' },
  },
  accounts: {
    kamino: { executable: true, address: addressBytes(KAMINO_LEND) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    instructionsSysvar: { address: addressBytes(SYSVAR_INSTRUCTIONS) },
    /** Kamino declares it a bare signer, so it is read-only. */
    liquidator: { signer: true },
    obligation: { writable: true },
    lendingMarket: {},
    lendingMarketAuthority: {},
    repayReserve: { writable: true },
    repayReserveLiquidityMint: {},
    repayReserveLiquiditySupply: { writable: true },
    withdrawReserve: { writable: true },
    withdrawReserveLiquidityMint: {},
    withdrawReserveCollateralMint: { writable: true },
    withdrawReserveCollateralSupply: { writable: true },
    withdrawReserveLiquiditySupply: { writable: true },
    /** Where Kamino's protocol fee on the seized collateral goes: the withdrawn reserve's fee vault. */
    withdrawReserveFeeReceiver: { writable: true },
    /** Pays the repayment. */
    userSourceLiquidity: { writable: true },
    /** Receives the seized cTokens, which Kamino redeems in the same instruction. */
    userDestinationCollateral: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
    /** Receives the redeemed collateral: the account the bounty is measured on. */
    userDestinationLiquidity: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
  },
  /**
   * The end of Kamino's v2 liquidation, its own writable flags kept: the withdrawn reserve's
   * collateral farm pair, the repaid reserve's debt farm pair, and the Farms program.
   */
  accountGroups: ['farmAccounts'],
  steps: [
    // Kamino checks the mints of the accounts it pays, not whose they are.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('userDestinationLiquidity'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('liquidator'), 'key'),
      ),
      'bountyGoesToTheLiquidator',
    ),
    step.require(
      expression.equal(
        expression.accountData(account.fixed('userDestinationCollateral'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('liquidator'), 'key'),
      ),
      'seizedCollateralGoesToTheLiquidator',
    ),

    // Kamino redeems the seized cTokens in the same instruction and pays the underlying here, less
    // its fee. What it can't redeem stays in `userDestinationCollateral` and doesn't count.
    step.snapshot(
      'payoutBefore',
      expression.accountData(account.fixed('userDestinationLiquidity'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
      'readPayoutBefore',
    ),

    // v2: the v1 handler refuses every caller but Kamino itself and a short whitelist.
    step.invoke({
      program: account.fixed('kamino'),
      accounts: [
        { account: account.fixed('liquidator'), signer: true, writable: false },
        { account: account.fixed('obligation'), signer: false, writable: true },
        { account: account.fixed('lendingMarket'), signer: false, writable: false },
        { account: account.fixed('lendingMarketAuthority'), signer: false, writable: false },
        { account: account.fixed('repayReserve'), signer: false, writable: true },
        { account: account.fixed('repayReserveLiquidityMint'), signer: false, writable: false },
        { account: account.fixed('repayReserveLiquiditySupply'), signer: false, writable: true },
        { account: account.fixed('withdrawReserve'), signer: false, writable: true },
        { account: account.fixed('withdrawReserveLiquidityMint'), signer: false, writable: false },
        { account: account.fixed('withdrawReserveCollateralMint'), signer: false, writable: true },
        { account: account.fixed('withdrawReserveCollateralSupply'), signer: false, writable: true },
        { account: account.fixed('withdrawReserveLiquiditySupply'), signer: false, writable: true },
        { account: account.fixed('withdrawReserveFeeReceiver'), signer: false, writable: true },
        { account: account.fixed('userSourceLiquidity'), signer: false, writable: true },
        { account: account.fixed('userDestinationCollateral'), signer: false, writable: true },
        { account: account.fixed('userDestinationLiquidity'), signer: false, writable: true },
        // `collateral_token_program`, `repay_liquidity_token_program`, `withdraw_liquidity_token_program`.
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('instructionsSysvar'), signer: false, writable: false },
      ],
      accountGroup: 'farmAccounts',
      data: [
        data.literal(KAMINO_LIQUIDATE),
        data.encode('u64', expression.input('liquidityAmount')),
        data.encode('u64', expression.input('minAcceptableReceived')),
        // No LTV override: liquidate on the protocol's own terms.
        data.encode('u64', expression.u64(0)),
      ],
      label: 'liquidate',
    }),

    step.require(
      expression.greaterThanOrEqual(
        expression.subtract(
          expression.accountData(account.fixed('userDestinationLiquidity'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
          expression.snapshot('payoutBefore'),
        ),
        expression.input('minimumBounty'),
      ),
      'liquidationPaidTheBounty',
    ),
  ],
});

export const compiled = compileTemplate(kaminoLiquidateWithProof);
// #endregion template
