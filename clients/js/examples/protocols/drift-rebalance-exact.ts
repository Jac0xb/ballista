/**
 * Move a position from marginfi to Drift in one transaction, depositing exactly what came out.
 *
 * Drift's `deposit(market_index: u16, amount: u64, reduce_only: bool)` needs a number that the
 * marginfi withdrawal produces moments earlier. A transaction has to guess it. Guess high and the
 * deposit fails on insufficient funds, taking the withdrawal down with it; guess low and the
 * remainder sits in the wallet, out of the market, until someone notices.
 *
 * Rate-shopping bots run this loop constantly. Today it is two transactions with an unhedged gap
 * between them, or a custom program.
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
  BORSH_FALSE,
  DRIFT_DEPOSIT,
  DRIFT_V2,
  MARGINFI_V2,
  MARGINFI_WITHDRAW,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  u16Bytes,
} from './shared.js';
import { addressBytes } from './shared.js';

/** Drift's spot market index for the asset being moved; 0 is USDC on mainnet. */
const SPOT_MARKET_INDEX = 0;

export const driftRebalanceExact = defineTemplate({
  inputs: {
    /** Do not bother rebalancing less than this. */
    minimumMoved: { type: 'u64' },
  },
  accounts: {
    marginfi: { executable: true, address: addressBytes(MARGINFI_V2) },
    drift: { executable: true, address: addressBytes(DRIFT_V2) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
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
    marginfiVaultAuthority: { writable: true },
    driftState: {},
    driftUser: { writable: true },
    driftUserStats: { writable: true },
    driftSpotMarketVault: { writable: true },
  },
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
        { account: account.fixed('marginfiVaultAuthority'), signer: false, writable: true },
        { account: account.fixed('marginfiVault'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
      ],
      data: [
        data.literal(MARGINFI_WITHDRAW),
        data.encode('u64', expression.u64(0)),
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
      program: account.fixed('drift'),
      accounts: [
        { account: account.fixed('driftState'), signer: false, writable: false },
        { account: account.fixed('driftUser'), signer: false, writable: true },
        { account: account.fixed('driftUserStats'), signer: false, writable: true },
        { account: account.fixed('owner'), signer: true, writable: false },
        { account: account.fixed('driftSpotMarketVault'), signer: false, writable: true },
        { account: account.fixed('walletAta'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
      ],
      data: [
        data.literal(DRIFT_DEPOSIT),
        data.literal(u16Bytes(SPOT_MARKET_INDEX)),
        // Exactly what marginfi released, not an estimate of it.
        data.encode('u64', expression.variable('moved')),
        data.literal(BORSH_FALSE),
      ],
      label: 'depositIntoDrift',
    }),
  ],
});

export const compiled = compileTemplate(driftRebalanceExact);
