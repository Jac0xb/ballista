/** Sell a token account's whole balance through Jupiter: docs/examples/protocols/token-sweep.md. */
// #region template
import {
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  data,
  defineTemplate,
  expression,
  step,
} from '@jac0xb/ballista';
import {
  JUPITER_ROUTE,
  JUPITER_V6,
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  TOKEN_ACCOUNT_OWNER_OFFSET,
  addressBytes,
} from './shared.js';

const balanceOf = (name: string) =>
  expression.accountData(account.fixed(name), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64');

/** The route's platform fee account and rate are chosen by whoever builds the run: cap the rate. */
export const MAX_PLATFORM_FEE_BPS = 0n;

export const tokenSweepIntoSwap = defineTemplate({
  inputs: {
    /** `route_plan` as the Swap API encoded it: the bytes between the discriminator and `in_amount`. */
    routePlan: { type: 'bytes', maxLength: 512 },
    /** The `in_amount` the route was quoted for. */
    quotedInAmount: { type: 'u64' },
    /** The quote's `quoted_out_amount` for that input. */
    quotedOutAmount: { type: 'u64' },
    /** The quote's `slippage_bps`. */
    slippageBps: { type: 'u64' },
    /** The quote's `platform_fee_bps`. */
    platformFeeBps: { type: 'u64' },
    /** Do not sell less than this. */
    dustFloor: { type: 'u64' },
  },
  accounts: {
    jupiter: { executable: true, address: addressBytes(JUPITER_V6) },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    seller: { signer: true, writable: true },
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
    // Both ends of the sale are the seller's: the step that pays the proceeds can name any account
    // of the output mint, so the one measured must be the seller's.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('sourceAta'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('seller'), 'key'),
      ),
      'sweepsTheSellersOwnBalance',
    ),
    step.require(
      expression.equal(
        expression.accountData(account.fixed('destinationAta'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountField(account.fixed('seller'), 'key'),
      ),
      'proceedsGoToTheSeller',
    ),

    step.let('available', balanceOf('sourceAta'), 'readSellableBalance'),

    step.require(
      expression.greaterThan(expression.variable('available'), expression.input('dustFloor')),
      'worthSelling',
    ),

    // The quote was for `quotedInAmount`; selling `available` instead should fetch proportionally
    // more or less. Jupiter enforces its slippage against whatever quote the instruction carries.
    step.let(
      'quotedOut',
      expression.cast(
        'u64',
        expression.divide(
          expression.multiply(
            expression.cast('u128', expression.input('quotedOutAmount')),
            expression.cast('u128', expression.variable('available')),
          ),
          expression.cast('u128', expression.input('quotedInAmount')),
        ),
      ),
      'rescaleQuoteToBalance',
    ),

    step.snapshot('proceedsBefore', balanceOf('destinationAta'), 'readProceedsBefore'),

    // The fee account sits in the route's own accounts: any nonzero rate pays whoever chose it.
    step.require(
      expression.lessThanOrEqual(expression.input('platformFeeBps'), expression.u64(MAX_PLATFORM_FEE_BPS)),
      'platformFeeWithinCap',
    ),

    // `route` takes the token program, the signer, and the user's source and destination token
    // accounts first; the route's own accounts follow as the group.
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
        data.encode('bytes', expression.input('routePlan')),
        data.encode('u64', expression.variable('available')),
        data.encode('u64', expression.variable('quotedOut')),
        data.encode('u16', expression.input('slippageBps')),
        data.encode('u8', expression.input('platformFeeBps')),
      ],
      label: 'sell',
    }),

    // Jupiter checks this too. Checking it here, on the balances, holds whatever the route did.
    step.require(
      expression.greaterThanOrEqual(
        expression.subtract(balanceOf('destinationAta'), expression.snapshot('proceedsBefore')),
        expression.cast(
          'u64',
          expression.divide(
            expression.multiply(
              expression.cast('u128', expression.variable('quotedOut')),
              expression.cast(
                'u128',
                expression.subtract(expression.u64(10_000), expression.input('slippageBps')),
              ),
            ),
            expression.u128(10_000),
          ),
        ),
      ),
      'saleMetTheQuote',
    ),

    // The whole balance was the input, so anything left means the route did not take it all.
    step.require(
      expression.lessThanOrEqual(balanceOf('sourceAta'), expression.input('dustFloor')),
      'nothingMeaningfulLeftBehind',
    ),
  ],
});
// #endregion template

export const compiled = compileTemplate(tokenSweepIntoSwap);
