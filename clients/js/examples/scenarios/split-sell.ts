/**
 * A split sell: one Jupiter route, sold in equal slices by a count loop.
 *
 * Test-only, like every template in this folder: they exist to run Ballista's newer runtime
 * features against the real mainnet programs in `tests/protocols`, and are not public examples.
 * `splitSellPayout` is scenario A of `tests/protocols/findings/runtime-scenarios.md`;
 * `splitSellInner` is the inner run of scenario B, `nested-split-sell.ts`.
 *
 * Before any CPI the template checks what `jupiter-oracle-checked-swap.ts` checks: the Pyth account
 * is a fully verified update of the feed the caller names, at most a minute old, with a positive
 * price; each token account holds the mint it should; and the seller owns both.
 *
 * Then `step.repeat` sells `slices` equal parts of the source balance, one Jupiter `route` a pass.
 * Each pass writes the slice into `route` as `in_amount` with the quote rescaled to it, as
 * `token-sweep-into-swap.ts` rescales it to a balance, and requires that exactly the slice left
 * the source and that it fetched at least the oracle floor. It logs one `SLCE` event, the pass
 * index, the amount sold and the amount received, and carries the running total received out of
 * the loop. `sold == slice` is what ties the floor, computed once from the slice before the loop,
 * to what each pass actually sold.
 *
 * It sells wrapped SOL for USDC only. The oracle-checked swap reads both mints' decimals and the
 * feed's exponent to scale any pair, 28 registers of the 64 it uses; beside a count loop, its event
 * and a payout loop, that does not fit. Here the mints are pinned, so their decimals are known,
 * and the feed's exponent is required to be SOL/USD's −8. A fill of `sold` lamports is then worth
 * `sold × price ÷ 10^11` USDC base units, and the floor is that less `toleranceBps`, rounded down at
 * each step as the oracle-checked swap rounds it.
 */
import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  data,
  defineTemplate,
  expression,
  step,
  tokenTransfer,
  type Step,
} from '@jac0xb/ballista';
import {
  JUPITER_ROUTE,
  JUPITER_V6,
  PYTH,
  PYTH_RECEIVER,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  TOKEN_ACCOUNT_MINT_OFFSET,
  TOKEN_ACCOUNT_OWNER_OFFSET,
  WRAPPED_SOL_MINT,
  addressBytes,
} from '../protocols/shared.js';

/** USDC's mint. */
export const USDC_MINT = 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v' as const;
/** SOL/USD's Pyth exponent: prices are in units of 10^−8 dollars. */
export const SOL_USD_EXPONENT = -8;
/**
 * `10^(9 − 6 + 8)`: wrapped SOL's 9 decimals less USDC's 6, and the exponent. `sold × price ÷ this`
 * is a fill's worth in USDC base units.
 */
const SOL_USD_SCALE = 10n ** 11n;
/** ASCII `SLCE`: the tag of the event each slice logs. */
export const SLICE_EVENT_TAG = new TextEncoder().encode('SLCE');
/** The most slices one run sells: `step.repeat`'s `max`. */
export const MAX_SLICES = 8;
/** The most payout rows one run takes. */
export const MAX_PAYOUTS = 4;
/** The route's platform fee account and rate are chosen by whoever builds the run: cap the rate. */
export const MAX_PLATFORM_FEE_BPS = 0n;

const balanceOf = (name: string) => expression.accountData(account.fixed(name), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64');
const variable = expression.variable;
const input = expression.input;

/** The split sell's inputs. The route goes in parts, as `splitJupiterRoute` splits it. */
export const splitSellInputs = {
  /** The Pyth feed the price must come from: SOL/USD's. */
  feedId: { type: 'pubkey' },
  /** `route_plan` as the Swap API encoded it: the bytes between the discriminator and `in_amount`. */
  routePlan: { type: 'bytes', maxLength: 512 },
  /** The `in_amount` the route was quoted for. */
  quotedInAmount: { type: 'u64' },
  /** The quote's `quoted_out_amount` for that input. */
  quotedOutAmount: { type: 'u64' },
  /** The quote's `slippage_bps`. */
  slippageBps: { type: 'u64' },
  /** The quote's `platform_fee_bps`, at most `MAX_PLATFORM_FEE_BPS`. */
  platformFeeBps: { type: 'u64' },
  /** How far below the oracle a slice's fill may land, in basis points. */
  toleranceBps: { type: 'u64' },
  /** How many equal slices to sell the source balance in: 1 to `MAX_SLICES`. */
  slices: { type: 'u64' },
} as const;

/** The split sell's fixed accounts, in the order its run takes them. */
export const splitSellAccounts = {
  jupiter: { executable: true, address: addressBytes(JUPITER_V6) },
  tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
  priceUpdate: { owner: addressBytes(PYTH_RECEIVER), minDataLength: PYTH.length },
  seller: { signer: true },
  sourceAta: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: TOKEN_ACCOUNT_LENGTH },
  destinationAta: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: TOKEN_ACCOUNT_LENGTH },
} as const;

/** A payout row: a token account of the destination mint, and what it is paid. */
export const payoutBatch = {
  maxIterations: MAX_PAYOUTS,
  minIterations: 1,
  row: {
    recipientAta: { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: TOKEN_ACCOUNT_LENGTH },
  },
  rowInputs: { amount: { type: 'u64' } },
} as const;

/**
 * The checks, then the count loop. Leaves the total received in `totalReceived`. Every name the
 * steps use is declared by `splitSellInputs`, `splitSellAccounts` and the `routeAccounts` group.
 */
export function splitSellSteps(): Step[] {
  const priceUpdate = account.fixed('priceUpdate');
  const mintOf = (name: string) => expression.accountData(account.fixed(name), TOKEN_ACCOUNT_MINT_OFFSET, 'pubkey');
  const ownerOf = (name: string) => expression.accountData(account.fixed(name), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey');
  return [
    // Pin the verification level first: it decides where every other field sits.
    step.require(
      expression.equal(
        expression.accountData(priceUpdate, PYTH.verificationLevel, 'u8'),
        expression.u64(PYTH.verificationLevelFull),
      ),
      'priceIsFullyVerified',
    ),
    step.require(
      expression.equal(expression.accountData(priceUpdate, PYTH.feedId, 'pubkey'), input('feedId')),
      'priceIsTheExpectedFeed',
    ),
    step.require(
      expression.lessThanOrEqual(
        expression.subtract(expression.clockUnixTimestamp(), expression.accountData(priceUpdate, PYTH.publishTime, 'i64')),
        expression.i64(60),
      ),
      'oracleIsFresh',
    ),
    step.require(
      expression.equal(expression.accountData(priceUpdate, PYTH.exponent, 'i32'), expression.i64(SOL_USD_EXPONENT)),
      'priceIsInSolUsdUnits',
    ),
    // Pyth prices are signed: a negative one fails this cast, and zero the requirement after it.
    step.let('oraclePrice', expression.cast('u64', expression.accountData(priceUpdate, PYTH.price, 'i64')), 'readOraclePrice'),
    step.require(expression.greaterThan(variable('oraclePrice'), expression.u64(0)), 'oraclePriceIsPositive'),

    step.require(
      expression.equal(mintOf('sourceAta'), expression.pubkey(addressBytes(WRAPPED_SOL_MINT))),
      'sourceHoldsWrappedSol',
    ),
    step.require(expression.equal(mintOf('destinationAta'), expression.pubkey(addressBytes(USDC_MINT))), 'destinationHoldsUsdc'),
    step.let('sellerKey', expression.accountKey('seller')),
    step.require(expression.equal(ownerOf('sourceAta'), variable('sellerKey')), 'sellsTheSellersOwnTokens'),
    step.require(expression.equal(ownerOf('destinationAta'), variable('sellerKey')), 'proceedsGoToTheSeller'),

    // A count of zero would divide by zero; above `MAX_SLICES` the loop itself refuses it.
    step.require(expression.greaterThan(input('slices'), expression.u64(0)), 'atLeastOneSlice'),
    step.let('slice', expression.divide(balanceOf('sourceAta'), input('slices')), 'sizeTheSlice'),
    // The quote was for `quotedInAmount`; each slice should fetch its share of it.
    // `multiplyDivide` keeps the product exact, so three u64 operands need no casts.
    step.let(
      'sliceQuote',
      expression.multiplyDivide(input('quotedOutAmount'), variable('slice'), input('quotedInAmount')),
      'rescaleQuoteToTheSlice',
    ),
    // The oracle's worth of one slice, less the tolerance, rounded down at each step as the
    // oracle-checked swap rounds it. Every pass must sell exactly the slice, so one floor serves
    // them all.
    step.let(
      'sliceFloor',
      expression.multiplyDivide(
        expression.multiplyDivide(variable('slice'), variable('oraclePrice'), expression.u64(SOL_USD_SCALE)),
        expression.subtract(expression.u64(10_000), input('toleranceBps')),
        expression.u64(10_000),
      ),
      'computeSliceFloor',
    ),

    // The fee account sits in the route's own accounts: any nonzero rate pays whoever chose it.
    step.require(
      expression.lessThanOrEqual(input('platformFeeBps'), expression.u64(MAX_PLATFORM_FEE_BPS)),
      'platformFeeWithinCap',
    ),
    step.let('totalReceived', expression.u64(0)),
    step.repeat(
      input('slices'),
      [
        step.let('sourceBefore', balanceOf('sourceAta'), 'readSourceBeforeSlice'),
        step.let('proceedsBefore', balanceOf('destinationAta'), 'readProceedsBeforeSlice'),
        step.invoke({
          program: account.fixed('jupiter'),
          accounts: [
            { account: account.fixed('tokenProgram'), signer: false, writable: false },
            { account: account.fixed('seller'), signer: true, writable: false },
            { account: account.fixed('sourceAta'), signer: false, writable: true },
            { account: account.fixed('destinationAta'), signer: false, writable: true },
          ],
          accountGroup: 'routeAccounts',
          data: [
            data.literal(JUPITER_ROUTE),
            data.encode('bytes', input('routePlan')),
            data.encode('u64', variable('slice')),
            data.encode('u64', variable('sliceQuote')),
            data.encode('u16', input('slippageBps')),
            data.encode('u8', input('platformFeeBps')),
          ],
          label: 'sellSlice',
        }),
        step.let('sold', expression.subtract(variable('sourceBefore'), balanceOf('sourceAta')), 'measureSliceSold'),
        step.require(expression.equal(variable('sold'), variable('slice')), 'soldExactlyTheSlice'),
        step.let('received', expression.subtract(balanceOf('destinationAta'), variable('proceedsBefore')), 'measureSliceReceived'),
        step.require(expression.greaterThanOrEqual(variable('received'), variable('sliceFloor')), 'sliceBeatTheOracle'),
        step.emit(
          [
            data.literal(SLICE_EVENT_TAG),
            data.encode('u8', expression.loopIndex()),
            data.encode('u64', variable('sold')),
            data.encode('u64', variable('received')),
          ],
          'logSlice',
        ),
        step.assign('totalReceived', expression.add(variable('totalReceived'), variable('received')), 'addToTotalReceived'),
      ],
      { max: MAX_SLICES, carry: ['totalReceived'], label: 'sellSlices' },
    ),
  ];
}

/** The run's return data: the total received, as a little-endian `u64`. */
const returnTotalReceived = () =>
  step.setReturnData([data.encode('u64', variable('totalReceived'))], 'returnTotalReceived');

/**
 * Pays each row its `amount` out of `payer`'s `source`, after requiring that the rows so far ask
 * no more than `bound` in all. The last row's check is the whole sum's.
 */
export function payoutSteps(options: { source: string; payer: string; bound: string; label: string }): Step[] {
  return [
    step.let('paid', expression.u64(0)),
    step.forEach(
      [
        step.assign('paid', expression.add(variable('paid'), expression.rowInput('amount')), 'addUpPayouts'),
        step.require(expression.lessThanOrEqual(variable('paid'), variable(options.bound)), options.label),
        tokenTransfer({
          tokenProgram: account.fixed('tokenProgram'),
          source: account.fixed(options.source),
          destination: account.iteration('recipientAta'),
          authority: account.fixed(options.payer),
          amount: expression.rowInput('amount'),
          label: 'payRecipient',
        }),
      ],
      { carry: ['paid'], label: 'payRecipients' },
    ),
  ];
}

/**
 * Scenario A: the split sell, then a payout of the proceeds to the rows, then the total received
 * as the run's return data. Two loops in one template, and the return data set last, after every
 * invoke.
 */
export const splitSellPayout = defineTemplate({
  inputs: splitSellInputs,
  accounts: splitSellAccounts,
  batch: payoutBatch,
  accountGroups: ['routeAccounts'],
  steps: [
    ...splitSellSteps(),
    ...payoutSteps({ source: 'destinationAta', payer: 'seller', bound: 'totalReceived', label: 'payoutsWithinProceeds' }),
    returnTotalReceived(),
  ],
});

/** Scenario B's inner run: the split sell alone, returning the total received. */
export const splitSellInner = defineTemplate({
  inputs: splitSellInputs,
  accounts: splitSellAccounts,
  accountGroups: ['routeAccounts'],
  steps: [...splitSellSteps(), returnTotalReceived()],
});
