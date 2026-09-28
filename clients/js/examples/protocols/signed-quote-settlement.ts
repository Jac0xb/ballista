/**
 * Settle a maker's signed quote: the taker pays the quoted price, the maker delivers, and neither
 * side can stretch the quote past what the maker signed.
 *
 * The maker signs a 120-byte quote off chain: a price, the most it will sell, an expiry, the one
 * taker the quote is for, and the two mints. The taker puts the Ed25519 precompile instruction
 * that carries the signature directly before this template's run. The precompile verifies the
 * signature as part of the transaction, so a bad one fails it. The template checks that the
 * signature is the maker's, over a quote of this shape, and then holds the fill to the quote:
 * before the expiry, for this taker, no more than the maximum, in these mints, paid into an
 * account the maker owns, at the signed price rounded up in the maker's favour.
 *
 * Ballista does not control the maker's tokens: the maker's authority co-signs the transaction.
 * Because the template enforces the terms, the service that co-signs checks only that the
 * transaction runs this template and nothing else that could spend the maker's accounts. Ballista
 * keeps no state, so it cannot count fills. A quote can be settled again until it expires unless
 * the co-signer refuses a second settlement of the same quote.
 */
import {
  INSTRUCTIONS_SYSVAR_ADDRESS_BYTES,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  defineTemplate,
  ed25519Signature,
  expression,
  step,
  tokenTransfer,
} from '../../src/index.js';
import { TOKEN_ACCOUNT_LENGTH, TOKEN_ACCOUNT_MINT_OFFSET, TOKEN_ACCOUNT_OWNER_OFFSET } from './shared.js';

/** The signed quote. Integers are little-endian; keys are their 32 raw bytes. */
export const QUOTE = {
  length: 120,
  /** Quote-token base units per `PRICE_SCALE` base-token base units. */
  price: 0,
  /** The most base-token base units the maker delivers. */
  maxAmount: 8,
  /** The last Unix timestamp at which the quote can settle. */
  expiry: 16,
  /** The one wallet that can take the quote. */
  taker: 24,
  /** The mint the maker delivers. */
  baseMint: 56,
  /** The mint the taker pays in. */
  quoteMint: 88,
} as const;

/** Prices carry six decimals: a price of 1,000,000 is one quote unit per base unit. */
export const PRICE_SCALE = 1_000_000n;

const instructions = account.fixed('instructions');
const taker = account.fixed('taker');
const maker = account.fixed('maker');

/** The maker's signature, in the instruction directly before this template's run. */
export const signedQuote = ed25519Signature({
  sysvar: instructions,
  index: expression.subtract(expression.currentInstructionIndex(instructions), expression.u64(1)),
  signer: expression.accountField(maker, 'key'),
  messageLength: QUOTE.length,
  name: 'quote',
});

const tokenAccount = { writable: true, owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: TOKEN_ACCOUNT_LENGTH };

export const signedQuoteSettlement = defineTemplate({
  inputs: {
    /** Base-token base units to take, up to the quoted maximum. */
    amount: { type: 'u64' },
  },
  accounts: {
    instructions: { address: INSTRUCTIONS_SYSVAR_ADDRESS_BYTES },
    tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
    taker: { signer: true },
    maker: { signer: true },
    /** Pays, in the quote mint. */
    takerQuoteAccount: tokenAccount,
    /** Is paid, in the quote mint. */
    makerQuoteAccount: tokenAccount,
    /** Delivers, in the base mint. */
    makerBaseAccount: tokenAccount,
    /** Receives, in the base mint. */
    takerBaseAccount: tokenAccount,
  },
  steps: [
    ...signedQuote.steps,

    step.require(
      expression.lessThanOrEqual(expression.clockUnixTimestamp(), signedQuote.field(QUOTE.expiry, 'i64')),
      'quoteHasNotExpired',
    ),
    step.require(
      expression.equal(signedQuote.field(QUOTE.taker, 'pubkey'), expression.accountField(taker, 'key')),
      'quoteIsForThisTaker',
    ),
    step.require(
      expression.lessThanOrEqual(expression.input('amount'), signedQuote.field(QUOTE.maxAmount, 'u64')),
      'withinTheQuotedSize',
    ),

    // A token `transfer` moves only between two accounts of one mint, so pinning one side of each
    // leg pins both.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('takerQuoteAccount'), TOKEN_ACCOUNT_MINT_OFFSET, 'pubkey'),
        signedQuote.field(QUOTE.quoteMint, 'pubkey'),
      ),
      'paysInTheQuotedMint',
    ),
    step.require(
      expression.equal(
        expression.accountData(account.fixed('makerBaseAccount'), TOKEN_ACCOUNT_MINT_OFFSET, 'pubkey'),
        signedQuote.field(QUOTE.baseMint, 'pubkey'),
      ),
      'deliversTheQuotedMint',
    ),
    // The payment reaches an account the maker owns, not one the taker picked.
    step.require(
      expression.equal(
        expression.accountData(account.fixed('makerQuoteAccount'), TOKEN_ACCOUNT_OWNER_OFFSET, 'pubkey'),
        expression.accountField(maker, 'key'),
      ),
      'paymentReachesTheMaker',
    ),

    step.let(
      'payment',
      expression.multiplyDivide(
        expression.input('amount'),
        signedQuote.field(QUOTE.price, 'u64'),
        expression.u64(PRICE_SCALE),
        'up',
      ),
      'priceTheFill',
    ),
    tokenTransfer({
      tokenProgram: account.fixed('tokenProgram'),
      source: account.fixed('takerQuoteAccount'),
      destination: account.fixed('makerQuoteAccount'),
      authority: taker,
      amount: expression.variable('payment'),
      label: 'takerPays',
    }),
    tokenTransfer({
      tokenProgram: account.fixed('tokenProgram'),
      source: account.fixed('makerBaseAccount'),
      destination: account.fixed('takerBaseAccount'),
      authority: maker,
      amount: expression.input('amount'),
      label: 'makerDelivers',
    }),
  ],
});

export const compiled = compileTemplate(signedQuoteSettlement);
