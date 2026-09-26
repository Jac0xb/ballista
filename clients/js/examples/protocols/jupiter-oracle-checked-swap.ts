/**
 * Swap through Jupiter and prove the fill beat an independent oracle.
 *
 * Jupiter enforces the `slippageBps` it was given against the quote *it* produced. That protects
 * against movement between quote and execution; it does not protect against a bad quote, a
 * manipulated pool in the route, or a route built by someone other than the person signing.
 *
 * This template keeps a second opinion. It reads the Pyth price during execution, works out what
 * the input is worth at that price less a tolerance, runs the route, then measures the
 * destination account and requires the fill to clear that floor. Two independent sources have to
 * agree before the transaction is allowed to stand.
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
  JUPITER_V6,
  PYTH,
  PYTH_RECEIVER,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  addressBytes,
} from './shared.js';

export const jupiterOracleCheckedSwap = defineTemplate({
  inputs: {
    routeData: { type: 'bytes', maxLength: 512 },
    /** What is being sold, in the source token's base units. */
    amountIn: { type: 'u64' },
    /**
     * Scales the oracle price into output base units. The caller computes it from the two mints'
     * decimals and the feed's exponent; it is a property of the pair, not of the market.
     */
    priceScale: { type: 'u64' },
    /** How far below the oracle the fill may land, in basis points. */
    toleranceBps: { type: 'u64' },
  },
  accounts: {
    jupiter: { executable: true, address: addressBytes(JUPITER_V6) },
    priceUpdate: { owner: addressBytes(PYTH_RECEIVER), minDataLength: PYTH.length },
    trader: { signer: true, writable: true },
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

    step.let(
      'fairOut',
      expression.divide(
        expression.multiply(
          expression.multiply(expression.input('amountIn'), expression.input('priceScale')),
          expression.subtract(expression.u64(10_000), expression.input('toleranceBps')),
        ),
        expression.u64(10_000),
      ),
      'computeOracleFloor',
    ),

    step.snapshot(
      'balanceBefore',
      expression.accountData(account.fixed('destinationAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
      'readBalanceBeforeSwap',
    ),

    step.invoke({
      program: account.fixed('jupiter'),
      accounts: [{ account: account.fixed('trader'), signer: true, writable: true }],
      accountGroup: 'routeAccounts',
      data: [data.encode('bytes', expression.input('routeData'))],
      label: 'swap',
    }),

    step.require(
      expression.greaterThanOrEqual(
        expression.subtract(
          expression.accountData(account.fixed('destinationAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
          expression.snapshot('balanceBefore'),
        ),
        expression.variable('fairOut'),
      ),
      'fillBeatTheOracle',
    ),
  ],
});

export const compiled = compileTemplate(jupiterOracleCheckedSwap);
