/**
 * Settle at a signed quote: docs/examples/protocols/signed-quote.md.
 *
 * This template keeps no state, so it can't count settlements: a quote can settle again until it
 * expires. A template that must refuse a second settlement can count them in a registry entry.
 */
// #region template
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
} from '@jac0xb/ballista';
import { TOKEN_ACCOUNT_LENGTH, TOKEN_ACCOUNT_MINT_OFFSET, TOKEN_ACCOUNT_OWNER_OFFSET } from './shared.js';

/** The signed quote. Integers are little-endian; keys are their 32 raw bytes. */
export const QUOTE = {
  length: 128,
  /** `QUOTE_TAG`, marking the message as a settlement quote. */
  tag: 0,
  /** Quote-token base units per `PRICE_SCALE` base-token base units. */
  price: 8,
  /** The most base-token base units the maker delivers in one settlement. The quote can settle
   * again until it expires, so this bounds each settlement, not the total. */
  maxAmount: 16,
  /** The last Unix timestamp at which the quote can settle. */
  expiry: 24,
  /** The one wallet that can take the quote. */
  taker: 32,
  /** The mint the maker delivers. */
  baseMint: 64,
  /** The mint the taker pays in. */
  quoteMint: 96,
} as const;

/** The eight bytes every quote starts with. */
export const QUOTE_TAG = new TextEncoder().encode('BLSTQT01');

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
      expression.equal(
        signedQuote.field(QUOTE.tag, 'u64'),
        expression.u64(new DataView(QUOTE_TAG.buffer).getBigUint64(0, true)),
      ),
      'quoteIsTagged',
    ),
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
// #endregion template
