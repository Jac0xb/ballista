/**
 * Swap through Jupiter and prove the fill beat an independent oracle.
 *
 * Jupiter enforces the `slippageBps` it was given against the quote *it* produced. That protects
 * against movement between quote and execution; it does not protect against a bad quote, a
 * manipulated pool in the route, or a route built by someone other than the person signing.
 *
 * This template keeps a second opinion. It reads the Pyth price during execution, runs the route,
 * then measures how much actually left the source account and how much arrived, and requires the
 * fill to clear what the sold amount was worth at the oracle price less a tolerance. Two
 * independent sources have to agree before the transaction is allowed to stand.
 *
 * The feed must price the token being sold in the token being bought: SOL/USD when selling SOL
 * for USDC. Pyth's price is `price × 10^exponent` per whole token, so a fill in base units is
 * worth `sold × price / 10^(sourceDecimals − destinationDecimals − exponent)`. The caller computes
 * that divisor once for the pair, and the template pins the exponent it assumes: a feed whose
 * exponent changes fails the run instead of being priced a thousand times off.
 *
 * A transaction cannot express this: the fill is only known after the route runs, and by then
 * every instruction is already committed.
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
  JUPITER_ROUTE,
  JUPITER_V6,
  PYTH,
  PYTH_RECEIVER,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  addressBytes,
} from './shared.js';

const balanceOf = (name: string) =>
  expression.accountData(account.fixed(name), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64');

export const jupiterOracleCheckedSwap = defineTemplate({
  inputs: {
    /** Jupiter's `route` arguments: the Swap API's instruction data after the discriminator. */
    routeArgs: { type: 'bytes', maxLength: 512 },
    /** The feed's exponent, which `scaleDivisor` was computed for. Pyth's are negative. */
    priceExponent: { type: 'i64' },
    /** `10^(sourceDecimals − destinationDecimals − priceExponent)`, a property of the pair. */
    scaleDivisor: { type: 'u128' },
    /** How far below the oracle the fill may land, in basis points. */
    toleranceBps: { type: 'u64' },
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

    // The exponent is an i32. Read its bits as a u32 and compare them with the two's-complement
    // encoding of the one the divisor assumes: 2^32 + exponent, for a negative exponent.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('priceUpdate'), PYTH.exponent, 'u32'),
        expression.cast('u64', expression.add(expression.i64(1n << 32n), expression.input('priceExponent'))),
      ),
      'exponentIsTheOneTheDivisorAssumes',
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

    // `route` takes the token program, the signer, and the user's source and destination token
    // accounts first. Passing the two token accounts here means the accounts Jupiter moves are the
    // ones this template measures; the route's own accounts follow as the group.
    step.invoke({
      program: account.fixed('jupiter'),
      accounts: [
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('trader'), signer: true, writable: false },
        { account: account.fixed('sourceAta'), signer: false, writable: true },
        { account: account.fixed('destinationAta'), signer: false, writable: true },
      ],
      accountGroup: 'routeAccounts',
      data: [data.literal(JUPITER_ROUTE), data.encode('bytes', expression.input('routeArgs'))],
      label: 'swap',
    }),

    // What actually left, whatever the route data claimed it would sell.
    step.let(
      'sold',
      expression.subtract(expression.snapshot('sourceBefore'), balanceOf('sourceAta')),
      'measureAmountSold',
    ),

    // sold × price / divisor is the fill at the oracle price, in destination base units. Dividing
    // before applying the tolerance keeps the product inside u128, and rounds the floor down.
    step.let(
      'fairOut',
      expression.cast(
        'u64',
        expression.divide(
          expression.multiply(
            expression.divide(
              expression.multiply(
                expression.cast('u128', expression.variable('sold')),
                expression.cast('u128', expression.variable('oraclePrice')),
              ),
              expression.input('scaleDivisor'),
            ),
            expression.cast(
              'u128',
              expression.subtract(expression.u64(10_000), expression.input('toleranceBps')),
            ),
          ),
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

export const compiled = compileTemplate(jupiterOracleCheckedSwap);
