/**
 * Sell SOL for USDC through Jupiter, checked against Pyth's price:
 * docs/examples/protocols/jupiter-oracle-swap.md.
 */
// #region template
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
  JUPITER_ROUTE,
  JUPITER_V6,
  PYTH,
  PYTH_RECEIVER,
  SPL_MINT,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  TOKEN_ACCOUNT_MINT_OFFSET,
  TOKEN_ACCOUNT_OWNER_OFFSET,
  USDC_MINT,
  WRAPPED_SOL_MINT,
  addressBytes,
  pythFeedId,
} from './shared.js';

const balanceOf = (name: string) =>
  expression.accountData(account.fixed(name), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64');

/** The route's platform fee account and rate are chosen by whoever builds the run: cap the rate. */
export const MAX_PLATFORM_FEE_BPS = 0n;

/** The Pyth feed the price must come from: SOL/USD, which prices the SOL sold in the USDC bought. */
export const FEED_ID = pythFeedId('ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d');

/** 1%, in basis points: how far below the oracle's valuation the fill may land. */
export const TOLERANCE_BPS = 100n;

export const jupiterOracleCheckedSwap = defineTemplate({
  inputs: {
    /** `route_plan` as the Swap API encoded it: the bytes between the discriminator and `in_amount`. */
    routePlan: { type: 'bytes', maxLength: 512 },
    /** The route's `in_amount`: what the route sells, and exactly what must leave `sourceAta`. */
    inAmount: { type: 'u64' },
    /** The quote's `quoted_out_amount`. */
    quotedOutAmount: { type: 'u64' },
    /** The quote's `slippage_bps`. */
    slippageBps: { type: 'u64' },
    /** The quote's `platform_fee_bps`. */
    platformFeeBps: { type: 'u64' },
  },
  accounts: {
    jupiter: { executable: true, address: addressBytes(JUPITER_V6) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    priceUpdate: { owner: addressBytes(PYTH_RECEIVER), minDataLength: PYTH.length },
    trader: { signer: true, writable: true },
    sourceAta: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
    destinationAta: {
      writable: true,
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: TOKEN_ACCOUNT_LENGTH,
    },
    // The pair `FEED_ID` prices: what the route sells, and what it buys.
    sourceMint: {
      address: addressBytes(WRAPPED_SOL_MINT),
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: SPL_MINT.length,
    },
    destinationMint: {
      address: addressBytes(USDC_MINT),
      owner: TOKEN_PROGRAM_ADDRESS_BYTES,
      minDataLength: SPL_MINT.length,
    },
  },
  accountGroups: ['routeAccounts'],
  steps: [
    // Pin the verification level first: it decides where every other field sits.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('priceUpdate'), PYTH.verificationLevel, 'u8'),
        expression.u64(PYTH.verificationLevelFull),
      ),
      'priceIsFullyVerified',
    ),

    // The owner pin takes any feed's price; the feed id is what says which one this is.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('priceUpdate'), PYTH.feedId, 'pubkey'),
        expression.pubkey(FEED_ID),
      ),
      'priceIsTheExpectedFeed',
    ),

    step.require(
      expression.lessThanOrEqual(
        expression.subtract(
          expression.clockUnixTimestamp(),
          expression.accountData(account.fixed('priceUpdate'), PYTH.publishTime, 'i64'),
        ),
        expression.i64(60),
      ),
      'oracleIsFresh',
    ),

    // Each token account must hold the mint whose decimals scale it.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('sourceAta'), TOKEN_ACCOUNT_MINT_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('sourceMint'), 'key'),
      ),
      'sourceHoldsTheSourceMint',
    ),
    step.require(
      expression.equal(
        expression.accountData(account.fixed('destinationAta'), TOKEN_ACCOUNT_MINT_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('destinationMint'), 'key'),
      ),
      'destinationHoldsTheDestinationMint',
    ),

    // Both ends of the swap are the trader's: the step that pays the fill can name any account of
    // the destination mint, so the one measured must be the trader's. The key is read once: the
    // template is at the runtime's 64 registers.
    step.let('traderKey', expression.accountKey('trader')),
    step.require(
      expression.equal(
        expression.accountData(account.fixed('sourceAta'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.variable('traderKey'),
      ),
      'sellsTheTradersOwnTokens',
    ),
    step.require(
      expression.equal(
        expression.accountData(account.fixed('destinationAta'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.variable('traderKey'),
      ),
      'proceedsGoToTheTrader',
    ),

    // price × 10^exponent is per whole token. In base units the fill is worth
    // sold × price × 10^(destinationDecimals + exponent − sourceDecimals). The exponent is an
    // i32 and usually negative, so split the power into a multiplier and a divisor, each ≥ 0,
    // and let multiplyDivide apply both exactly.
    step.let(
      'scale',
      expression.subtract(
        expression.add(
          expression.cast('i64', expression.accountData(account.fixed('destinationMint'), SPL_MINT.decimals, 'u8')),
          expression.accountData(account.fixed('priceUpdate'), PYTH.exponent, 'i32'),
        ),
        expression.cast('i64', expression.accountData(account.fixed('sourceMint'), SPL_MINT.decimals, 'u8')),
      ),
      'computeDecimalScale',
    ),

    // Pyth prices are signed; a negative or zero price means the feed is unusable here.
    step.let(
      'oraclePrice',
      expression.accountData(account.fixed('priceUpdate'), PYTH.price, 'i64'),
      'readOraclePrice',
    ),
    step.require(
      expression.greaterThan(expression.variable('oraclePrice'), expression.i64(0)),
      'oraclePriceIsPositive',
    ),

    step.snapshot('sourceBefore', balanceOf('sourceAta'), 'readSourceBeforeSwap'),
    step.snapshot('balanceBefore', balanceOf('destinationAta'), 'readBalanceBeforeSwap'),

    // The fee account sits in the route's own accounts: any nonzero rate pays whoever chose it.
    step.require(
      expression.lessThanOrEqual(expression.input('platformFeeBps'), expression.u64(MAX_PLATFORM_FEE_BPS)),
      'platformFeeWithinCap',
    ),

    // `route` takes the token program, the signer, and the user's source and destination token
    // accounts first; the route's own accounts follow as the group. Jupiter moves the accounts its
    // steps name, not necessarily these two, which is why `soldTheRouteInput` below exists.
    step.invoke({
      program: account.fixed('jupiter'),
      accounts: [
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('trader'), signer: true, writable: false },
        { account: account.fixed('sourceAta'), signer: false, writable: true },
        { account: account.fixed('destinationAta'), signer: false, writable: true },
      ],
      accountGroup: 'routeAccounts',
      data: [
        data.literal(JUPITER_ROUTE),
        data.encode('bytes', expression.input('routePlan')),
        data.encode('u64', expression.input('inAmount')),
        data.encode('u64', expression.input('quotedOutAmount')),
        data.encode('u16', expression.input('slippageBps')),
        data.encode('u8', expression.input('platformFeeBps')),
      ],
      label: 'swap',
    }),

    // What actually left, whatever the route data claimed it would sell.
    step.let(
      'sold',
      expression.subtract(expression.snapshot('sourceBefore'), balanceOf('sourceAta')),
      'measureAmountSold',
    ),

    // The route sold `inAmount`, so that much must have left the account measured. If the steps
    // moved other accounts, `sold` is 0 and so is the floor below, which any fill would clear.
    step.require(
      expression.equal(expression.variable('sold'), expression.input('inAmount')),
      'soldTheRouteInput',
    ),

    // sold × price, scaled by 10^scale, is the fill at the oracle price in destination base
    // units; multiplyDivide computes the exact product and applies it, then the floor keeps all but
    // `TOLERANCE_BPS` of it. sold × price fits u128 because both factors are below 2^64. Using max
    // with zero means at least one of the two powers of ten below is 1, so no select is needed: a
    // select evaluates both branches, and the unused one would fail its cast.
    step.let(
      'fairOut',
      expression.cast(
        'u64',
        expression.multiplyDivide(
          expression.multiplyDivide(
            expression.multiply(
              expression.cast('u128', expression.variable('sold')),
              expression.cast('u128', expression.variable('oraclePrice')),
            ),
            expression.powerOfTen(expression.cast('u64', expression.max(expression.variable('scale'), expression.i64(0)))),
            expression.powerOfTen(
              expression.cast('u64', expression.max(expression.subtract(expression.i64(0), expression.variable('scale')), expression.i64(0))),
            ),
          ),
          expression.u128(10_000n - TOLERANCE_BPS),
          expression.u128(10_000),
        ),
      ),
      'computeOracleFloor',
    ),

    step.require(
      expression.greaterThanOrEqual(
        expression.subtract(balanceOf('destinationAta'), expression.snapshot('balanceBefore')),
        expression.variable('fairOut'),
      ),
      'fillBeatTheOracle',
    ),
  ],
});
// #endregion template

export const compiled = compileTemplate(jupiterOracleCheckedSwap);
