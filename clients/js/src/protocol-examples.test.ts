/**
 * The live-protocol examples are real source files, and this is what keeps them real.
 *
 * Each one compiles here, and its payload is written to `fixtures/protocol-examples.json`, which
 * the Rust suite feeds to the on-chain verifier. A template that stops compiling, or that the
 * verifier would reject, fails the build rather than being discovered at upload time.
 *
 * Regenerate with `pnpm fixtures`.
 */
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { describe, expect, test } from 'vitest';

import { compileTemplate, inspectTemplate, type Template } from './index.js';
import * as protocols from '../examples/protocols/index.js';

const FIXTURE_PATH = fileURLToPath(new URL('../../../fixtures/protocol-examples.json', import.meta.url));
const UPDATE = process.env.UPDATE_FIXTURES === '1';
const hex = (bytes: Uint8Array) => [...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join('');

describe('live protocol examples', () => {
  const entries = Object.entries(protocols) as [string, Template][];

  test('every example is exported', () => {
    expect(entries.length).toBe(12);
  });

  test.each(entries)('%s compiles to a template the verifier can parse', (name, template) => {
    const compiled = compileTemplate(template);
    // `inspectTemplate` re-reads the header out of the bytes, so agreement means the payload
    // describes itself the way the compiler thinks it does.
    expect(inspectTemplate(compiled.bytes)).toEqual(compiled.stats);
    expect(compiled.bytes.length).toBeGreaterThan(0);
    expect(compiled.stats.instructions).toBeGreaterThan(0);
    // Each example names its steps, so a failing run can be traced back to a line.
    expect(compiled.sourceMap.some((entry) => entry.label !== undefined)).toBe(true);
    expect(name).toMatch(/^[a-z]/);
  });

  test('payloads are recorded for the Rust verifier', () => {
    const payloads = Object.fromEntries(
      entries.map(([name, template]) => [name, hex(compileTemplate(template).bytes)]),
    );
    if (UPDATE) {
      writeFileSync(FIXTURE_PATH, `${JSON.stringify(payloads, null, 2)}\n`);
      return;
    }
    expect(existsSync(FIXTURE_PATH), 'run `pnpm fixtures`').toBe(true);
    expect(JSON.parse(readFileSync(FIXTURE_PATH, 'utf8'))).toEqual(payloads);
  });
});
