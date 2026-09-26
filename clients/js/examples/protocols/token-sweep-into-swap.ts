/**
 * Sell an entire token balance, whatever it turns out to be.
 *
 * This is the smallest useful shape on the list and the one that appears everywhere: a fee
 * account, an airdrop claim, a vesting withdrawal, the dust left over from a route. The balance
 * is still moving when the transaction is signed, so the amount to sell does not exist yet.
 *
 * A route built with a fixed input amount fails when the balance came up short and strands the
 * difference when it came up long. Here the swap's input amount is read out of the account, and
 * the run refuses to bother below a floor.
 *
 * The amount at offset 64 is the raw SPL Token balance. Under Token-2022's transfer-fee
 * extension part of that can be withheld and unspendable, tracked separately by the
 * `TransferFeeAmount` extension past the 165-byte base layout, so for a fee-bearing mint read
 * the withheld amount from the extension and subtract it.
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
  TOKEN_ACCOUNT_AMOUNT_OFFSET,
  TOKEN_ACCOUNT_LENGTH,
  addressBytes,
} from './shared.js';

export const tokenSweepIntoSwap = defineTemplate({
  inputs: {
    /**
     * The route, built for a nominal input. Jupiter reads its own amount from this data, so the
     * caller quotes for roughly the expected balance; the guard below is what makes the
     * difference safe rather than the route.
     */
    routeData: { type: 'bytes', maxLength: 512 },
    /** Do not sell less than this. */
    dustFloor: { type: 'u64' },
    /** And do not accept less than this for it. */
    minimumOut: { type: 'u64' },
  },
  accounts: {
    jupiter: { executable: true, address: addressBytes(JUPITER_V6) },
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
    step.let(
      'available',
      expression.accountData(account.fixed('sourceAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
      'readSellableBalance',
    ),

    step.require(
      expression.greaterThan(expression.variable('available'), expression.input('dustFloor')),
      'worthSelling',
    ),

    step.snapshot(
      'proceedsBefore',
      expression.accountData(account.fixed('destinationAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
      'readProceedsBefore',
    ),

    step.invoke({
      program: account.fixed('jupiter'),
      accounts: [{ account: account.fixed('seller'), signer: true, writable: true }],
      accountGroup: 'routeAccounts',
      data: [data.encode('bytes', expression.input('routeData'))],
      label: 'sell',
    }),

    step.require(
      expression.greaterThanOrEqual(
        expression.subtract(
          expression.accountData(account.fixed('destinationAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
          expression.snapshot('proceedsBefore'),
        ),
        expression.input('minimumOut'),
      ),
      'saleMetItsFloor',
    ),

    // Whatever the route left behind is dust by construction: it was below the floor or the
    // route could not take it. Nothing is stranded silently, because the balance is re-read.
    step.require(
      expression.lessThanOrEqual(
        expression.accountData(account.fixed('sourceAta'), TOKEN_ACCOUNT_AMOUNT_OFFSET, 'u64'),
        expression.input('dustFloor'),
      ),
      'nothingMeaningfulLeftBehind',
    ),
  ],
});

export const compiled = compileTemplate(tokenSweepIntoSwap);
