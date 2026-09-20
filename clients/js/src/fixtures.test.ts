/**
 * Compiler fixtures shared with the Rust suites.
 *
 * Each fixture compiles to `fixtures/<name>.hex` at the repository root, with stats, hash, and
 * source map recorded in `fixtures/manifest.json`. The Rust verifier tests and the Mollusk suite
 * load the same files, so any compiler change that alters output shows up here first.
 *
 * Regenerate with `pnpm fixtures` (sets `UPDATE_FIXTURES=1`).
 */
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { afterAll, describe, expect, test } from 'vitest';

import {
  ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES,
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  assertAta,
  compileTemplate,
  defineTemplate,
  ensureAssociatedTokenAccount,
  expression,
  step,
  systemTransfer,
  type Template,
} from './index.js';

const FIXTURE_DIR = fileURLToPath(new URL('../../../fixtures/', import.meta.url));
const MANIFEST_PATH = `${FIXTURE_DIR}manifest.json`;
const UPDATE = process.env.UPDATE_FIXTURES === '1';

const hex = (bytes: Uint8Array) => [...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join('');
const fill = (byte: number) => new Uint8Array(32).fill(byte);

const systemPrograms = {
  systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
} as const;

export const fixtures: Record<string, () => Template> = {
  'system-transfer': () =>
    defineTemplate({
      inputs: { amount: { type: 'u64' } },
      accounts: {
        ...systemPrograms,
        source: { signer: true, writable: true },
        destination: { writable: true },
      },
      steps: [
        systemTransfer({
          systemProgram: account.fixed('systemProgram'),
          from: account.fixed('source'),
          to: account.fixed('destination'),
          lamports: expression.input('amount'),
        }),
      ],
    }),

  'batch-transfer-30': () =>
    defineTemplate({
      inputs: { amount: { type: 'u64' } },
      accounts: { ...systemPrograms, source: { signer: true, writable: true } },
      batch: { maxIterations: 30, row: { recipient: { writable: true } } },
      steps: [
        step.forEach([
          systemTransfer({
            systemProgram: account.fixed('systemProgram'),
            from: account.fixed('source'),
            to: account.iteration('recipient'),
            lamports: expression.input('amount'),
          }),
        ]),
      ],
    }),

  'ensure-ata': () =>
    defineTemplate({
      accounts: {
        associatedTokenProgram: { executable: true, address: ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES },
        tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
        ...systemPrograms,
        mint: { owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 82 },
        payer: { signer: true, writable: true, owner: SYSTEM_PROGRAM_ADDRESS_BYTES },
        owner: {},
        associatedTokenAccount: { writable: true },
      },
      steps: [
        assertAta({
          associatedTokenAccount: account.fixed('associatedTokenAccount'),
          owner: account.fixed('owner'),
          mint: account.fixed('mint'),
          tokenProgram: account.fixed('tokenProgram'),
          associatedTokenProgram: account.fixed('associatedTokenProgram'),
          label: 'ataMatches',
        }),
        ensureAssociatedTokenAccount({
          associatedTokenProgram: account.fixed('associatedTokenProgram'),
          payer: account.fixed('payer'),
          associatedTokenAccount: account.fixed('associatedTokenAccount'),
          owner: account.fixed('owner'),
          mint: account.fixed('mint'),
          systemProgram: account.fixed('systemProgram'),
          tokenProgram: account.fixed('tokenProgram'),
          label: 'createIfMissing',
        }),
      ],
    }),

  'assert-ata': () =>
    defineTemplate({
      accounts: {
        associatedTokenProgram: { executable: true, address: ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES },
        tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
        owner: {},
        mint: {},
        associatedTokenAccount: {},
      },
      steps: [
        assertAta({
          associatedTokenAccount: account.fixed('associatedTokenAccount'),
          owner: account.fixed('owner'),
          mint: account.fixed('mint'),
          tokenProgram: account.fixed('tokenProgram'),
          associatedTokenProgram: account.fixed('associatedTokenProgram'),
        }),
      ],
    }),

  'checked-transfer-snapshot': () =>
    defineTemplate({
      inputs: { amount: { type: 'u64' } },
      accounts: {
        ...systemPrograms,
        source: { signer: true, writable: true },
        destination: { writable: true },
      },
      steps: [
        step.snapshot('before', expression.accountField(account.fixed('source'), 'lamports')),
        systemTransfer({
          systemProgram: account.fixed('systemProgram'),
          from: account.fixed('source'),
          to: account.fixed('destination'),
          lamports: expression.input('amount'),
        }),
        step.snapshot('after', expression.accountField(account.fixed('source'), 'lamports')),
        step.require(
          expression.equal(
            expression.snapshot('after'),
            expression.subtract(expression.snapshot('before'), expression.input('amount')),
          ),
          'balanceDropsByAmount',
        ),
      ],
    }),

  'carry-sum': () =>
    defineTemplate({
      inputs: { budget: { type: 'u64' } },
      accounts: {},
      batch: { maxIterations: 3, minIterations: 1, row: { recipient: {} } },
      steps: [
        step.let('total', expression.u64(0)),
        step.forEach(
          [
            step.assign(
              'total',
              expression.add(
                expression.variable('total'),
                expression.accountField(account.iteration('recipient'), 'lamports'),
              ),
            ),
          ],
          { carry: ['total'] },
        ),
        step.require(expression.lessThanOrEqual(expression.variable('total'), expression.input('budget')), 'withinBudget'),
      ],
    }),

  'dynamic-read': () =>
    defineTemplate({
      inputs: { offset: { type: 'u64' }, expected: { type: 'u64' } },
      accounts: { holder: { owner: TOKEN_PROGRAM_ADDRESS_BYTES } },
      steps: [
        step.require(
          expression.equal(
            expression.accountData(account.fixed('holder'), expression.input('offset'), 'u64'),
            expression.input('expected'),
          ),
          'fieldMatches',
        ),
      ],
    }),

  'return-data': () =>
    defineTemplate({
      accounts: {
        tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
        mint: { owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 82 },
      },
      steps: [
        step.invoke({
          program: account.fixed('tokenProgram'),
          programAddress: TOKEN_PROGRAM_ADDRESS_BYTES,
          accounts: [{ account: account.fixed('mint'), signer: false, writable: false }],
          data: [{ kind: 'literal', bytes: Uint8Array.of(21) }],
          label: 'getAccountDataSize',
        }),
        step.let('size', expression.returnData('u64')),
        step.require(expression.equal(expression.variable('size'), expression.u64(165)), 'sizeIs165'),
      ],
    }),

  'event-flag': () =>
    defineTemplate({
      emitEvent: true,
      inputs: { amount: { type: 'u64' } },
      accounts: {
        ...systemPrograms,
        source: { signer: true, writable: true },
        destination: { writable: true },
      },
      steps: [
        systemTransfer({
          systemProgram: account.fixed('systemProgram'),
          from: account.fixed('source'),
          to: account.fixed('destination'),
          lamports: expression.input('amount'),
        }),
      ],
    }),

  'pinned-mint-read': () =>
    defineTemplate({
      accounts: { mint: { address: fill(4), owner: TOKEN_PROGRAM_ADDRESS_BYTES } },
      steps: [
        step.require(
          expression.equal(expression.accountData(account.fixed('mint'), 44, 'u8'), expression.u64(6)),
          'sixDecimals',
        ),
      ],
    }),
};

const manifest: Record<string, unknown> = {};

describe('shared compiler fixtures', () => {
  for (const [name, define] of Object.entries(fixtures)) {
    test(name, () => {
      const compiled = compileTemplate(define());
      const entry = { hash: hex(compiled.hash), stats: compiled.stats, sourceMap: compiled.sourceMap };
      const hexPath = `${FIXTURE_DIR}${name}.hex`;
      if (UPDATE) {
        mkdirSync(FIXTURE_DIR, { recursive: true });
        writeFileSync(hexPath, `${hex(compiled.bytes)}\n`);
        manifest[name] = entry;
        return;
      }
      expect(existsSync(hexPath), `missing ${hexPath}; run pnpm fixtures`).toBe(true);
      expect(hex(compiled.bytes)).toBe(readFileSync(hexPath, 'utf8').trim());
      const recorded = JSON.parse(readFileSync(MANIFEST_PATH, 'utf8')) as Record<string, typeof entry>;
      expect(recorded[name]).toEqual(entry);
    });
  }

  afterAll(() => {
    if (UPDATE) {
      writeFileSync(MANIFEST_PATH, `${JSON.stringify(manifest, null, 2)}\n`);
    }
  });
});
