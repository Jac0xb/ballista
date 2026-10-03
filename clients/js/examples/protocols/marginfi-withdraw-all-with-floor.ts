/** Withdraw everything, with a minimum: docs/examples/protocols/marginfi-withdraw.md. */
// #region template
import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  data,
  defineTemplate,
  expression,
  step,
  tokenTransfer,
} from '../../src/index.js';
import {
  MARGINFI_V2,
  MARGINFI_WITHDRAW,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  TOKEN_ACCOUNT_OWNER_OFFSET,
  addressBytes,
} from './shared.js';

export const marginfiWithdrawAllWithFloor = defineTemplate({
  inputs: {
    /** The run fails below this, rather than sweeping a disappointing withdrawal onward. */
    minimumWithdrawn: { type: 'u64' },
  },
  accounts: {
    marginfi: { executable: true, address: addressBytes(MARGINFI_V2) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    marginfiGroup: {},
    marginfiAccount: { writable: true },
    authority: { signer: true },
    bank: { writable: true },
    bankLiquidityVault: { writable: true },
    /** A PDA marginfi signs for, so it is passed read-only. */
    bankLiquidityVaultAuthority: {},
    destinationAta: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
    /** Where the proceeds go once the floor is met: another of the authority's own accounts. */
    treasuryAta: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
  },
  /**
   * What marginfi's health check reads after the withdrawal: for each balance still open, its bank
   * and then its oracle, by bank address from highest to lowest. Empty if this was the only one.
   */
  accountGroups: ['healthAccounts'],
  steps: [
    // marginfi pays whichever token account it is given, and the sweep pays whichever treasury the
    // run names.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('destinationAta'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('authority'), 'key'),
      ),
      'withdrawalGoesToTheAuthority',
    ),
    step.require(
      expression.equal(
        expression.accountData(account.fixed('treasuryAta'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('authority'), 'key'),
      ),
      'sweepGoesToTheAuthority',
    ),

    step.snapshot(
      'balanceBefore',
      expression.accountData(account.fixed('destinationAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
      'readBalanceBeforeWithdraw',
    ),

    step.invoke({
      program: account.fixed('marginfi'),
      accounts: [
        { account: account.fixed('marginfiGroup'), signer: false, writable: false },
        { account: account.fixed('marginfiAccount'), signer: false, writable: true },
        { account: account.fixed('authority'), signer: true, writable: false },
        { account: account.fixed('bank'), signer: false, writable: true },
        { account: account.fixed('destinationAta'), signer: false, writable: true },
        { account: account.fixed('bankLiquidityVaultAuthority'), signer: false, writable: false },
        { account: account.fixed('bankLiquidityVault'), signer: false, writable: true },
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
      label: 'withdrawAll',
    }),

    step.let(
      'withdrawn',
      expression.subtract(
        expression.accountData(account.fixed('destinationAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
        expression.snapshot('balanceBefore'),
      ),
      'measureWithdrawal',
    ),

    step.require(
      expression.greaterThanOrEqual(expression.variable('withdrawn'), expression.input('minimumWithdrawn')),
      'withdrawalMetItsFloor',
    ),

    tokenTransfer({
      tokenProgram: account.fixed('tokenProgram'),
      source: account.fixed('destinationAta'),
      destination: account.fixed('treasuryAta'),
      authority: account.fixed('authority'),
      amount: expression.variable('withdrawn'),
      label: 'sweepToTreasury',
    }),
  ],
});

export const compiled = compileTemplate(marginfiWithdrawAllWithFloor);
// #endregion template
