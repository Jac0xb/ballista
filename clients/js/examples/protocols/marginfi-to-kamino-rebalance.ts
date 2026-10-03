/** Move a position into Kamino: docs/examples/protocols/marginfi-to-kamino.md. */
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
  KAMINO_DEPOSIT,
  KAMINO_LEND,
  MARGINFI_V2,
  MARGINFI_WITHDRAW,
  SYSVAR_INSTRUCTIONS,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  addressBytes,
} from './shared.js';

export const marginfiToKaminoRebalance = defineTemplate({
  inputs: {
    /** Do not bother rebalancing less than this. */
    minimumMoved: { type: 'u64' },
  },
  accounts: {
    marginfi: { executable: true, address: addressBytes(MARGINFI_V2) },
    kamino: { executable: true, address: addressBytes(KAMINO_LEND) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    instructionsSysvar: { address: addressBytes(SYSVAR_INSTRUCTIONS) },
    owner: { signer: true, writable: true },
    /** The wallet account the assets pass through. */
    walletAta: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
    marginfiGroup: {},
    marginfiAccount: { writable: true },
    marginfiBank: { writable: true },
    marginfiVault: { writable: true },
    marginfiVaultAuthority: {},
    obligation: { writable: true },
    lendingMarket: {},
    lendingMarketAuthority: {},
    reserve: { writable: true },
    reserveLiquidityMint: {},
    reserveLiquiditySupply: { writable: true },
    reserveCollateralMint: { writable: true },
    reserveDestinationDepositCollateral: { writable: true },
  },
  /**
   * `healthAccounts`: what marginfi's health check reads after the withdrawal, as in
   * `marginfi-withdraw-all-with-floor.ts`. `farmAccounts`: the end of Kamino's v2 deposit, its own
   * writable flags kept: the collateral farm pair and the Farms program.
   */
  accountGroups: ['healthAccounts', 'farmAccounts'],
  steps: [
    step.snapshot(
      'walletBefore',
      expression.accountData(account.fixed('walletAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
      'readWalletBeforeWithdraw',
    ),

    step.invoke({
      program: account.fixed('marginfi'),
      accounts: [
        { account: account.fixed('marginfiGroup'), signer: false, writable: false },
        { account: account.fixed('marginfiAccount'), signer: false, writable: true },
        { account: account.fixed('owner'), signer: true, writable: false },
        { account: account.fixed('marginfiBank'), signer: false, writable: true },
        { account: account.fixed('walletAta'), signer: false, writable: true },
        { account: account.fixed('marginfiVaultAuthority'), signer: false, writable: false },
        { account: account.fixed('marginfiVault'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
      ],
      accountGroup: 'healthAccounts',
      data: [
        data.literal(MARGINFI_WITHDRAW),
        // `amount` is ignored when `withdraw_all` is Some(true), but Borsh still reads it.
        data.encode('u64', expression.u64(0)),
        // Option::Some(true).
        data.literal(Uint8Array.of(1, 1)),
      ],
      label: 'withdrawFromMarginfi',
    }),

    step.let(
      'moved',
      expression.subtract(
        expression.accountData(account.fixed('walletAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
        expression.snapshot('walletBefore'),
      ),
      'measureWithdrawal',
    ),

    step.require(
      expression.greaterThanOrEqual(expression.variable('moved'), expression.input('minimumMoved')),
      'worthRebalancing',
    ),

    // v2: the v1 handler refuses every caller but Kamino itself and a short whitelist.
    step.invoke({
      program: account.fixed('kamino'),
      accounts: [
        { account: account.fixed('owner'), signer: true, writable: true },
        { account: account.fixed('obligation'), signer: false, writable: true },
        { account: account.fixed('lendingMarket'), signer: false, writable: false },
        { account: account.fixed('lendingMarketAuthority'), signer: false, writable: false },
        { account: account.fixed('reserve'), signer: false, writable: true },
        { account: account.fixed('reserveLiquidityMint'), signer: false, writable: false },
        { account: account.fixed('reserveLiquiditySupply'), signer: false, writable: true },
        { account: account.fixed('reserveCollateralMint'), signer: false, writable: true },
        { account: account.fixed('reserveDestinationDepositCollateral'), signer: false, writable: true },
        { account: account.fixed('walletAta'), signer: false, writable: true },
        // `placeholder_user_destination_collateral`, never used: the Kamino program means "none".
        { account: account.fixed('kamino'), signer: false, writable: false },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('instructionsSysvar'), signer: false, writable: false },
      ],
      accountGroup: 'farmAccounts',
      data: [
        data.literal(KAMINO_DEPOSIT),
        // Exactly what marginfi released, not an estimate of it.
        data.encode('u64', expression.variable('moved')),
      ],
      label: 'depositIntoKamino',
    }),
  ],
});

export const compiled = compileTemplate(marginfiToKaminoRebalance);
// #endregion template
