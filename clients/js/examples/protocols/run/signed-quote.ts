import { getAddressDecoder, getAddressEncoder, type Address, type Instruction } from '@solana/kit';

import { ED25519_PROGRAM_ADDRESS_BYTES, INSTRUCTIONS_SYSVAR_ADDRESS_BYTES } from '../../../src/index.js';
import { buildKitRunInstruction } from '../../../src/kit.js';
import { QUOTE, QUOTE_TAG, compiled } from '../signed-quote-settlement.js';
import { TOKEN_PROGRAM, at, pinned } from './programs.js';

const ED25519_PROGRAM = getAddressDecoder().decode(ED25519_PROGRAM_ADDRESS_BYTES);
const INSTRUCTIONS_SYSVAR = getAddressDecoder().decode(INSTRUCTIONS_SYSVAR_ADDRESS_BYTES);

/** The quote a maker signs off chain. */
export interface Quote {
  /** Quote-token base units per 1,000,000 base-token base units. */
  price: bigint;
  /** The most base-token base units the maker delivers. */
  maxAmount: bigint;
  /** The last Unix timestamp at which the quote can settle. */
  expiry: bigint;
  /** The one wallet that can take the quote. */
  taker: Address;
  /** The mint the maker delivers, and the mint the taker pays in. */
  baseMint: Address;
  quoteMint: Address;
}

/** The 128 bytes the maker signs: the tag `BLSTQT01`, then the fields, integers little-endian. */
export function quoteMessage(quote: Quote): Uint8Array {
  const encoder = getAddressEncoder();
  const message = new Uint8Array(QUOTE.length);
  const view = new DataView(message.buffer);
  message.set(QUOTE_TAG, QUOTE.tag);
  view.setBigUint64(QUOTE.price, quote.price, true);
  view.setBigUint64(QUOTE.maxAmount, quote.maxAmount, true);
  view.setBigInt64(QUOTE.expiry, quote.expiry, true);
  message.set(encoder.encode(quote.taker), QUOTE.taker);
  message.set(encoder.encode(quote.baseMint), QUOTE.baseMint);
  message.set(encoder.encode(quote.quoteMint), QUOTE.quoteMint);
  return message;
}

/**
 * The Ed25519 precompile instruction that verifies `signature`, `signer`'s over `message`. It
 * holds one signature, with the key, the signature and the message in its own data.
 */
export function buildEd25519Instruction(input: {
  signer: Address;
  signature: Uint8Array;
  message: Uint8Array;
}): Instruction {
  if (input.signature.length !== 64) throw new RangeError('An Ed25519 signature is 64 bytes');
  // 0xffff as an instruction index: the bytes are in this instruction's own data.
  const OWN_DATA = 0xffff;
  const keyOffset = 2 + 14;
  const signatureOffset = keyOffset + 32;
  const messageOffset = signatureOffset + 64;
  const data = new Uint8Array(messageOffset + input.message.length);
  const view = new DataView(data.buffer);
  data[0] = 1; // one signature, then a padding byte
  const offsets = [signatureOffset, OWN_DATA, keyOffset, OWN_DATA, messageOffset, input.message.length, OWN_DATA];
  offsets.forEach((field, index) => view.setUint16(2 + 2 * index, field, true));
  data.set(getAddressEncoder().encode(input.signer), keyOffset);
  data.set(input.signature, signatureOffset);
  data.set(input.message, messageOffset);
  return { programAddress: ED25519_PROGRAM, data };
}

/**
 * The transaction's two instructions, in order: the Ed25519 instruction carrying the maker's
 * signature over the quote, and the run, which reads the signature from the instruction directly
 * before it. The taker and the maker both sign the transaction.
 */
export function buildSignedQuoteRun(input: {
  templateAddress: Address;
  taker: Address;
  maker: Address;
  /** Pays, in the quote mint. */
  takerQuoteAccount: Address;
  /** Is paid, in the quote mint: the maker's own account. */
  makerQuoteAccount: Address;
  /** Delivers, in the base mint. */
  makerBaseAccount: Address;
  /** Receives, in the base mint. */
  takerBaseAccount: Address;
  quote: Quote;
  /** The maker's 64-byte Ed25519 signature over `quoteMessage(quote)`. */
  signature: Uint8Array;
  /** Base-token base units to take, up to the quote's `maxAmount`. */
  amount: bigint;
}): [Instruction, Instruction] {
  const verify = buildEd25519Instruction({
    signer: input.maker,
    signature: input.signature,
    message: quoteMessage(input.quote),
  });
  const run = buildKitRunInstruction({
    compiled,
    templateAddress: input.templateAddress,
    inputs: { amount: input.amount },
    accounts: {
      instructions: pinned(INSTRUCTIONS_SYSVAR),
      tokenProgram: pinned(TOKEN_PROGRAM),
      taker: at(input.taker),
      maker: at(input.maker),
      takerQuoteAccount: at(input.takerQuoteAccount),
      makerQuoteAccount: at(input.makerQuoteAccount),
      makerBaseAccount: at(input.makerBaseAccount),
      takerBaseAccount: at(input.takerBaseAccount),
    },
  });
  return [verify, run];
}
