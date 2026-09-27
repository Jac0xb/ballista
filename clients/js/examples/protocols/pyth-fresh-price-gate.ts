/**
 * Act only on a Pyth price that is fresh, confident, and inside a band — checked during
 * execution, not before signing.
 *
 * Inside a program this is `get_price_no_older_than`. A transaction cannot do it: it can read the
 * feed while it is being built, but the price it acts on is the one at execution, and the gap
 * between the two is the entire risk. Slots pass, the publisher stalls, the market moves.
 *
 * The first requirement is not about the price at all: it pins the account's verification level.
 * `VerificationLevel` is a Borsh enum whose `Full` variant is one byte and whose `Partial` is
 * two, so every field after it sits one byte earlier in a `Full` account than in a `Partial`
 * one. Reading a price at a fixed offset without checking the level is reading whichever field
 * happens to be there. Requiring `Full` fixes the layout and is the stronger guarantee besides.
 *
 * The action here is a Jupiter `route`. It takes the token program and the signer first, so the
 * template passes those two itself and the rest of the route's list arrives as a group.
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
import { JUPITER_ROUTE, JUPITER_V6, PYTH, PYTH_RECEIVER, addressBytes } from './shared.js';

/** Valid only once the verification level has been pinned to `Full`; see the require below. */
const price = expression.accountData(account.fixed('priceUpdate'), PYTH.price, 'i64');
const confidence = expression.accountData(account.fixed('priceUpdate'), PYTH.confidence, 'u64');
const publishTime = expression.accountData(account.fixed('priceUpdate'), PYTH.publishTime, 'i64');

export const pythFreshPriceGate = defineTemplate({
  inputs: {
    /** How stale a price may be, in seconds. */
    maximumAge: { type: 'i64' },
    /** The widest confidence interval the caller will act on. */
    maximumConfidence: { type: 'u64' },
    floorPrice: { type: 'i64' },
    ceilingPrice: { type: 'i64' },
    /** Jupiter's `route` arguments: the Swap API's instruction data after the discriminator. */
    actionData: { type: 'bytes', maxLength: 512 },
  },
  accounts: {
    /**
     * Pinning the owner is what makes the offsets meaningful: without it a caller could pass any
     * account whose bytes happen to satisfy the comparisons.
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

    step.invoke({
      program: account.fixed('actionProgram'),
      accounts: [
        { account: account.fixed('tokenProgram'), signer: false, writable: false },
        { account: account.fixed('actor'), signer: true, writable: false },
      ],
      accountGroup: 'actionAccounts',
      data: [data.literal(JUPITER_ROUTE), data.encode('bytes', expression.input('actionData'))],
      label: 'actOnTheOracle',
    }),
  ],
});

export const compiled = compileTemplate(pythFreshPriceGate);
