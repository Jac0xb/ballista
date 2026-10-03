/**
 * Run a Jupiter swap only on a fresh, confident Pyth price in a band:
 * docs/examples/protocols/pyth-gate.md.
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
import { JUPITER_ROUTE, JUPITER_V6, PYTH, PYTH_RECEIVER, addressBytes } from './shared.js';

/** Valid only once the verification level has been pinned to `Full`; see the require below. */
const price = expression.accountData(account.fixed('priceUpdate'), PYTH.price, 'i64');
const confidence = expression.accountData(account.fixed('priceUpdate'), PYTH.confidence, 'u64');
const publishTime = expression.accountData(account.fixed('priceUpdate'), PYTH.publishTime, 'i64');

/** The route's platform fee account and rate are chosen by whoever builds the run: cap the rate. */
export const MAX_PLATFORM_FEE_BPS = 0n;

export const pythFreshPriceGate = defineTemplate({
  inputs: {
    /**
     * The Pyth feed the price must come from, as its 32-byte id: SOL/USD's is
     * `ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d`.
     */
    feedId: { type: 'pubkey' },
    /**
     * The feed's exponent, which the three bounds below are in units of: SOL/USD's is −8. The
     * account holds it as an i32.
     */
    exponent: { type: 'i64' },
    /** How stale a price may be, in seconds. */
    maximumAge: { type: 'i64' },
    /** The widest confidence interval the caller will act on. */
    maximumConfidence: { type: 'u64' },
    floorPrice: { type: 'i64' },
    ceilingPrice: { type: 'i64' },
    /** `route_plan` as the Swap API encoded it: the bytes between the discriminator and `in_amount`. */
    routePlan: { type: 'bytes', maxLength: 512 },
    /** The route's `in_amount`. */
    inAmount: { type: 'u64' },
    /** The quote's `quoted_out_amount`. */
    quotedOutAmount: { type: 'u64' },
    /** The quote's `slippage_bps`. */
    slippageBps: { type: 'u64' },
    /** The quote's `platform_fee_bps`, at most `MAX_PLATFORM_FEE_BPS`. */
    platformFeeBps: { type: 'u64' },
  },
  accounts: {
    /**
     * Pinning the owner is what makes the offsets meaningful: without it a caller could pass any
     * account whose bytes happen to satisfy the comparisons. It does not say which feed the price
     * belongs to; `priceIsTheExpectedFeed` does.
     */
    priceUpdate: { owner: addressBytes(PYTH_RECEIVER), minDataLength: PYTH.length },
    actionProgram: { executable: true, address: addressBytes(JUPITER_V6) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    actor: { signer: true, writable: true },
  },
  accountGroups: ['actionAccounts'],
  steps: [
    // Fixes the layout. Without this the offsets below are a guess.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('priceUpdate'), PYTH.verificationLevel, 'u8'),
        expression.u64(PYTH.verificationLevelFull),
      ),
      'priceIsFullyVerified',
    ),

    // Which feed the price belongs to. Its offset, like the rest, assumes the level just pinned.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('priceUpdate'), PYTH.feedId, 'pubkey'),
        expression.input('feedId'),
      ),
      'priceIsTheExpectedFeed',
    ),

    // What the raw integers below mean. At another exponent each bound is off by a power of ten.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('priceUpdate'), PYTH.exponent, 'i32'),
        expression.input('exponent'),
      ),
      'priceExponentIsExpected',
    ),

    step.require(
      expression.lessThanOrEqual(
        expression.subtract(expression.clockUnixTimestamp(), publishTime),
        expression.input('maximumAge'),
      ),
      'priceIsFresh',
    ),

    // A wide confidence interval means the publishers disagree; treat it as no price at all.
    step.require(
      expression.lessThanOrEqual(confidence, expression.input('maximumConfidence')),
      'publishersAgree',
    ),

    step.require(expression.greaterThanOrEqual(price, expression.input('floorPrice')), 'priceAboveFloor'),
    step.require(expression.lessThanOrEqual(price, expression.input('ceilingPrice')), 'priceBelowCeiling'),

    // The fee account sits in the route's own accounts: any nonzero rate pays whoever chose it.
    step.require(
      expression.lessThanOrEqual(expression.input('platformFeeBps'), expression.u64(MAX_PLATFORM_FEE_BPS)),
      'platformFeeWithinCap',
    ),
    step.invoke({
      program: account.fixed('actionProgram'),
      accounts: [
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('actor'), signer: true, writable: false },
      ],
      accountGroup: 'actionAccounts',
      data: [
        data.literal(JUPITER_ROUTE),
        data.encode('bytes', expression.input('routePlan')),
        data.encode('u64', expression.input('inAmount')),
        data.encode('u64', expression.input('quotedOutAmount')),
        data.encode('u16', expression.input('slippageBps')),
        data.encode('u8', expression.input('platformFeeBps')),
      ],
      label: 'actOnTheOracle',
    }),
  ],
});
// #endregion template

export const compiled = compileTemplate(pythFreshPriceGate);
