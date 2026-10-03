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
  INSTRUCTIONS_SYSVAR_ADDRESS_BYTES,
  RUNTIME_ERROR_NAMES,
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  VERIFIER_ERROR_NAMES,
  account,
  assertAta,
  compileTemplate,
  data,
  defineTemplate,
  inspectTemplate,
  ensureAssociatedTokenAccount,
  expression,
  step,
  rateLimit,
  systemTransfer,
  type Template,
} from './index.js';
import { signedQuoteSettlement } from '../examples/protocols/signed-quote-settlement.js';

const FIXTURE_DIR = fileURLToPath(new URL('../../../fixtures/', import.meta.url));
const MANIFEST_PATH = `${FIXTURE_DIR}manifest.json`;
const UPDATE = process.env.UPDATE_FIXTURES === '1';

const hex = (bytes: Uint8Array) => [...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join('');
const fill = (byte: number) => new Uint8Array(32).fill(byte);

/** `MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr`, the SPL Memo program the Mollusk suite loads. */
const MEMO_PROGRAM_ADDRESS_BYTES = Uint8Array.of(
  5, 74, 83, 90, 153, 41, 33, 6, 77, 36, 232, 113, 96, 218, 56, 124, 124, 53, 181, 221, 188, 146, 187, 129,
  228, 31, 168, 64, 65, 5, 68, 141,
);

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

  'assert-ata-with-bump': () =>
    defineTemplate({
      inputs: { bump: { type: 'u64' } },
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
          bump: expression.input('bump'),
        }),
      ],
    }),

  'waterfall-payout': () =>
    defineTemplate({
      inputs: { reserve: { type: 'u64' } },
      accounts: { ...systemPrograms, treasury: { signer: true, writable: true } },
      batch: {
        maxIterations: 4,
        minIterations: 1,
        row: { creditor: { writable: true } },
        rowInputs: { owed: { type: 'u64' } },
      },
      steps: [
        step.let(
          'remaining',
          expression.subtract(
            expression.accountField(account.fixed('treasury'), 'lamports'),
            expression.input('reserve'),
          ),
        ),
        step.forEach(
          [
            step.let('pay', expression.min(expression.variable('remaining'), expression.rowInput('owed'))),
            systemTransfer({
              systemProgram: account.fixed('systemProgram'),
              from: account.fixed('treasury'),
              to: account.iteration('creditor'),
              lamports: expression.variable('pay'),
              when: expression.greaterThan(expression.variable('pay'), expression.u64(0)),
            }),
            step.assign(
              'remaining',
              expression.subtract(expression.variable('remaining'), expression.variable('pay')),
            ),
          ],
          { carry: ['remaining'] },
        ),
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

  'payroll-row-amounts': () =>
    defineTemplate({
      accounts: { ...systemPrograms, treasury: { signer: true, writable: true } },
      batch: {
        maxIterations: 3,
        minIterations: 1,
        row: { recipient: { writable: true } },
        rowInputs: { amount: { type: 'u64' } },
      },
      steps: [
        step.forEach([
          systemTransfer({
            systemProgram: account.fixed('systemProgram'),
            from: account.fixed('treasury'),
            to: account.iteration('recipient'),
            lamports: expression.rowInput('amount'),
          }),
        ]),
      ],
    }),

  'group-forward-transfer': () =>
    defineTemplate({
      inputs: { amount: { type: 'u64' } },
      accounts: {
        ...systemPrograms,
        source: { signer: true, writable: true },
        destination: { writable: true },
      },
      accountGroups: ['extra'],
      steps: [
        step.invoke({
          program: account.fixed('systemProgram'),
          programAddress: SYSTEM_PROGRAM_ADDRESS_BYTES,
          accounts: [
            { account: account.fixed('source'), signer: true, writable: true },
            { account: account.fixed('destination'), signer: false, writable: true },
          ],
          accountGroup: 'extra',
          data: [data.literal(Uint8Array.of(2, 0, 0, 0)), data.encode('u64', expression.input('amount'))],
        }),
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

  'math-ops': () =>
    defineTemplate({
      inputs: {
        amount: { type: 'u64' },
        price: { type: 'u64' },
        divisor: { type: 'u64' },
        flags: { type: 'u64' },
        exponent: { type: 'u64' },
      },
      accounts: { feed: { owner: TOKEN_PROGRAM_ADDRESS_BYTES } },
      steps: [
        // 1,000,003 × 7 = 7,000,021, and ÷ 3 is 2,333,340⅓.
        step.require(
          expression.equal(
            expression.multiplyDivide(expression.input('amount'), expression.input('price'), expression.input('divisor')),
            expression.u64(2_333_340),
          ),
          'floor',
        ),
        step.require(
          expression.equal(
            expression.multiplyDivide(expression.input('amount'), expression.input('price'), expression.input('divisor'), 'up'),
            expression.u64(2_333_341),
          ),
          'ceiling',
        ),
        // (2^100 + 1)^2 ÷ 2^90 needs the 256-bit path.
        step.require(
          expression.equal(
            expression.multiplyDivide(
              expression.u128((1n << 100n) + 1n),
              expression.u128((1n << 100n) + 1n),
              expression.u128(1n << 90n),
            ),
            expression.u128((1n << 110n) + 2048n),
          ),
          'wideFloor',
        ),
        step.require(
          expression.equal(expression.remainder(expression.input('amount'), expression.input('divisor')), expression.u64(1)),
          'remainder',
        ),
        step.require(
          expression.equal(
            expression.shiftRight(expression.bitAnd(expression.input('flags'), expression.u64(0xff00)), expression.u64(8)),
            expression.u64(0xab),
          ),
          'highByte',
        ),
        step.require(
          expression.equal(expression.shiftLeft(expression.u64(1), expression.u64(63)), expression.u64(1n << 63n)),
          'topBit',
        ),
        step.require(
          expression.equal(
            expression.bitOr(expression.bitXor(expression.input('flags'), expression.u64(0xffff)), expression.u64(0x000f)),
            expression.u64(0x543f),
          ),
          'xorThenOr',
        ),
        step.require(
          expression.equal(expression.powerOfTen(expression.input('exponent')), expression.u128(10n ** 18n)),
          'powerOfTen',
        ),
        // The feed's amount is 0xfffffff8, whose low four bytes are the i32 −8.
        step.require(
          expression.equal(expression.accountData(account.fixed('feed'), 64, 'i32'), expression.i64(-8)),
          'signedRead',
        ),
      ],
    }),

  loops: () =>
    defineTemplate({
      inputs: { rounds: { type: 'u64' }, amount: { type: 'u64' } },
      accounts: {
        ...systemPrograms,
        payer: { signer: true, writable: true },
        recipient: { writable: true },
      },
      batch: { maxIterations: 3, minIterations: 1, row: { holder: {} } },
      steps: [
        // A count loop pays `amount` once per round and adds up the round indexes.
        step.let('indexSum', expression.u64(0)),
        step.repeat(
          expression.input('rounds'),
          [
            systemTransfer({
              systemProgram: account.fixed('systemProgram'),
              from: account.fixed('payer'),
              to: account.fixed('recipient'),
              lamports: expression.input('amount'),
            }),
            step.assign('indexSum', expression.add(expression.variable('indexSum'), expression.loopIndex())),
          ],
          { max: 4, carry: ['indexSum'], label: 'payEachRound' },
        ),
        // 2 × (0 + 1 + … + (rounds − 1)) + rounds = rounds².
        step.require(
          expression.equal(
            expression.add(expression.multiply(expression.variable('indexSum'), expression.u64(2)), expression.input('rounds')),
            expression.multiply(expression.input('rounds'), expression.input('rounds')),
          ),
          'indexesAddUp',
        ),
        // Two row loops over the same rows: the first totals their lamports, and the second
        // checks that no row holds more than half of that total.
        step.let('total', expression.u64(0)),
        step.forEach(
          [
            step.assign(
              'total',
              expression.add(expression.variable('total'), expression.accountField(account.iteration('holder'), 'lamports')),
            ),
          ],
          { carry: ['total'], label: 'totalRows' },
        ),
        step.forEach(
          [
            step.require(
              expression.lessThanOrEqual(
                expression.multiply(expression.accountField(account.iteration('holder'), 'lamports'), expression.u64(2)),
                expression.variable('total'),
              ),
              'noRowAboveHalf',
            ),
          ],
          { label: 'checkShares' },
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

  output: () =>
    defineTemplate({
      inputs: { amount: { type: 'u64' }, memo: { type: 'bytes', maxLength: 16 } },
      accounts: { ...systemPrograms, payer: { signer: true, writable: true } },
      batch: { maxIterations: 2, minIterations: 1, row: { recipient: { writable: true } } },
      steps: [
        step.let('paid', expression.u64(0)),
        step.forEach(
          [
            systemTransfer({
              systemProgram: account.fixed('systemProgram'),
              from: account.fixed('payer'),
              to: account.iteration('recipient'),
              lamports: expression.input('amount'),
            }),
            // The next row sends the transfer's data again without encoding it, so this log
            // must leave the invocation's buffer alone.
            step.emit(
              [
                data.literal(Uint8Array.of(0x50, 0x41, 0x49, 0x44)),
                data.encode('u8', expression.loopIndex()),
                data.encode('pubkey', expression.accountField(account.iteration('recipient'), 'key')),
              ],
              'logRow',
            ),
            step.assign('paid', expression.add(expression.variable('paid'), expression.input('amount'))),
          ],
          { carry: ['paid'] },
        ),
        // Every log starts with a tag of four bytes or more, here ASCII `MEMO`.
        step.emit(
          [data.literal(Uint8Array.of(0x4d, 0x45, 0x4d, 0x4f)), data.encode('bytes', expression.input('memo'))],
          'logMemo',
        ),
        step.setReturnData(
          [
            data.encode('u64', expression.variable('paid')),
            data.encode('pubkey', expression.accountField(account.fixed('payer'), 'key')),
          ],
          'returnTotal',
        ),
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

  // Run by the Mollusk suite as the second of three instructions, between two memos: "before",
  // signed by `memoSigner`, and "after", with no accounts. The inputs point it at the first memo.
  introspection: () => {
    const sysvar = account.fixed('instructions');
    const neighbour = expression.input('neighbour');
    const position = expression.input('position');
    return defineTemplate({
      inputs: { neighbour: { type: 'u64' }, position: { type: 'u64' }, dataOffset: { type: 'u64' } },
      accounts: {
        instructions: { address: INSTRUCTIONS_SYSVAR_ADDRESS_BYTES },
        memoSigner: { signer: true },
        mint: { owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 82 },
      },
      steps: [
        step.require(expression.equal(expression.instructionCount(sysvar), expression.u64(3)), 'threeInstructions'),
        step.require(expression.equal(expression.currentInstructionIndex(sysvar), expression.u64(1)), 'runsSecond'),
        step.require(
          expression.equal(expression.instructionProgram(sysvar, neighbour), expression.pubkey(MEMO_PROGRAM_ADDRESS_BYTES)),
          'neighbourIsMemo',
        ),
        step.require(expression.equal(expression.instructionAccountCount(sysvar, neighbour), expression.u64(1)), 'oneAccount'),
        step.require(
          expression.equal(
            expression.instructionAccount(sysvar, neighbour, position),
            expression.accountField(account.fixed('memoSigner'), 'key'),
          ),
          'memoNamesItsSigner',
        ),
        step.require(expression.instructionAccountIsSigner(sysvar, neighbour, position), 'memoIsSigned'),
        step.require(expression.not(expression.instructionAccountIsWritable(sysvar, neighbour, position)), 'signerIsReadOnly'),
        step.require(expression.equal(expression.instructionDataLength(sysvar, neighbour), expression.u64(6)), 'memoIsSixBytes'),
        // "before" starts 62 65 66 6f: "befo" as a little-endian u32.
        step.require(
          expression.equal(expression.instructionData(sysvar, neighbour, 0, 'u32'), expression.u64(0x6f66_6562)),
          'memoSaysBefore',
        ),
        step.let('after', expression.instructionDataBytes(sysvar, 2, 0, 5)),
        step.require(
          expression.equal(expression.variable('after'), expression.bytes(new TextEncoder().encode('after'))),
          'lastMemoSaysAfter',
        ),
        step.require(expression.equal(expression.bytesLength(expression.variable('after')), expression.u64(5)), 'fiveBytes'),
        step.require(
          expression.equal(
            expression.accountDataBytes(account.fixed('mint'), expression.input('dataOffset'), 1),
            expression.bytes(Uint8Array.of(6)),
          ),
          'mintHasSixDecimals',
        ),
      ],
    });
  },
  /**
   * A transfer of `amount` lamports from the caller, capped per caller at 1,000,000 lamports that
   * refill at 10 a second. The Mollusk suite runs it as the clock moves on and steps back.
   */
  'rate-limited-transfer': () =>
    defineTemplate({
      inputs: { amount: { type: 'u64' } },
      registries: { limits: { spent: 'u64', lastSpend: 'i64' } },
      accounts: {
        caller: { signer: true, writable: true },
        recipient: { writable: true },
        limits: account.registry('limits', { key: expression.accountKey('caller'), payer: 'caller' }),
        systemProgram: account.systemProgram(),
      },
      steps: [
        ...rateLimit({
          registry: 'limits',
          cap: expression.u64(1_000_000),
          refillPerSecond: expression.u64(10),
          amount: expression.input('amount'),
        }),
        systemTransfer({
          systemProgram: account.fixed('systemProgram'),
          from: account.fixed('caller'),
          to: account.fixed('recipient'),
          lamports: expression.input('amount'),
          label: 'pay',
        }),
      ],
    }),

  'signed-quote-settlement': () => signedQuoteSettlement,

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
      // Read back out of the bytes, as a client inspects a template it did not compile.
      expect(inspectTemplate(compiled.bytes)).toEqual(compiled.stats);
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

  for (const [file, names] of [
    ['runtime-error-names.txt', RUNTIME_ERROR_NAMES],
    ['verifier-error-names.txt', VERIFIER_ERROR_NAMES],
  ] as const) {
    test(file, () => {
      const path = `${FIXTURE_DIR}${file}`;
      const content = `${names.join('\n')}\n`;
      if (UPDATE) {
        writeFileSync(path, content);
        return;
      }
      expect(readFileSync(path, 'utf8')).toBe(content);
    });
  }

  afterAll(() => {
    if (UPDATE) {
      writeFileSync(MANIFEST_PATH, `${JSON.stringify(manifest, null, 2)}\n`);
    }
  });
});
