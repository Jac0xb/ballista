/** Buy a basket of pump.fun coins within a budget: docs/examples/protocols/pump-buy-basket.md. */
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
  PUMP_FUN_BUY,
  PUMP_TRACK_VOLUME,
  TOKEN_ACCOUNT_LENGTH,
  TOKEN_ACCOUNT_OWNER_OFFSET,
  addressBytes,
} from './shared.js';

const buyer = account.fixed('buyer');
const row = (name: string) => account.iteration(name);
const buyersLamports = expression.accountField(buyer, 'lamports');

export const pumpFunBuyBasket = defineTemplate({
  inputs: {
    /** The most the whole basket may take from the buyer, in lamports: fees and rent included. */
    budget: { type: 'u64' },
  },
  accounts: {
    pumpProgram: { executable: true, address: addressBytes(PUMP_FUN) },
    global: {},
    /** Pays for every coin, and receives every coin in its own token accounts. */
    buyer: { signer: true, writable: true },
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    /** pump.fun mints every coin `create_v2` makes with Token-2022. */
    tokenProgram: { executable: true, address: TOKEN_2022_PROGRAM_ADDRESS_BYTES },
    eventAuthority: {},
    globalVolumeAccumulator: { writable: true },
    /** The buyer's volume record. pump.fun creates it on the buyer's first buy, at the buyer's cost. */
    userVolumeAccumulator: { writable: true },
    feeConfig: {},
    feeProgram: { executable: true, address: addressBytes(PUMP_FEES) },
    buybackFeeRecipient: { writable: true },
  },
  batch: {
    // Each buy makes eight calls (pump.fun, its fee program, a token transfer, four SOL transfers
    // and its event log), and a transaction holds at most 64. Seven rows use 60 at most.
    maxIterations: 7,
    minIterations: 1,
    row: {
      mint: {},
      /** Owner-pinned, so `complete` is read from an account pump.fun wrote. */
      bondingCurve: {
        writable: true,
        owner: addressBytes(PUMP_FUN),
        minDataLength: PUMP_BONDING_CURVE.minLength,
      },
      /** The curve's own token account, which the coins come from. */
      curveTokenAccount: { writable: true },
      /** Must belong to the buyer (`tokensGoToTheBuyer`). */
      buyerTokenAccount: {
        writable: true,
        owner: TOKEN_2022_PROGRAM_ADDRESS_BYTES,
        minDataLength: TOKEN_ACCOUNT_LENGTH,
      },
      creatorVault: { writable: true },
      bondingCurveV2: {},
      /** An ordinary fee recipient, or a reserved one for a mayhem-mode coin. */
      feeRecipient: { writable: true },
    },
    rowInputs: {
      /** Base units of the coin to buy. */
      amount: { type: 'u64' },
      /** The most this buy may cost, in lamports, fees included. pump.fun enforces it. */
      maxSolCost: { type: 'u64' },
    },
  },
  steps: [
    step.let('spent', expression.u64(0)),
    step.forEach(
      [
        // pump.fun checks only the mint of the account it pays, so a run built by someone else
        // could otherwise send the coins anywhere.
        step.require(
          expression.equal(
            expression.accountData(row('buyerTokenAccount'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
            expression.accountField(buyer, 'key'),
          ),
          'tokensGoToTheBuyer',
        ),
        // A graduated coin trades on PumpSwap. Say so here, before pump.fun refuses the buy with
        // its own `BondingCurveComplete` (6005).
        step.require(
          expression.not(expression.accountData(row('bondingCurve'), PUMP_BONDING_CURVE.complete, 'bool')),
          'curveNotGraduated',
        ),
        step.let('lamportsBefore', buyersLamports, 'readLamportsBefore'),
        step.invoke({
          program: account.fixed('pumpProgram'),
          accounts: [
            { account: account.fixed('global'), signer: false, writable: false },
            { account: row('feeRecipient'), signer: false, writable: true },
            { account: row('mint'), signer: false, writable: false },
            { account: row('bondingCurve'), signer: false, writable: true },
            { account: row('curveTokenAccount'), signer: false, writable: true },
            { account: row('buyerTokenAccount'), signer: false, writable: true },
            { account: buyer, signer: true, writable: true },
            { account: account.fixed('systemProgram'), signer: false, writable: false },
            { account: account.fixed('tokenProgram'), signer: false, writable: false },
            { account: row('creatorVault'), signer: false, writable: true },
            { account: account.fixed('eventAuthority'), signer: false, writable: false },
            { account: account.fixed('pumpProgram'), signer: false, writable: false },
            { account: account.fixed('globalVolumeAccumulator'), signer: false, writable: true },
            { account: account.fixed('userVolumeAccumulator'), signer: false, writable: true },
            { account: account.fixed('feeConfig'), signer: false, writable: false },
            { account: account.fixed('feeProgram'), signer: false, writable: false },
            { account: row('bondingCurveV2'), signer: false, writable: false },
            { account: account.fixed('buybackFeeRecipient'), signer: false, writable: true },
          ],
          data: [
            data.literal(PUMP_FUN_BUY),
            data.encode('u64', expression.rowInput('amount')),
            data.encode('u64', expression.rowInput('maxSolCost')),
            data.literal(PUMP_TRACK_VOLUME),
          ],
          label: 'buyOnTheCurve',
        }),
        // What the buy took from the buyer: the price, pump.fun's and the creator's fees, and any
        // rent it charged.
        step.assign(
          'spent',
          expression.add(
            expression.variable('spent'),
            expression.subtract(expression.variable('lamportsBefore'), buyersLamports),
          ),
          'addWhatItCost',
        ),
        step.require(
          expression.lessThanOrEqual(expression.variable('spent'), expression.input('budget')),
          'withinBudget',
        ),
      ],
      { carry: ['spent'], label: 'everyCoin' },
    ),
  ],
});

export const compiled = compileTemplate(pumpFunBuyBasket);
// #endregion template
