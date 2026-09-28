/**
 * Move a position from marginfi to Kamino in one transaction, depositing exactly what came out.
 *
 * Kamino's `deposit_reserve_liquidity_and_obligation_collateral_v2(liquidity_amount: u64)` needs a
 * number that the marginfi withdrawal produces moments earlier. A transaction has to guess it.
 * Guess high and the deposit fails on insufficient funds, taking the withdrawal down with it;
 * guess low and the remainder sits in the wallet, earning nothing, until someone notices.
 *
 * The template empties the marginfi balance, measures what landed in the wallet, and deposits
 * exactly that.
 * - `healthAccounts` is what marginfi's health check reads once the withdrawn balance is gone: for
 *   every balance the account still holds, its bank and then its oracle, by bank address from
 *   highest to lowest. It is empty when the withdrawn balance was the only one.
 * - `farmAccounts` is the end of Kamino's v2 deposit: the obligation's farm user state and the
 *   reserve's collateral farm, or the Kamino program for each when the reserve has no collateral
 *   farm; then the Farms program. A group carries them so each keeps its own writable flag.
 * - Kamino takes the deposit only into an obligation refreshed in the same slot. Put
 *   `refresh_reserve` for each reserve the obligation holds, then `refresh_obligation` with those
 *   reserves, before this run.
 * - Kamino mints whole cTokens only, and takes just what they are worth: of the amount it is asked
 *   for, less than one cToken's worth (a base unit or so) can stay in `walletAta`.
 *
 * Without a template this takes two transactions, with the funds sitting in the wallet between
 * them, or a custom program.
 *
 * SPL Token banks and reserves only: `tokenProgram` and `walletAta`'s owner are pinned to SPL
 * Token.
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
