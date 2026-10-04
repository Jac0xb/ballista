/** Sell a whole pump.fun balance with a floor: docs/examples/protocols/pump-sell-all.md. */
// #region template
import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  TOKEN_2022_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  data,
  defineTemplate,
  expression,
  step,
} from '@jac0xb/ballista';
import {
  PUMP_BONDING_CURVE,
  PUMP_FEES,
  PUMP_FUN,
  PUMP_FUN_SELL,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  TOKEN_ACCOUNT_MINT_OFFSET,
  TOKEN_ACCOUNT_OWNER_OFFSET,
  addressBytes,
} from './shared.js';

const seller = account.fixed('seller');
const sellerTokenAccount = account.fixed('sellerTokenAccount');
const sellersLamports = expression.accountField(seller, 'lamports');

export const pumpFunSellAll = defineTemplate({
  inputs: {
    /** The least the sale must pay the seller, in lamports, after every fee pump.fun takes. */
    minSolOut: { type: 'u64' },
  },
  accounts: {
    pumpProgram: { executable: true, address: addressBytes(PUMP_FUN) },
    global: {},
    /** An ordinary fee recipient, or a reserved one for a mayhem-mode coin. */
    feeRecipient: { writable: true },
    /** pump.fun derives the curve from this mint, and refuses a curve of any other. */
    mint: {},
    /** Owner-pinned, so `complete` is read from an account pump.fun wrote. */
    bondingCurve: {
      writable: true,
      owner: addressBytes(PUMP_FUN),
      minDataLength: PUMP_BONDING_CURVE.minLength,
    },
    curveTokenAccount: { writable: true },
    /** Sold whole. Must belong to the seller and hold `mint`. */
    sellerTokenAccount: {
      writable: true,
      owner: TOKEN_2022_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
    seller: { signer: true, writable: true },
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    creatorVault: { writable: true },
    /** pump.fun mints every coin `create_v2` makes with Token-2022. */
    tokenProgram: { executable: true, address: TOKEN_2022_PROGRAM_ADDRESS_BYTES },
    eventAuthority: {},
    feeConfig: {},
    feeProgram: { executable: true, address: addressBytes(PUMP_FEES) },
    bondingCurveV2: {},
    buybackFeeRecipient: { writable: true },
  },
  steps: [
    // A delegate approved on someone else's account could otherwise sell their balance.
    step.require(
      expression.equal(
        expression.accountData(sellerTokenAccount, TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountField(seller, 'key'),
      ),
      'sellsTheSellersOwnTokens',
    ),
    step.require(
      expression.equal(
        expression.accountData(sellerTokenAccount, TOKEN_ACCOUNT_MINT_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('mint'), 'key'),
      ),
      'holdsTheCurvesCoin',
    ),
    // A graduated coin trades on PumpSwap. Say so here, before pump.fun refuses the sale with its
    // own `BondingCurveComplete` (6005).
    step.require(
      expression.not(expression.accountData(account.fixed('bondingCurve'), PUMP_BONDING_CURVE.complete, 'bool')),
      'curveNotGraduated',
    ),
    // Whatever the account holds when the transaction runs, not what it held at signing.
    step.let('balance', expression.accountData(sellerTokenAccount, TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'), 'readBalance'),
    step.require(expression.greaterThan(expression.variable('balance'), expression.u64(0)), 'hasTokensToSell'),
    step.let('lamportsBefore', sellersLamports, 'readLamportsBefore'),
    step.invoke({
      program: account.fixed('pumpProgram'),
      accounts: [
        { account: account.fixed('global'), signer: false, writable: false },
        { account: account.fixed('feeRecipient'), signer: false, writable: true },
        { account: account.fixed('mint'), signer: false, writable: false },
        { account: account.fixed('bondingCurve'), signer: false, writable: true },
        { account: account.fixed('curveTokenAccount'), signer: false, writable: true },
        { account: sellerTokenAccount, signer: false, writable: true },
        { account: seller, signer: true, writable: true },
        { account: account.fixed('systemProgram'), signer: false, writable: false },
        { account: account.fixed('creatorVault'), signer: false, writable: true },
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('eventAuthority'), signer: false, writable: false },
        { account: account.fixed('pumpProgram'), signer: false, writable: false },
        { account: account.fixed('feeConfig'), signer: false, writable: false },
        { account: account.fixed('feeProgram'), signer: false, writable: false },
        { account: account.fixed('bondingCurveV2'), signer: false, writable: false },
        { account: account.fixed('buybackFeeRecipient'), signer: false, writable: true },
      ],
      data: [
        data.literal(PUMP_FUN_SELL),
        data.encode('u64', expression.variable('balance')),
        // pump.fun's own floor, left at 0: the check below measures what actually arrived.
        data.encode('u64', expression.u64(0)),
      ],
      label: 'sellEverything',
    }),
    // The seller's lamports after the sale, less before: the price, net of pump.fun's and the
    // creator's fees.
    step.require(
      expression.greaterThanOrEqual(
        expression.subtract(sellersLamports, expression.variable('lamportsBefore')),
        expression.input('minSolOut'),
      ),
      'receivedAtLeastMinSolOut',
    ),
  ],
});

export const compiled = compileTemplate(pumpFunSellAll);
// #endregion template
