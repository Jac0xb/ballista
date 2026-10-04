import { getAddressDecoder, getAddressEncoder, address } from '@solana/kit';
import { describe, expect, test } from 'vitest';

import { decodeBase58, encodeBase58 } from './base58.js';
import {
  BALLISTA_PROGRAM_ADDRESS,
  RUN_EVENT_LENGTH,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  decodeRunEvent,
  parseProgramData,
} from './index.js';

const JUPITER = 'JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4';
const base64 = (bytes: Uint8Array) => Buffer.from(bytes).toString('base64');

/** A run event laid out as the program's `encode_event` writes it. */
function runEvent(iterations: number, expanded: number, executed: bigint, template: Uint8Array): Uint8Array {
  const event = new Uint8Array(RUN_EVENT_LENGTH);
  event.set(new TextEncoder().encode('BEV1'));
  event[4] = 1;
  event[5] = iterations;
  event[6] = expanded;
  new DataView(event.buffer).setBigUint64(7, executed, true);
  event.set(template, 15);
  return event;
}

describe('base58', () => {
  test('matches Kit for addresses with and without leading zero bytes', () => {
    for (const text of [
      BALLISTA_PROGRAM_ADDRESS,
      '11111111111111111111111111111111',
      'Sysvar1nstructions1111111111111111111111111',
      'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA',
      JUPITER,
    ]) {
      const bytes = Uint8Array.from(getAddressEncoder().encode(address(text)));
      expect(decodeBase58(text)).toEqual(bytes);
      expect(encodeBase58(bytes)).toBe(text);
    }
    expect(encodeBase58(Uint8Array.of(0, 0, 1))).toBe('112');
    expect(() => decodeBase58('0OIl')).toThrow(TypeError);
  });
});

describe('the run event', () => {
  test('decodes every field', () => {
    const executed = (1n << 63n) | 0b101n;
    const event = decodeRunEvent(runEvent(3, 64, executed, TOKEN_PROGRAM_ADDRESS_BYTES));
    expect(event).toEqual({
      version: 1,
      iterations: 3,
      expanded: 64,
      executed,
      templateAddress: 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA',
    });
    expect(event?.templateAddress).toBe(getAddressDecoder().decode(TOKEN_PROGRAM_ADDRESS_BYTES));
  });

  test('decodes a field that sits inside a larger buffer', () => {
    const padded = new Uint8Array(RUN_EVENT_LENGTH + 5);
    padded.set(runEvent(1, 2, 3n, new Uint8Array(32).fill(9)), 5);
    expect(decodeRunEvent(padded.subarray(5))).toMatchObject({ iterations: 1, expanded: 2, executed: 3n });
  });

  test('is undefined for anything but 47 bytes starting BEV1', () => {
    const event = runEvent(0, 0, 0n, new Uint8Array(32));
    expect(decodeRunEvent(event.subarray(0, RUN_EVENT_LENGTH - 1))).toBeUndefined();
    expect(decodeRunEvent(Uint8Array.of(...event, 0))).toBeUndefined();
    const otherVersion = event.slice();
    otherVersion[3] = 0x32; // BEV2
    expect(decodeRunEvent(otherVersion)).toBeUndefined();
    // A template's emit: a tag of four bytes or more that does not start with BEV, then its data.
    const emit = new Uint8Array(RUN_EVENT_LENGTH);
    emit.set(new TextEncoder().encode('PAID'));
    expect(decodeRunEvent(emit)).toBeUndefined();
  });
});

describe('Program data lines', () => {
  // The same logs as `program_data_belongs_to_the_invocation_around_it` in tests/protocols: a
  // nested run's line, then Jupiter's, then the outer run's own after both returned.
  const nested = [
    `Program ${BALLISTA_PROGRAM_ADDRESS} invoke [1]`,
    `Program ${BALLISTA_PROGRAM_ADDRESS} invoke [2]`,
    `Program ${JUPITER} invoke [3]`,
    'Program data: AQI=',
    `Program ${JUPITER} success`,
    'Program data: U0xDRQ==',
    'Program log: data: not a data line',
    `Program ${BALLISTA_PROGRAM_ADDRESS} success`,
    'Program data: UEFJRA== AA==',
    `Program ${BALLISTA_PROGRAM_ADDRESS} success`,
  ];

  test('belong to the innermost invocation open around them', () => {
    expect(parseProgramData(nested)).toEqual([
      { program: JUPITER, height: 3, invocation: 2, fields: [Uint8Array.of(1, 2)] },
      { program: BALLISTA_PROGRAM_ADDRESS, height: 2, invocation: 1, fields: [new TextEncoder().encode('SLCE')] },
      { program: BALLISTA_PROGRAM_ADDRESS, height: 1, invocation: 0, fields: [new TextEncoder().encode('PAID'), Uint8Array.of(0)] },
    ]);
  });

  test('number each invocation, so an emit pairs with the run event that names its template', () => {
    const template = new Uint8Array(32).fill(7);
    const logs = [
      `Program ${BALLISTA_PROGRAM_ADDRESS} invoke [1]`,
      `Program data: ${base64(new TextEncoder().encode('PAID'))}`,
      `Program data: ${base64(runEvent(0, 0, 0n, template))}`,
      `Program ${BALLISTA_PROGRAM_ADDRESS} success`,
      `Program ${BALLISTA_PROGRAM_ADDRESS} invoke [1]`,
      `Program data: ${base64(new TextEncoder().encode('PAID'))}`,
      `Program ${BALLISTA_PROGRAM_ADDRESS} success`,
    ];
    const lines = parseProgramData(logs);
    expect(lines.map((line) => line.invocation)).toEqual([0, 0, 1]);
    expect(decodeRunEvent(lines[1]!.fields[0]!)?.templateAddress).toBe(encodeBase58(template));
  });

  test('stop at a truncated log and still attribute what came before', () => {
    const truncated = [
      `Program ${BALLISTA_PROGRAM_ADDRESS} invoke [1]`,
      'Program data: UEFJRA==',
      `Program ${JUPITER} invoke [2]`,
      'Log truncated',
    ];
    expect(parseProgramData(truncated)).toEqual([
      { program: BALLISTA_PROGRAM_ADDRESS, height: 1, invocation: 0, fields: [new TextEncoder().encode('PAID')] },
    ]);
  });

  test('refuse logs that do not nest', () => {
    expect(() => parseProgramData(['Program data: AQI='])).toThrow(/outside every invocation/);
    expect(() => parseProgramData([`Program ${JUPITER} invoke [2]`])).toThrow(/skips a level/);
    expect(() =>
      parseProgramData([`Program ${BALLISTA_PROGRAM_ADDRESS} invoke [1]`, `Program ${JUPITER} success`]),
    ).toThrow(/not the innermost/);
    expect(() =>
      parseProgramData([`Program ${BALLISTA_PROGRAM_ADDRESS} invoke [1]`, 'Program data: !!!']),
    ).toThrow(/not base64/);
  });

  test('ignore every other line, and close a failed invocation', () => {
    // A transaction's second instruction fails after the first succeeded. Its data line was
    // logged before the failure, which is why a failed transaction's events cannot be trusted.
    const failed = [
      `Program ${JUPITER} invoke [1]`,
      'Program data: AQI=',
      `Program ${JUPITER} consumed 31337 of 200000 compute units`,
      `Program return: ${JUPITER} AAAAAAAAAAA=`,
      `Program ${JUPITER} success`,
      `Program ${BALLISTA_PROGRAM_ADDRESS} invoke [1]`,
      'Program data: UEFJRA==',
      'Program log: 0x3, 0x28, 0xff, 0x9, 0xff',
      `Program ${BALLISTA_PROGRAM_ADDRESS} consumed 4100 of 168663 compute units`,
      `Program ${BALLISTA_PROGRAM_ADDRESS} failed: custom program error: 0x3177f`,
    ];
    expect(parseProgramData(failed).map((line) => [line.program, line.invocation])).toEqual([
      [JUPITER, 0],
      [BALLISTA_PROGRAM_ADDRESS, 1],
    ]);
  });
});
