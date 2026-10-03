/**
 * The live-protocol examples are real source files, and this is what keeps them real.
 *
 * Each one compiles here, and its payload — together with its fixed-account, input, row-input,
 * batch-account and account-group order, and the step label of each program counter — is written
 * to `fixtures/protocol-examples.json`. The Rust suite feeds the payload to the on-chain verifier,
 * and the LiteSVM harness uses the recorded order to build each run instruction by name instead of
 * by hand-copied position, and the labels to name the step a failed run stopped at. A
 * template that stops compiling, or that the verifier would reject, fails the build rather than
 * being discovered at upload time.
 *
 * The test-only runtime scenarios in `examples/scenarios` are recorded the same way, in
 * `fixtures/protocol-scenarios.json`. They are not examples, so they stay out of the example count.
 *
 * Regenerate with `pnpm fixtures`.
 */
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { address } from '@solana/kit';
import { describe, expect, test } from 'vitest';

import { compileTemplate, inspectTemplate, type Template } from './index.js';
import { getTemplateAddress } from './kit.js';
import * as protocols from '../examples/protocols/index.js';
import * as scenarios from '../examples/scenarios/index.js';
import { INNER_TEMPLATE_ADDRESS, INNER_TEMPLATE_ID, TEST_CREATOR } from '../examples/scenarios/nested-split-sell.js';

const FIXTURE_PATH = fileURLToPath(new URL('../../../fixtures/protocol-examples.json', import.meta.url));
const SCENARIOS_PATH = fileURLToPath(new URL('../../../fixtures/protocol-scenarios.json', import.meta.url));
const UPDATE = process.env.UPDATE_FIXTURES === '1';
const hex = (bytes: Uint8Array) => [...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join('');

/** Each template's payload, name orders and step labels, by name, as the LiteSVM harness reads them. */
function fixtureOf(entries: [string, Template][]) {
  return Object.fromEntries(
    entries.map(([name, template]) => {
      const compiled = compileTemplate(template);
      return [
        name,
        {
          payload: hex(compiled.bytes),
          fixedAccounts: compiled.fixedAccountOrder,
          inputs: compiled.inputOrder,
          rowInputs: compiled.rowInputOrder,
          batchAccounts: compiled.batchAccountOrder,
          accountGroups: compiled.accountGroupOrder,
          // Program counter to step label. A labelled step spans several instructions, so a
          // failure's pc names its step, never the other way round.
          labels: Object.fromEntries(
            compiled.sourceMap.filter((entry) => entry.label !== undefined).map((entry) => [entry.pc, entry.label]),
          ),
        },
      ];
    }),
  );
}

/** Writes `fixture` to `path` under `pnpm fixtures`, and otherwise requires the file to match it. */
function recordOrCompare(path: string, fixture: ReturnType<typeof fixtureOf>) {
  if (UPDATE) {
    writeFileSync(path, `${JSON.stringify(fixture, null, 2)}\n`);
    return;
  }
  expect(existsSync(path), 'run `pnpm fixtures`').toBe(true);
  expect(JSON.parse(readFileSync(path, 'utf8'))).toEqual(fixture);
}

function compilesForTheVerifier(name: string, template: Template) {
  const compiled = compileTemplate(template);
  // `inspectTemplate` re-reads the header out of the bytes, so agreement means the payload
  // describes itself the way the compiler thinks it does.
  expect(inspectTemplate(compiled.bytes)).toEqual(compiled.stats);
  expect(compiled.bytes.length).toBeGreaterThan(0);
  expect(compiled.stats.instructions).toBeGreaterThan(0);
  // Each template names its steps, so a failing run can be traced back to a line.
  expect(compiled.sourceMap.some((entry) => entry.label !== undefined)).toBe(true);
  expect(name).toMatch(/^[a-z]/);
}

describe('live protocol examples', () => {
  const entries = Object.entries(protocols) as [string, Template][];

  test('every example is exported', () => {
    expect(entries.length).toBe(13);
  });

  test.each(entries)('%s compiles to a template the verifier can parse', compilesForTheVerifier);

  test('payloads and account/input order are recorded for the Rust verifier', () => {
    recordOrCompare(FIXTURE_PATH, fixtureOf(entries));
  });
});

describe('runtime scenarios', () => {
  const entries = Object.entries(scenarios) as [string, Template][];

  test('every scenario is exported', () => {
    expect(entries.length).toBe(6);
  });

  test.each(entries)('%s compiles to a template the verifier can parse', compilesForTheVerifier);

  test('the nested run pins the inner template where the protocol tests upload it', async () => {
    const [inner] = await getTemplateAddress(address(TEST_CREATOR), INNER_TEMPLATE_ID);
    expect(inner).toBe(INNER_TEMPLATE_ADDRESS);
  });

  test('payloads and account/input order are recorded for the LiteSVM harness', () => {
    recordOrCompare(SCENARIOS_PATH, fixtureOf(entries));
  });
});
