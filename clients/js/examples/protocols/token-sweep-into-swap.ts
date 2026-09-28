/**
 * Sell an entire token balance, whatever it turns out to be.
 *
 * This is the smallest useful shape on the list and the one that appears everywhere: a fee
 * account, an airdrop claim, a vesting withdrawal, the dust left over from a route. The balance
 * is still moving when the transaction is signed, so the amount to sell does not exist yet.
 *
 * A route built with a fixed input amount fails when the balance came up short and strands the
 * difference when it came up long. Jupiter's `route` carries that amount as `in_amount`, after
 * the route plan and before the quote. So the caller hands over the route plan and the quote's
 * own numbers separately, and the template writes the instruction itself: the balance it reads
 * becomes `in_amount`, and the quoted output is rescaled to match. The plan splits its input by
 * percentage, so the same plan sells more or less. How far the balance may drift above the quote
 * is bounded twice: the rescaling is linear, so the price impact of the extra size has to fit
 * within `slippageBps`, and the swap has to stay within the pools and tick arrays the route's
 * accounts cover.
 *
 * Both token accounts are pinned to the legacy SPL Token program, so a Token-2022 account fails
 * the owner check instead of being read with the wrong layout.
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
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  addressBytes,
} from './shared.js';

const balanceOf = (name: string) =>
  expression.accountData(account.fixed(name), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64');

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

export const compiled = compileTemplate(tokenSweepIntoSwap);
