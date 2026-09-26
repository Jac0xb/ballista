/**
 * Settle Drift PnL, then withdraw without ever borrowing.
 *
 * `settle_pnl` is permissionless: anyone may call it for any user and market. That makes it a
 * natural thing to schedule, and a natural thing to batch with the withdrawal that depends on it.
 * The risk in the batch is the withdrawal: with `reduce_only` off, a Drift withdrawal larger than
 * the deposit opens a borrow for the difference.
 *
 * The template settles, withdraws `minimumSettled` with `reduce_only` on, so the withdrawal can
 * only draw down a deposit, and then requires the destination token account to have received the
 * full amount. If the account could not pay that out without borrowing, the run reverts, settle
 * included.
 *
 * It does not tell settled PnL apart from an earlier deposit. That needs the `User` account's spot
 * positions, a zero-copy struct whose offsets are not derived here. With them, a `when` on the
 * withdrawal could also turn "nothing settled" into a no-op instead of a revert.
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
  BORSH_TRUE,
  DRIFT_V2,
  DRIFT_WITHDRAW,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  addressBytes,
  anchorDiscriminator,
  u16Bytes,
} from './shared.js';

const DRIFT_SETTLE_PNL = anchorDiscriminator('settle_pnl');
const PERP_MARKET_INDEX = 0;
const SPOT_MARKET_INDEX = 0;

export const driftSettleWhenProfitable = defineTemplate({
  inputs: {
    /** The smallest settlement worth withdrawing. */
    minimumSettled: { type: 'u64' },
  },
  accounts: {
    drift: { executable: true, address: addressBytes(DRIFT_V2) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    owner: { signer: true, writable: true },
    driftState: {},
    driftUser: { writable: true },
    driftUserStats: { writable: true },
    driftSpotMarketVault: { writable: true },
    driftSigner: {},
    perpMarket: { writable: true },
    spotMarket: { writable: true },
    destinationAta: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
  },
  steps: [
    step.snapshot(
      'balanceBefore',
      expression.accountData(account.fixed('destinationAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
      'readBalanceBeforeSettle',
    ),

    step.invoke({
      program: account.fixed('drift'),
      accounts: [
        { account: account.fixed('driftState'), signer: false, writable: false },
        { account: account.fixed('owner'), signer: true, writable: false },
        { account: account.fixed('driftUser'), signer: false, writable: true },
        { account: account.fixed('spotMarket'), signer: false, writable: true },
        { account: account.fixed('perpMarket'), signer: false, writable: true },
      ],
      data: [data.literal(DRIFT_SETTLE_PNL), data.literal(u16Bytes(PERP_MARKET_INDEX))],
      label: 'settlePnl',
    }),

    step.invoke({
      program: account.fixed('drift'),
      accounts: [
        { account: account.fixed('driftState'), signer: false, writable: false },
        { account: account.fixed('driftUser'), signer: false, writable: true },
        { account: account.fixed('driftUserStats'), signer: false, writable: true },
        { account: account.fixed('owner'), signer: true, writable: false },
        { account: account.fixed('driftSpotMarketVault'), signer: false, writable: true },
        { account: account.fixed('driftSigner'), signer: false, writable: false },
        { account: account.fixed('destinationAta'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
      ],
      data: [
        data.literal(DRIFT_WITHDRAW),
        data.literal(u16Bytes(SPOT_MARKET_INDEX)),
        data.encode('u64', expression.input('minimumSettled')),
        // reduce_only: draw down a deposit, never open a borrow.
        data.literal(BORSH_TRUE),
      ],
      label: 'withdrawSettled',
    }),

    step.require(
      expression.greaterThanOrEqual(
        expression.subtract(
          expression.accountData(account.fixed('destinationAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
          expression.snapshot('balanceBefore'),
        ),
        expression.input('minimumSettled'),
      ),
      'settlementLanded',
    ),
  ],
});

export const compiled = compileTemplate(driftSettleWhenProfitable);
