import { address, getAddressEncoder } from '@solana/kit';
import { describe, expect, test } from 'vitest';

import {
  BALLISTA_PROGRAM_ADDRESS,
  account,
  addressBytes,
  anchorDiscriminator,
  compileTemplate,
  defineTemplate,
  expression,
  step,
} from './index.js';

describe('addressBytes', () => {
  test('gives the 32 bytes Kit encodes, as a Uint8Array a template accepts', () => {
    const encoded = getAddressEncoder().encode(address(BALLISTA_PROGRAM_ADDRESS));
    const bytes = addressBytes(address(BALLISTA_PROGRAM_ADDRESS));
    expect(bytes).toEqual(Uint8Array.from(encoded));
    expect(addressBytes('11111111111111111111111111111111')).toEqual(new Uint8Array(32));
    // `encoded` is a ReadonlyUint8Array, which `address` and `expression.pubkey` refuse in tsc.
    const template = defineTemplate({
      accounts: { ballista: { executable: true, address: bytes } },
      steps: [step.require(expression.equal(expression.accountKey('ballista'), expression.pubkey(bytes)))],
    });
    expect(compileTemplate(template).stats.instructions).toBeGreaterThan(0);
  });

  test('refuses text that is not 32 bytes of base58', () => {
    expect(() => addressBytes('not base58: 0OIl')).toThrow(TypeError);
    expect(() => addressBytes('1111')).toThrow(/32-byte/);
    expect(() => addressBytes(`${BALLISTA_PROGRAM_ADDRESS}z`)).toThrow(/32-byte/);
  });
});

describe('anchorDiscriminator', () => {
  test('is the first 8 bytes of sha256("global:<name>")', () => {
    // Jupiter v6 `route`, as its instruction data starts.
    expect(Buffer.from(anchorDiscriminator('route')).toString('hex')).toBe('e517cb977ae3ad2a');
    expect(anchorDiscriminator('collect_fees')).toHaveLength(8);
  });
});
