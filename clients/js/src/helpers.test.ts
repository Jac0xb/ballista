import { address, getAddressEncoder } from '@solana/kit';
import { describe, expect, test } from 'vitest';

import {
  BALLISTA_PROGRAM_ADDRESS,
  TOKEN_2022_PROGRAM_ADDRESS_BYTES,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  addressBytes,
  anchorDiscriminator,
  compileTemplate,
  defineTemplate,
  expression,
  rateLimit,
  step,
  systemTransfer,
  type Step,
} from './index.js';

describe('addressBytes', () => {
  test('the token program constants are the bytes of their addresses', () => {
    expect(TOKEN_PROGRAM_ADDRESS_BYTES).toEqual(addressBytes('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'));
    expect(TOKEN_2022_PROGRAM_ADDRESS_BYTES).toEqual(addressBytes('TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb'));
  });

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

describe('rateLimit in a template that keeps two limits', () => {
  const limit = (registry: string, fields: { spent?: string; lastSpend?: string; name?: string } = {}): Step[] =>
    rateLimit({
      registry,
      cap: expression.u64(1_000_000_000),
      refillPerSecond: expression.u64(11_574),
      amount: expression.input('amount'),
      ...fields,
    });
  const letNames = (steps: Step[]) => steps.flatMap((item) => (item.kind === 'let' ? [item.name] : []));
  const labels = (steps: Step[]) => steps.flatMap((item) => (item.kind === 'require' ? [item.label] : []));

  test('names its variables after the registry account, and the field when it is not `spent`', () => {
    expect(letNames(limit('perCaller'))).toEqual([
      'perCallerLast',
      'perCallerNow',
      'perCallerSpent',
      'perCallerRefill',
      'perCallerTotal',
    ]);
    expect(letNames(limit('limits', { spent: 'usdcSpent', lastSpend: 'usdcLast' }))[0]).toBe('limitsUsdcSpentLast');
    expect(labels(limit('perCaller'))).toEqual(['withinRateLimit']);
    expect(letNames(limit('perCaller', { name: 'daily' }))[0]).toBe('dailyLast');
    expect(labels(limit('perCaller', { name: 'daily' }))).toEqual(['withinDaily']);
  });

  test('compiles a per-caller limit beside a shared one, and two limits in one entry', () => {
    const template = defineTemplate({
      inputs: { amount: { type: 'u64' } },
      registries: { limits: { spent: 'u64', lastSpend: 'i64', usdcSpent: 'u64', usdcLast: 'i64' } },
      accounts: {
        caller: { signer: true, writable: true },
        recipient: { writable: true },
        perCaller: account.registry('limits', { key: expression.accountKey('caller'), payer: 'caller' }),
        shared: account.registry('limits', { payer: 'caller' }),
        systemProgram: account.systemProgram(),
      },
      steps: [
        ...limit('perCaller'),
        ...limit('shared'),
        ...limit('shared', { spent: 'usdcSpent', lastSpend: 'usdcLast' }),
        systemTransfer({
          systemProgram: account.fixed('systemProgram'),
          from: account.fixed('caller'),
          to: account.fixed('recipient'),
          lamports: expression.input('amount'),
        }),
      ],
    });
    const compiled = compileTemplate(template);
    // Each limit's requirement is labeled withinRateLimit; its path tells them apart.
    const paths = compiled.sourceMap.filter((entry) => entry.label === 'withinRateLimit').map((entry) => entry.path);
    expect([...new Set(paths)]).toEqual(['steps[5]', 'steps[13]', 'steps[21]']);
  });
});
