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
 * worth `sold × price × 10^(destinationDecimals + exponent − sourceDecimals)`. The template reads
 * the feed's exponent and both mints' decimals on chain and computes that scale itself, and checks
 * each token account against the mint it is supposed to hold so a caller cannot point the decimals
 * read at the wrong mint. The caller supplies only the route and the tolerance.
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
  SPL_MINT,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  TOKEN_ACCOUNT_MINT_OFFSET,
  addressBytes,
} from './shared.js';

const balanceOf = (name: string) =>
  expression.accountData(account.fixed(name), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64');

export const jupiterOracleCheckedSwap = defineTemplate({
  inputs: {
    /** Jupiter's `route` arguments: the Swap API's instruction data after the discriminator. */
    routeArgs: { type: 'bytes', maxLength: 512 },
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
    sourceMint: { owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: SPL_MINT.length },
    destinationMint: { owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: SPL_MINT.length },
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

    // sold × price, scaled by 10^scale, is the fill at the oracle price in destination base
    // units; multiplyDivide computes the exact product and applies it. sold × price fits u128
    // because both factors are below 2^64. Using max with zero means at least one of the two
    // powers of ten below is 1, so no select is needed — a select evaluates both branches, and
    // the unused one would fail its cast.
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
          expression.cast('u128', expression.subtract(expression.u64(10_000), expression.input('toleranceBps'))),
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
