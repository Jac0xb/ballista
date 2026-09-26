/**
 * Withdraw everything from a marginfi bank, then refuse the run unless enough came out.
 *
 * `lending_account_withdraw(amount: u64, withdraw_all: Option<bool>)` will empty a position, but
 * it does not tell the caller how much that was, and the caller cannot condition on it. A
 * withdrawal that returns far less than expected — the bank was drained, the position had been
 * liquidated, utilization capped it — still succeeds, and anything sequenced after it proceeds
 * on a false assumption.
 *
 * Here the amount that landed is measured and has to clear a floor before the run continues. The
 * trailing `Option::None` byte is `withdraw_all` left unset on the first argument path; the
 * second element sets it to `Some(true)`.
 */
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
    bankLiquidityVaultAuthority: { writable: true },
    destinationAta: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
    /** Where the proceeds go once the floor is met. */
    treasuryAta: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
  },
  steps: [
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
        { account: account.fixed('bankLiquidityVaultAuthority'), signer: false, writable: true },
        { account: account.fixed('bankLiquidityVault'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
      ],
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
