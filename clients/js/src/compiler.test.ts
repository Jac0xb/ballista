import { describe, expect, test } from 'vitest';
import { ZodError } from 'zod';

import {
  ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES,
  ED25519_PROGRAM_ADDRESS_BYTES,
  INSTRUCTIONS_SYSVAR_ADDRESS_BYTES,
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  assertAta,
  buildRunInstruction,
  compileTemplate,
  data,
  decodeTemplateAccount,
  defineTemplate,
  ed25519Signature,
  rateLimit,
  encodeRun,
  ensureAssociatedTokenAccount,
  expression,
  inspectTemplate,
  opcode,
  planTemplateUpload,
  resumeTemplateUpload,
  step,
  systemTransfer,
  type CompiledTemplate,
  type DataPart,
  type Expression,
  type Step,
  type TemplateInput,
} from './index.js';

const address = (byte: number) => new Uint8Array(32).fill(byte);
/** Every emit starts with a literal tag of at least four bytes. */
const TAG = new TextEncoder().encode('TAG1');

const HEADER_LENGTH = 24;
const ACCOUNT_RECORD_LENGTH = 8;
const INPUT_RECORD_LENGTH = 4;
const INSTRUCTION_LENGTH = 16;
const CPI_DESCRIPTOR_LENGTH = 12;
const CPI_ACCOUNT_LENGTH = 2;
const DATA_SEGMENT_LENGTH = 8;
/** Data segment kinds, numbered as `wire.rs` numbers `DATA_LITERAL`, `DATA_REG_U64` and `DATA_REG_PUBKEY`. */
const segmentKind = { literal: 0, u64: 4, pubkey: 7 } as const;

/** Byte offset of instruction `pc` inside a compiled payload. */
function instructionOffset(compiled: CompiledTemplate, pc: number): number {
  const accounts = compiled.stats.fixedAccounts + compiled.stats.batchStride;
  return HEADER_LENGTH + accounts * ACCOUNT_RECORD_LENGTH + compiled.stats.inputs * INPUT_RECORD_LENGTH + pc * INSTRUCTION_LENGTH;
}

function readU64(bytes: Uint8Array, offset: number): bigint {
  return new DataView(bytes.buffer, bytes.byteOffset).getBigUint64(offset, true);
}

/** A record's range immediate, the u64 at byte 6: where the range starts and its length, two little-endian u32s. */
function rangeOf(record: Uint8Array): [start: number, length: number] {
  const immediate = readU64(record, 6);
  return [Number(immediate & 0xffff_ffffn), Number(immediate >> 32n)];
}

/** The issues of the schema error that `define` throws. */
function schemaIssues(define: () => unknown) {
  try {
    define();
  } catch (error) {
    if (error instanceof ZodError) return error.issues;
    throw error;
  }
  throw new Error('the template was accepted');
}

const transfer = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
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
});

describe('Ballista compiler', () => {
  test('compiles the system transfer with stable stats and a matching inspection', () => {
    const compiled = compileTemplate(transfer);
    expect(compiled.stats).toMatchObject({
      payloadBytes: 152,
      instructions: 2,
      cpis: 1,
      registers: 1,
      batchMinIterations: 0,
      emitEvent: false,
    });
    expect(compiled.bytes[4]).toBe(1);
    expect(inspectTemplate(compiled.bytes)).toEqual(compiled.stats);
    expect(compiled.sourceMap).toEqual([
      // The input loads before the first step, so it carries its own path.
      { pc: 0, path: 'inputs.amount' },
      { pc: 1, path: 'steps[0]' },
    ]);
  });

  test('compiles a constant-size 30-recipient batch template', () => {
    const batch = defineTemplate({
      inputs: { amount: { type: 'u64' } },
      accounts: {
        systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
        source: { signer: true, writable: true },
      },
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
    });
    const compiled = compileTemplate(batch);
    expect(compiled.stats).toMatchObject({
      batchStride: 1,
      batchMaxIterations: 30,
      maxExpandedCpis: 30,
      cpis: 1,
    });
    expect(compiled.bytes.length).toBeLessThan(200);

    const instruction = buildRunInstruction({
      compiled,
      programAddress: address(9),
      templateAddress: address(8),
      inputs: { amount: 1_000n },
      accounts: {
        systemProgram: { address: SYSTEM_PROGRAM_ADDRESS_BYTES },
        source: { address: address(2) },
      },
      batchRows: Array.from({ length: 30 }, (_, index) => ({ recipient: { address: address(index + 20) } })),
    });
    expect(instruction.accounts).toHaveLength(33);
    expect(instruction.data).toEqual(Uint8Array.of(5, 232, 3, 0, 0, 0, 0, 0, 0));
  });

  test('guards non-idempotent associated-token creation on account emptiness', () => {
    const ensureUsdcAta = defineTemplate({
      accounts: {
        associatedTokenProgram: { executable: true, address: ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES },
        tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
        systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
        mint: { address: address(4), owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 82 },
        payer: { signer: true, writable: true, owner: SYSTEM_PROGRAM_ADDRESS_BYTES },
        owner: {},
        associatedTokenAccount: { writable: true },
      },
      steps: [
        ensureAssociatedTokenAccount({
          associatedTokenProgram: account.fixed('associatedTokenProgram'),
          payer: account.fixed('payer'),
          associatedTokenAccount: account.fixed('associatedTokenAccount'),
          owner: account.fixed('owner'),
          mint: account.fixed('mint'),
          systemProgram: account.fixed('systemProgram'),
          tokenProgram: account.fixed('tokenProgram'),
        }),
      ],
    });

    const compiled = compileTemplate(ensureUsdcAta);
    expect(compiled.stats).toMatchObject({ instructions: 2, cpis: 1, maxExpandedCpis: 1 });
    expect(compiled.template.steps[0]).toMatchObject({
      kind: 'invoke',
      data: [],
      when: {
        kind: 'accountField',
        account: { kind: 'account', name: 'associatedTokenAccount' },
        field: 'isEmpty',
      },
    });
  });

  test('composes invoke guards with nested AND and OR expressions', () => {
    const when = expression.or(
      expression.and(expression.input('enabled'), expression.input('withinLimit')),
      expression.input('force'),
    );
    const guarded = defineTemplate({
      inputs: {
        enabled: { type: 'bool' },
        withinLimit: { type: 'bool' },
        force: { type: 'bool' },
      },
      accounts: { program: { executable: true, unsafeUnpinned: true } },
      steps: [
        step.invoke({
          program: account.fixed('program'),
          accounts: [],
          data: [],
          when,
        }),
      ],
    });

    expect(() => compileTemplate(guarded)).not.toThrow();
    expect(guarded.steps[0]).toMatchObject({ kind: 'invoke', when });
  });

  test('keeps named snapshots in registers for post-CPI delta assertions', () => {
    const checkedTransfer = defineTemplate({
      inputs: { amount: { type: 'u64' } },
      accounts: {
        systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
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
        ),
      ],
    });

    // The amount input is read twice but loads once, before the first step.
    expect(compileTemplate(checkedTransfer).stats).toMatchObject({ instructions: 7, registers: 5 });
    expect(() =>
      compileTemplate(
        defineTemplate({
          accounts: {},
          steps: [step.require(expression.snapshot('missing'))],
        }),
      ),
    ).toThrow('Unknown variable: missing');
    expect(() =>
      compileTemplate(
        defineTemplate({
          accounts: {},
          steps: [
            step.let('value', expression.bool(true)),
            step.let('value', expression.bool(false)),
            step.require(expression.variable('value')),
          ],
        }),
      ),
    ).toThrow('Variable already defined: value');
  });

  test('compiles canonical PDA and ATA relationship assertions', () => {
    const assertedAta = defineTemplate({
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
    });
    const compiled = compileTemplate(assertedAta);
    expect(compiled.stats).toMatchObject({ instructions: 7, registers: 6, cpis: 0 });
    expect(new DataView(compiled.bytes.buffer, compiled.bytes.byteOffset).getUint16(14, true)).toBe(3);
    expect(compiled.bytes[instructionOffset(compiled, 4)]).toBe(47);

    const oversizedSeed = defineTemplate({
      inputs: { seed: { type: 'bytes', maxLength: 33 } },
      accounts: { program: { executable: true, address: address(9) }, candidate: {} },
      steps: [
        step.require(
          expression.equal(
            expression.accountField(account.fixed('candidate'), 'key'),
            expression.pda(account.fixed('program'), [expression.input('seed')]),
          ),
        ),
      ],
    });
    expect(() => compileTemplate(oversizedSeed)).toThrow('PDA seed can exceed 32 bytes');

    const nonExecutableProgram = defineTemplate({
      accounts: { program: {}, candidate: {} },
      steps: [
        step.require(
          expression.equal(
            expression.accountField(account.fixed('candidate'), 'key'),
            expression.pda(account.fixed('program'), [expression.bytes(Uint8Array.of(1))]),
          ),
        ),
      ],
    });
    expect(() => compileTemplate(nonExecutableProgram)).toThrow('PDA program account must require executable=true');
  });

  test('a supplied bump compiles to CREATE_PDA and must be a u64', () => {
    const withBump = defineTemplate({
      inputs: { bump: { type: 'u64' } },
      accounts: {
        associatedTokenProgram: { executable: true, address: ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES },
        tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
        mint: {},
        owner: {},
        ata: {},
      },
      steps: [
        assertAta({
          associatedTokenAccount: account.fixed('ata'),
          owner: account.fixed('owner'),
          mint: account.fixed('mint'),
          tokenProgram: account.fixed('tokenProgram'),
          associatedTokenProgram: account.fixed('associatedTokenProgram'),
          bump: expression.input('bump'),
        }),
      ],
    });
    const compiled = compileTemplate(withBump);
    // Load the bump, three account keys, CREATE_PDA, the candidate key, the comparison, require.
    expect(compiled.stats).toMatchObject({ instructions: 8, registers: 7, cpis: 0 });
    expect(compiled.bytes[instructionOffset(compiled, 5)]).toBe(50);

    const pubkeyBump = defineTemplate({
      accounts: { program: { executable: true, address: address(9) }, candidate: {} },
      steps: [
        step.require(
          expression.equal(
            expression.accountField(account.fixed('candidate'), 'key'),
            expression.pda(
              account.fixed('program'),
              [expression.bytes(Uint8Array.of(1))],
              expression.accountField(account.fixed('candidate'), 'key'),
            ),
          ),
        ),
      ],
    });
    expect(() => compileTemplate(pubkeyBump)).toThrow('PDA bump');
  });

  test('requires pins for programs and data reads unless the author opts out', () => {
    const unpinnedInvoke = defineTemplate({
      accounts: { program: { executable: true } },
      steps: [step.invoke({ program: account.fixed('program'), accounts: [], data: [] })],
    });
    expect(() => compileTemplate(unpinnedInvoke)).toThrow('Invoke program account program must pin an address');

    const unpinnedPda = defineTemplate({
      accounts: { program: { executable: true }, candidate: {} },
      steps: [
        step.require(
          expression.equal(
            expression.accountField(account.fixed('candidate'), 'key'),
            expression.pda(account.fixed('program'), [expression.bytes(Uint8Array.of(1))]),
          ),
        ),
      ],
    });
    expect(() => compileTemplate(unpinnedPda)).toThrow('PDA program account program must pin an address');

    const unpinnedRead = defineTemplate({
      accounts: { holder: {} },
      steps: [
        step.require(
          expression.equal(expression.accountData(account.fixed('holder'), 64, 'u64'), expression.u64(1)),
        ),
      ],
    });
    expect(() => compileTemplate(unpinnedRead)).toThrow('pins neither owner nor address');

    const optedOut = defineTemplate({
      accounts: { program: { executable: true, unsafeUnpinned: true }, holder: { unsafeUnpinned: true } },
      steps: [
        step.require(
          expression.equal(expression.accountData(account.fixed('holder'), 64, 'u64'), expression.u64(1)),
        ),
        step.invoke({ program: account.fixed('program'), accounts: [], data: [] }),
      ],
    });
    expect(() => compileTemplate(optedOut)).not.toThrow();

    const ownerPinnedRead = defineTemplate({
      accounts: { holder: { owner: TOKEN_PROGRAM_ADDRESS_BYTES } },
      steps: [
        step.require(
          expression.equal(expression.accountData(account.fixed('holder'), 64, 'u64'), expression.u64(1)),
        ),
      ],
    });
    expect(() => compileTemplate(ownerPinnedRead)).not.toThrow();

    const wrongProgram = defineTemplate({
      inputs: { amount: { type: 'u64' } },
      accounts: {
        systemProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
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
    });
    expect(() => compileTemplate(wrongProgram)).toThrow('Invoke targets program');
  });

  test('infers the minimum data length from static reads', () => {
    const compiled = compileTemplate(
      defineTemplate({
        accounts: {
          holder: { owner: TOKEN_PROGRAM_ADDRESS_BYTES },
          declared: { owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 100 },
        },
        steps: [
          step.require(
            expression.equal(
              expression.accountData(account.fixed('holder'), 64, 'u64'),
              expression.accountData(account.fixed('declared'), 32, 'u64'),
            ),
          ),
        ],
      }),
    );
    const view = new DataView(compiled.bytes.buffer, compiled.bytes.byteOffset);
    expect(view.getUint32(HEADER_LENGTH + 4, true)).toBe(72);
    expect(view.getUint32(HEADER_LENGTH + ACCOUNT_RECORD_LENGTH + 4, true)).toBe(100);
  });

  test('compiles loop-carried sums with assign and move', () => {
    const budgeted = defineTemplate({
      inputs: { budget: { type: 'u64' } },
      accounts: {},
      batch: { maxIterations: 3, row: { recipient: {} } },
      steps: [
        step.let('total', expression.u64(0)),
        step.forEach(
          [
            step.assign(
              'total',
              expression.add(expression.variable('total'), expression.accountField(account.iteration('recipient'), 'lamports')),
            ),
          ],
          { carry: ['total'] },
        ),
        step.require(expression.lessThanOrEqual(expression.variable('total'), expression.input('budget')), 'withinBudget'),
      ],
    });
    const compiled = compileTemplate(budgeted);
    expect(compiled.stats).toMatchObject({ instructions: 8, registers: 5 });
    const forEachPc = compiled.sourceMap.find((entry) => entry.path === 'steps[1]')!.pc;
    const record = instructionOffset(compiled, forEachPc);
    expect(compiled.bytes[record]).toBe(42);
    expect(compiled.bytes[record + 2]).toBe(3);
    // Register 0 holds the hoisted budget input, so the carried total is register 1.
    expect(readU64(compiled.bytes, record + 6)).toBe(1n << 1n);
    const move = instructionOffset(compiled, forEachPc + 3);
    expect(compiled.bytes[move]).toBe(49);
    expect(compiled.bytes[move + 1]).toBe(1);
    expect(compiled.sourceMap.at(-1)).toEqual({ pc: 7, path: 'steps[2]', label: 'withinBudget' });

    expect(() =>
      defineTemplate({
        accounts: {},
        steps: [step.let('total', expression.u64(0)), step.assign('total', expression.u64(1))],
      }),
    ).toThrow('assign is only valid inside a loop');
    expect(() =>
      compileTemplate(
        defineTemplate({
          accounts: {},
          batch: { maxIterations: 1, row: { recipient: {} } },
          steps: [step.let('total', expression.u64(0)), step.forEach([step.assign('total', expression.u64(1))])],
        }),
      ),
    ).toThrow("must be listed in the loop's carry");
    expect(() =>
      compileTemplate(
        defineTemplate({
          accounts: {},
          batch: { maxIterations: 1, row: { recipient: {} } },
          steps: [step.forEach([step.require(expression.bool(true))], { carry: ['missing'] })],
        }),
      ),
    ).toThrow('must be defined before the loop');
    expect(() =>
      compileTemplate(
        defineTemplate({
          accounts: {},
          batch: { maxIterations: 1, row: { recipient: {} } },
          steps: [
            step.let('total', expression.u64(0)),
            step.forEach([step.assign('total', expression.bool(true))], { carry: ['total'] }),
          ],
        }),
      ),
    ).toThrow('must keep its u64 type');
  });

  test('encodes minimum iterations and rejects short batches when building a run', () => {
    const compiled = compileTemplate(
      defineTemplate({
        accounts: {},
        batch: { maxIterations: 4, minIterations: 2, row: { recipient: {} } },
        steps: [step.forEach([step.require(expression.bool(true))])],
      }),
    );
    expect(compiled.bytes[20]).toBe(2);
    expect(compiled.stats.batchMinIterations).toBe(2);
    expect(inspectTemplate(compiled.bytes).batchMinIterations).toBe(2);
    expect(() =>
      buildRunInstruction({
        compiled,
        programAddress: address(9),
        templateAddress: address(8),
        accounts: {},
        batchRows: [{ recipient: { address: address(1) } }],
      }),
    ).toThrow('below the template minimum');
    expect(() =>
      defineTemplate({
        accounts: {},
        batch: { maxIterations: 1, minIterations: 2, row: { recipient: {} } },
        steps: [step.forEach([step.require(expression.bool(true))])],
      }),
    ).toThrow('minIterations cannot exceed maxIterations');
  });

  test('compiles dynamic-offset reads with the instruction flag', () => {
    const compiled = compileTemplate(
      defineTemplate({
        inputs: { offset: { type: 'u64' }, expected: { type: 'u64' } },
        accounts: { holder: { owner: TOKEN_PROGRAM_ADDRESS_BYTES } },
        steps: [
          step.require(
            expression.equal(
              expression.accountData(account.fixed('holder'), expression.input('offset'), 'u64'),
              expression.input('expected'),
            ),
          ),
        ],
      }),
    );
    // Both inputs load first, so the read is the third instruction and takes its offset from
    // register 0, where the offset input landed.
    const read = instructionOffset(compiled, 2);
    expect(compiled.bytes[read]).toBe(13);
    expect(compiled.bytes[read + 3]).toBe(0);
    expect(compiled.bytes[read + 5]).toBe(1);
    expect(readU64(compiled.bytes, read + 6)).toBe(0n);
    expect(new DataView(compiled.bytes.buffer, compiled.bytes.byteOffset).getUint32(HEADER_LENGTH + 4, true)).toBe(0);

    expect(() =>
      compileTemplate(
        defineTemplate({
          inputs: { offset: { type: 'bool' } },
          accounts: { holder: { owner: TOKEN_PROGRAM_ADDRESS_BYTES } },
          steps: [
            step.require(
              expression.equal(
                expression.accountData(account.fixed('holder'), expression.input('offset'), 'u64'),
                expression.u64(1),
              ),
            ),
          ],
        }),
      ),
    ).toThrow('accountData offset requires u64');
  });

  test('compiles returnData only as a let directly after an unconditional invoke', () => {
    const sized = defineTemplate({
      accounts: {
        tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
        mint: { owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: 82 },
      },
      steps: [
        step.invoke({
          program: account.fixed('tokenProgram'),
          accounts: [{ account: account.fixed('mint'), signer: false, writable: false }],
          data: [{ kind: 'literal', bytes: Uint8Array.of(21) }],
        }),
        step.let('size', expression.returnData('u64')),
        step.require(expression.equal(expression.variable('size'), expression.u64(165))),
      ],
    });
    const compiled = compileTemplate(sized);
    // The comparison's constant is hoisted ahead of the invoke, so the read still follows it
    // directly, which is what the verifier requires.
    const read = instructionOffset(compiled, 2);
    expect(compiled.bytes[read]).toBe(48);
    expect(compiled.bytes[read + 2]).toBe(13);
    expect(compiled.stats.instructions).toBe(5);

    expect(() =>
      compileTemplate(
        defineTemplate({ accounts: {}, steps: [step.let('size', expression.returnData('u64'))] }),
      ),
    ).toThrow('directly after an unconditional invoke');
    expect(() =>
      compileTemplate(
        defineTemplate({
          inputs: { go: { type: 'bool' } },
          accounts: { program: { executable: true, unsafeUnpinned: true } },
          steps: [
            step.invoke({ program: account.fixed('program'), accounts: [], data: [], when: expression.input('go') }),
            step.let('size', expression.returnData('u64')),
          ],
        }),
      ),
    ).toThrow('directly after an unconditional invoke');
    expect(() =>
      compileTemplate(
        defineTemplate({
          accounts: { program: { executable: true, unsafeUnpinned: true } },
          steps: [
            step.invoke({ program: account.fixed('program'), accounts: [], data: [] }),
            step.require(expression.equal(expression.returnData('u64'), expression.u64(1))),
          ],
        }),
      ),
    ).toThrow('must be the value of a let');
    expect(() =>
      compileTemplate(
        defineTemplate({
          accounts: { program: { executable: true, unsafeUnpinned: true } },
          steps: [
            step.invoke({ program: account.fixed('program'), accounts: [], data: [] }),
            step.let('size', expression.returnData('u64', 1020)),
          ],
        }),
      ),
    ).toThrow('extends past 1024 bytes');
  });

  test('records a source map that reaches into loop bodies', () => {
    const compiled = compileTemplate(
      defineTemplate({
        inputs: { amount: { type: 'u64' } },
        accounts: {
          systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
          source: { signer: true, writable: true },
        },
        batch: { maxIterations: 2, row: { recipient: { writable: true } } },
        steps: [
          step.let('amount', expression.input('amount'), 'loadAmount'),
          step.forEach(
            [
              step.require(expression.greaterThan(expression.variable('amount'), expression.u64(0)), 'positive'),
              systemTransfer({
                systemProgram: account.fixed('systemProgram'),
                from: account.fixed('source'),
                to: account.iteration('recipient'),
                lamports: expression.variable('amount'),
                label: 'pay',
              }),
            ],
            { label: 'payEveryone' },
          ),
        ],
      }),
    );
    expect(compiled.sourceMap).toEqual([
      // Inputs and constants are materialized once, before the loop, so the body is three
      // instructions instead of five.
      { pc: 0, path: 'inputs.amount' },
      { pc: 1, path: 'constants[0]' },
      { pc: 2, path: 'steps[1]', label: 'payEveryone' },
      { pc: 3, path: 'steps[1].steps[0]', label: 'positive' },
      { pc: 4, path: 'steps[1].steps[0]', label: 'positive' },
      { pc: 5, path: 'steps[1].steps[1]', label: 'pay' },
    ]);
  });

  test('compiles row inputs and account groups', () => {
    const template = defineTemplate({
      inputs: { fee: { type: 'u64' } },
      accounts: {
        systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
        treasury: { signer: true, writable: true },
      },
      batch: {
        maxIterations: 4,
        row: { recipient: { writable: true } },
        rowInputs: { amount: { type: 'u64' } },
      },
      accountGroups: ['extra'],
      steps: [
        step.forEach([
          step.invoke({
            program: account.fixed('systemProgram'),
            accounts: [
              { account: account.fixed('treasury'), signer: true, writable: true },
              { account: account.iteration('recipient'), signer: false, writable: true },
            ],
            accountGroup: 'extra',
            data: [data.literal(Uint8Array.of(2, 0, 0, 0)), data.encode('u64', expression.rowInput('amount'))],
          }),
        ]),
      ],
    });
    const compiled = compileTemplate(template);
    expect(compiled.bytes[21]).toBe(1); // row input count
    expect(compiled.bytes[22]).toBe(1); // account group count
    expect(compiled.stats).toMatchObject({ inputs: 1, rowInputs: 1, accountGroups: 1 });
    expect(compiled.rowInputOrder).toEqual(['amount']);
    expect(compiled.accountGroupOrder).toEqual(['extra']);
    // Header, three account records, then two input records: fee then the row's amount.
    expect([...compiled.bytes.slice(48, 56)]).toEqual([2, 0, 0, 0, 2, 0, 0, 0]);
    // Instructions start at 56: FOREACH, then the row load whose operand a carries the iteration bit.
    expect(compiled.bytes[56]).toBe(42);
    expect(compiled.bytes[72]).toBe(1);
    expect(compiled.bytes[74]).toBe(0x80);
    // Three instructions end at 104; the CPI descriptor's second byte names group 0.
    expect(compiled.bytes[105]).toBe(0);
    expect(inspectTemplate(compiled.bytes)).toEqual(compiled.stats);

    const run = buildRunInstruction({
      compiled,
      programAddress: address(1),
      templateAddress: address(2),
      inputs: { fee: 5n },
      accounts: { systemProgram: { address: SYSTEM_PROGRAM_ADDRESS_BYTES }, treasury: { address: address(3) } },
      batchRows: [{ recipient: { address: address(4) } }, { recipient: { address: address(5) } }],
      batchInputs: [{ amount: 10n }, { amount: 20n }],
      accountGroups: { extra: [{ address: address(6) }, { address: address(7), writable: true }] },
    });
    // Template, two fixed, two rows, two group members.
    expect(run.accounts).toHaveLength(7);
    expect(run.accounts[5]).toMatchObject({ signer: false, writable: false });
    expect(run.accounts[6]).toMatchObject({ signer: false, writable: true });
    // Discriminator, group prefix [2], fee, then the two row amounts.
    expect([...run.data]).toEqual([5, 2, 5, 0, 0, 0, 0, 0, 0, 0, 10, 0, 0, 0, 0, 0, 0, 0, 20, 0, 0, 0, 0, 0, 0, 0]);

    expect(() =>
      buildRunInstruction({
        compiled,
        programAddress: address(1),
        templateAddress: address(2),
        inputs: { fee: 5n },
        accounts: { systemProgram: { address: SYSTEM_PROGRAM_ADDRESS_BYTES }, treasury: { address: address(3) } },
        batchRows: [{ recipient: { address: address(4) } }],
        batchInputs: [],
      }),
    ).toThrow(/one batch input record per row/);
    expect(() =>
      defineTemplate({ ...template, steps: [step.require(expression.equal(expression.rowInput('amount'), expression.u64(1)))] }),
    ).toThrow();
    expect(() =>
      compileTemplate(
        defineTemplate({
          inputs: {},
          accounts: {},
          batch: { maxIterations: 1, row: { recipient: {} }, rowInputs: { amount: { type: 'u64' } } },
          steps: [step.require(expression.equal(expression.rowInput('amount'), expression.u64(1))), step.forEach([step.require(expression.bool(true))])],
        }),
      ),
    ).toThrow(/only valid inside forEach/);
    expect(() =>
      defineTemplate({
        inputs: {},
        accounts: { systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES } },
        steps: [step.invoke({ program: account.fixed('systemProgram'), accounts: [], data: [], accountGroup: 'missing' })],
      }),
    ).toThrow(/Unknown account group/);
  });

  test('sets the event flag in the header', () => {
    const compiled = compileTemplate({ ...transfer, emitEvent: true });
    expect(compiled.bytes[17]).toBe(1);
    expect(compiled.stats.emitEvent).toBe(true);
    expect(inspectTemplate(compiled.bytes).emitEvent).toBe(true);
  });

  test('plans one-shot, chunked, and resumable uploads', () => {
    const compiled = compileTemplate(transfer);
    expect(planTemplateUpload(compiled, 4).mode).toBe('oneShot');
    const chunked = planTemplateUpload(compiled, 4, { maxInstructionDataBytes: 64 });
    expect(chunked.mode).toBe('chunked');
    expect(chunked.instructions[0]?.kind).toBe('begin');
    expect(chunked.instructions.at(-1)?.kind).toBe('finalize');

    const accountBytes = encodeAccount(compiled, 0, 59);
    const decoded = decodeTemplateAccount(accountBytes);
    const resumed = resumeTemplateUpload(compiled, decoded, { maxInstructionDataBytes: 64 });
    expect(resumed.instructions[0]).toMatchObject({ kind: 'write', offset: 59 });
  });

  test('validates input and account bindings before run construction', () => {
    const compiled = compileTemplate(transfer);
    expect(() => encodeRun(compiled, {})).toThrow('Missing input');
    expect(() =>
      buildRunInstruction({
        compiled,
        programAddress: address(9),
        templateAddress: address(8),
        inputs: { amount: 1n },
        accounts: {
          systemProgram: { address: address(7) },
          source: { address: address(2) },
          destination: { address: address(3) },
        },
      }),
    ).toThrow('fixed address');
  });

  test('rejects nested iteration and excessive worst-case CPI expansion', () => {
    expect(() =>
      defineTemplate({
        accounts: {},
        batch: { maxIterations: 1, row: { recipient: {} } },
        steps: [step.forEach([step.forEach([step.require(expression.bool(true))])])],
      }),
    ).toThrow('Nested iteration');

    const tooManyCpis = defineTemplate({
      inputs: { amount: { type: 'u64' } },
      accounts: {
        systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
        source: { signer: true, writable: true },
      },
      batch: { maxIterations: 33, row: { recipient: { writable: true } } },
      steps: [
        step.forEach([
          systemTransfer({
            systemProgram: account.fixed('systemProgram'),
            from: account.fixed('source'),
            to: account.iteration('recipient'),
            lamports: expression.input('amount'),
          }),
          systemTransfer({
            systemProgram: account.fixed('systemProgram'),
            from: account.fixed('source'),
            to: account.iteration('recipient'),
            lamports: expression.input('amount'),
          }),
        ]),
      ],
    });
    expect(() => compileTemplate(tooManyCpis)).toThrow('66 CPIs');
  });
});

function encodeAccount(compiled: ReturnType<typeof compileTemplate>, state: 0 | 1, written: number): Uint8Array {
  const output = new Uint8Array(80 + compiled.bytes.length);
  const view = new DataView(output.buffer);
  output[0] = 1;
  output[1] = 2;
  output[2] = state;
  output[3] = 254;
  output.set(address(7), 4);
  view.setUint16(36, 4, true);
  view.setUint32(40, compiled.bytes.length, true);
  view.setUint32(44, written, true);
  output.set(compiled.hash, 48);
  output.set(compiled.bytes.slice(0, written), 80);
  return output;
}

/** Every 16-byte instruction record of a compiled payload. */
function records(compiled: CompiledTemplate): Uint8Array[] {
  return Array.from({ length: compiled.stats.instructions }, (_, pc) => {
    const offset = instructionOffset(compiled, pc);
    return compiled.bytes.slice(offset, offset + INSTRUCTION_LENGTH);
  });
}

/** The blob's length, a little-endian u16 at byte 18 of the header; the blob ends the payload. */
function blobLength(compiled: CompiledTemplate): number {
  return compiled.bytes[18]! | (compiled.bytes[19]! << 8);
}

/** The minimum data length recorded for fixed account `index`: a u32 at byte 4 of its record. */
function minDataLength(compiled: CompiledTemplate, index: number): number {
  const view = new DataView(compiled.bytes.buffer, compiled.bytes.byteOffset);
  return view.getUint32(HEADER_LENGTH + index * ACCOUNT_RECORD_LENGTH + 4, true);
}

/** The data segments (kind and register) and each CPI descriptor's segment range, from the bytes. */
function segmentTables(compiled: CompiledTemplate) {
  const view = new DataView(compiled.bytes.buffer, compiled.bytes.byteOffset);
  const cpiStart = instructionOffset(compiled, compiled.stats.instructions);
  // The header counts CPI accounts in the u16 at byte 12 and data segments in the u16 at byte 14.
  const cpiAccounts = view.getUint16(12, true);
  const segmentStart = cpiStart + compiled.stats.cpis * CPI_DESCRIPTOR_LENGTH + cpiAccounts * CPI_ACCOUNT_LENGTH;
  const segments = Array.from({ length: view.getUint16(14, true) }, (_, index) => ({
    kind: compiled.bytes[segmentStart + index * DATA_SEGMENT_LENGTH]!,
    register: compiled.bytes[segmentStart + index * DATA_SEGMENT_LENGTH + 1]!,
  }));
  // A descriptor's segment count is its byte 5, and its first segment the u16 at byte 6.
  const cpis = Array.from({ length: compiled.stats.cpis }, (_, index) => ({
    start: view.getUint16(cpiStart + index * CPI_DESCRIPTOR_LENGTH + 6, true),
    length: compiled.bytes[cpiStart + index * CPI_DESCRIPTOR_LENGTH + 5]!,
  }));
  return { segments, cpis };
}

describe('data segments', () => {
  test('an invocation part that derives a PDA leaves the invocation its own segments', () => {
    const compiled = compileTemplate(
      defineTemplate({
        accounts: { program: { executable: true, address: address(9) }, owner: {} },
        steps: [
          step.invoke({
            program: account.fixed('program'),
            accounts: [],
            data: [
              data.encode(
                'pubkey',
                expression.pda(account.fixed('program'), [expression.accountField(account.fixed('owner'), 'key')]),
              ),
            ],
          }),
        ],
      }),
    );
    const { segments, cpis } = segmentTables(compiled);
    // Register 0 is the owner's key, the PDA's only seed; register 1 is the derived address.
    expect(segments.slice(cpis[0]!.start, cpis[0]!.start + cpis[0]!.length)).toEqual([
      { kind: segmentKind.pubkey, register: 1 },
    ]);
  });

  test('a PDA seed that derives a PDA leaves the outer derivation its own segments', () => {
    // The associated token account of a vault PDA: the vault is the account's owner seed.
    const compiled = compileTemplate(
      defineTemplate({
        accounts: {
          vaultProgram: { executable: true, address: address(9) },
          tokenProgram: { executable: true, address: TOKEN_PROGRAM_ADDRESS_BYTES },
          associatedTokenProgram: { executable: true, address: ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES },
          authority: {},
          mint: {},
          vaultTokens: {},
        },
        steps: [
          step.require(
            expression.equal(
              expression.accountField(account.fixed('vaultTokens'), 'key'),
              expression.pda(account.fixed('associatedTokenProgram'), [
                expression.pda(account.fixed('vaultProgram'), [expression.accountField(account.fixed('authority'), 'key')]),
                expression.accountField(account.fixed('tokenProgram'), 'key'),
                expression.accountField(account.fixed('mint'), 'key'),
              ]),
            ),
          ),
        ],
      }),
    );
    const [vault, ata] = records(compiled).filter((record) => record[0] === opcode.derivePda);
    const [start, length] = rangeOf(ata!);
    // Register 0 is the token account's key and register 1 the authority's, the vault's only seed.
    // Register 2 is the vault; registers 3 and 4 are the token program's and the mint's keys.
    expect(vault![1]).toBe(2);
    expect(segmentTables(compiled).segments.slice(start, start + length)).toEqual([
      { kind: segmentKind.pubkey, register: 2 },
      { kind: segmentKind.pubkey, register: 3 },
      { kind: segmentKind.pubkey, register: 4 },
    ]);
  });

  test('an output part that derives a PDA leaves the output its own segments', () => {
    for (const output of [step.emit, step.setReturnData]) {
      const compiled = compileTemplate(
        defineTemplate({
          accounts: { program: { executable: true, address: address(9) }, owner: {} },
          steps: [
            output([
              data.literal(TAG),
              data.encode(
                'pubkey',
                expression.pda(account.fixed('program'), [expression.accountField(account.fixed('owner'), 'key')]),
              ),
            ]),
          ],
        }),
      );
      const [record] = records(compiled).filter(
        (candidate) => candidate[0] === opcode.emit || candidate[0] === opcode.setReturnData,
      );
      const [start, length] = rangeOf(record!);
      // Register 0 is the owner's key, the PDA's only seed; register 1 is the derived address.
      expect(segmentTables(compiled).segments.slice(start, start + length)).toEqual([
        { kind: segmentKind.literal, register: 0xff },
        { kind: segmentKind.pubkey, register: 1 },
      ]);
    }
  });
});

describe('output steps', () => {
  const compileSteps = (inputs: TemplateInput['inputs'], steps: Step[], batch?: TemplateInput['batch']) =>
    compileTemplate(
      defineTemplate({
        inputs,
        accounts: {
          systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
          payer: { signer: true, writable: true },
        },
        ...(batch ? { batch } : {}),
        steps,
      }),
    );
  const pay = (to: ReturnType<typeof account.fixed>) =>
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('payer'),
      to,
      lamports: expression.u64(1),
    });
  const rows = { maxIterations: 2, row: { recipient: { writable: true } } };

  test('emit and setReturnData lower to one record naming a contiguous run of their parts', () => {
    const compiled = compileSteps({ amount: { type: 'u64' } }, [
      step.emit([data.literal(TAG), data.encode('u64', expression.input('amount'))], 'log'),
      step.setReturnData([data.encode('pubkey', expression.accountField(account.fixed('payer'), 'key'))], 'result'),
    ]);
    const outputs = records(compiled).filter(
      (record) => record[0] === opcode.emit || record[0] === opcode.setReturnData,
    );
    // Opcode, then dst, a, b, c and flags: an output names no register and takes no flag.
    expect(outputs.map((record) => [...record.slice(0, 6)])).toEqual([
      [opcode.emit, 0xff, 0xff, 0xff, 0xff, 0],
      [opcode.setReturnData, 0xff, 0xff, 0xff, 0xff, 0],
    ]);
    // The immediate is (first segment, segment count).
    expect(outputs.map(rangeOf)).toEqual([
      [0, 2],
      [2, 1],
    ]);
    expect(segmentTables(compiled).segments).toEqual([
      { kind: segmentKind.literal, register: 0xff },
      { kind: segmentKind.u64, register: 0 },
      { kind: segmentKind.pubkey, register: 1 },
    ]);
    expect(compiled.sourceMap.filter((entry) => entry.label !== undefined)).toEqual([
      { pc: 1, path: 'steps[0]', label: 'log' },
      { pc: 2, path: 'steps[1]', label: 'result' },
      { pc: 3, path: 'steps[1]', label: 'result' },
    ]);
  });

  test('emit may appear anywhere, loops and invokes included', () => {
    expect(() =>
      compileSteps(
        {},
        [
          step.emit([data.literal(TAG)]),
          step.forEach([
            step.emit([data.literal(TAG), data.encode('u64', expression.loopIndex())]),
            pay(account.iteration('recipient')),
            step.emit([data.literal(TAG), data.encode('u64', expression.loopIndex())]),
          ]),
          step.emit([data.literal(TAG)]),
        ],
        rows,
      ),
    ).not.toThrow();
  });

  test('setReturnData comes once, outside every loop, after every invoke', () => {
    const result = step.setReturnData([data.literal(Uint8Array.of(1))]);
    expect(() => compileSteps({}, [pay(account.fixed('payer')), result])).not.toThrow();
    expect(() => compileSteps({}, [step.forEach([pay(account.iteration('recipient'))]), result], rows)).not.toThrow();
    expect(() => compileSteps({}, [step.forEach([result])], rows)).toThrow(
      'setReturnData is not allowed inside a loop',
    );
    expect(() => compileSteps({}, [result, result])).toThrow('setReturnData may appear only once');
    expect(() => compileSteps({}, [result, pay(account.fixed('payer'))])).toThrow(
      'invoke cannot follow setReturnData',
    );
    expect(() => compileSteps({}, [result, step.forEach([pay(account.iteration('recipient'))])], rows)).toThrow(
      'invoke cannot follow setReturnData',
    );
  });

  test('setReturnData stays out of count loops, and emit may run in them', () => {
    const count = { n: { type: 'u64' } } as const;
    expect(() =>
      compileSteps(count, [step.repeat(expression.input('n'), [step.setReturnData([data.literal(Uint8Array.of(1))])], { max: 2 })]),
    ).toThrow('setReturnData is not allowed inside a loop');
    expect(() =>
      compileSteps(count, [
        step.repeat(expression.input('n'), [step.emit([data.literal(TAG), data.encode('u64', expression.loopIndex())])], { max: 2 }),
      ]),
    ).not.toThrow();
  });

  test('an output encodes at most 1,024 bytes, counting a bytes value at its maximum length', () => {
    const memo = { memo: { type: 'bytes', maxLength: 1024 } } as const;
    expect(() => compileSteps(memo, [step.setReturnData([data.encode('bytes', expression.input('memo'))])])).not.toThrow();
    expect(() =>
      compileSteps(memo, [step.emit([data.literal(TAG), data.encode('bytes', expression.input('memo'))])]),
    ).toThrow('emit can encode 1028 bytes; maximum is 1024');
    expect(() => compileSteps({}, [step.setReturnData([data.literal(new Uint8Array(1025))])])).toThrow(
      'setReturnData can encode 1025 bytes; maximum is 1024',
    );
  });

  test('an output takes 1 to 64 parts', () => {
    const parts = (count: number) =>
      Array.from({ length: count }, (_, index) => data.literal(index === 0 ? TAG : Uint8Array.of(index)));
    for (const output of [step.emit, step.setReturnData]) {
      expect(schemaIssues(() => compileSteps({}, [output([])]))).toMatchObject([
        { code: 'too_small', minimum: 1, path: ['steps', 0, 'parts'], message: 'Too small: expected array to have >=1 items' },
      ]);
      expect(() => compileSteps({}, [output(parts(64))])).not.toThrow();
      expect(schemaIssues(() => compileSteps({}, [output(parts(65))]))).toMatchObject([
        { code: 'too_big', maximum: 64, path: ['steps', 0, 'parts'], message: 'Too big: expected array to have <=64 items' },
      ]);
    }
  });

  test('emit starts with a literal tag of at least 4 bytes, outside the run event family', () => {
    const amount = { amount: { type: 'u64' } } as const;
    const value = data.encode('u64', expression.input('amount'));
    const tag = (text: string) => data.literal(new TextEncoder().encode(text));
    const emit = (parts: DataPart[]) => () => compileSteps(amount, [step.emit(parts)]);
    const untagged = "emit must start with a literal tag of at least 4 bytes, so its log cannot pass for Ballista's run event";
    const reserved = 'emit tag cannot start with "BEV": that tag family is reserved for Ballista\'s run event';

    // A tag alone or before the data, including tags that only resemble the family.
    for (const text of ['TAG1', 'a longer tag', 'BEU1', 'bev1', 'XBEV']) {
      expect(emit([tag(text), value])).not.toThrow();
      expect(emit([tag(text)])).not.toThrow();
    }
    // No tag, a value before the tag, and a three-byte tag, even with a fourth literal byte after
    // it: the tag is the first part alone.
    expect(emit([value])).toThrow(untagged);
    expect(emit([value, tag('TAG1')])).toThrow(untagged);
    expect(emit([tag('TAG'), value])).toThrow(untagged);
    expect(emit([tag('TAG'), tag('1')])).toThrow(untagged);
    // The run event's magic, and every other version of it.
    for (const text of ['BEV1', 'BEV2', 'BEV\0', 'BEVERAGE']) {
      expect(emit([tag(text), value])).toThrow(reserved);
    }
    // Return data is read by the caller that invoked the run, never mistaken for a log.
    expect(() => compileSteps(amount, [step.setReturnData([value])])).not.toThrow();
  });
});

describe('math expressions', () => {
  const compileSteps = (inputs: TemplateInput['inputs'], steps: Step[]) =>
    compileTemplate(defineTemplate({ inputs, accounts: {}, steps }));

  test('multiplyDivide lowers to one three-operand record, rounding by opcode', () => {
    const compiled = compileSteps(
      { a: { type: 'u64' }, b: { type: 'u64' }, c: { type: 'u64' } },
      [
        step.let('down', expression.multiplyDivide(expression.input('a'), expression.input('b'), expression.input('c'))),
        step.let('up', expression.multiplyDivide(expression.input('a'), expression.input('b'), expression.input('c'), 'up')),
        step.require(expression.lessThanOrEqual(expression.variable('down'), expression.variable('up'))),
      ],
    );
    const mulDivs = records(compiled).filter(
      (record) => record[0] === opcode.mulDiv || record[0] === opcode.mulDivCeil,
    );
    expect(mulDivs.map((record) => record[0])).toEqual([opcode.mulDiv, opcode.mulDivCeil]);
    // Operands a, b, c are the three hoisted input loads, registers 0 to 2.
    expect([...mulDivs[0]!.slice(2, 5)]).toEqual([0, 1, 2]);
  });

  test('multiplyDivide rejects mixed or signed operands', () => {
    expect(() =>
      compileSteps({ a: { type: 'u64' }, b: { type: 'u128' } }, [
        step.let('x', expression.multiplyDivide(expression.input('a'), expression.input('a'), expression.input('b'))),
      ]),
    ).toThrow(/multiplyDivide/);
    expect(() =>
      compileSteps({ a: { type: 'i64' } }, [
        step.let('x', expression.multiplyDivide(expression.input('a'), expression.input('a'), expression.input('a'))),
      ]),
    ).toThrow(/multiplyDivide/);
  });

  test('shifts take an unsigned value and a u64 amount; bitwise ops need matching unsigned operands', () => {
    expect(() =>
      compileSteps({ a: { type: 'u128' }, n: { type: 'u64' } }, [
        step.let('x', expression.shiftLeft(expression.input('a'), expression.input('n'))),
        step.let('y', expression.shiftRight(expression.input('a'), expression.input('n'))),
        step.let('z', expression.bitXor(expression.input('a'), expression.variable('x'))),
      ]),
    ).not.toThrow();
    expect(() =>
      compileSteps({ a: { type: 'i64' }, n: { type: 'u64' } }, [
        step.let('x', expression.shiftLeft(expression.input('a'), expression.input('n'))),
      ]),
    ).toThrow(/shiftLeft/);
    expect(() =>
      compileSteps({ a: { type: 'u64' }, b: { type: 'u128' } }, [
        step.let('x', expression.bitAnd(expression.input('a'), expression.input('b'))),
      ]),
    ).toThrow(/bitAnd/);
  });

  test('remainder works on every numeric type; powerOfTen takes a u64 and yields a u128', () => {
    for (const type of ['u64', 'i64', 'u128'] as const) {
      const compiled = compileSteps({ a: { type } }, [
        step.let('r', expression.remainder(expression.input('a'), expression.input('a'))),
      ]);
      expect(records(compiled).some((record) => record[0] === opcode.remainder)).toBe(true);
    }
    expect(() =>
      compileSteps({ a: { type: 'i64' }, e: { type: 'u64' } }, [
        step.let('r', expression.remainder(expression.input('a'), expression.input('a'))),
        step.require(expression.equal(expression.powerOfTen(expression.input('e')), expression.u128(1_000n))),
      ]),
    ).not.toThrow();
    expect(() =>
      compileSteps({ a: { type: 'u64' }, b: { type: 'i64' } }, [
        step.let('x', expression.remainder(expression.input('a'), expression.input('b'))),
      ]),
    ).toThrow(/remainder/);
    expect(() =>
      compileSteps({ a: { type: 'u128' } }, [step.let('x', expression.powerOfTen(expression.input('a')))]),
    ).toThrow(/powerOfTen/);
  });

  test('an i32 read is typed i64 and raises the data floor to cover its four bytes', () => {
    const compiled = compileTemplate(
      defineTemplate({
        accounts: { feed: { owner: address(7) } },
        steps: [
          step.require(
            expression.lessThan(expression.accountData(account.fixed('feed'), 89, 'i32'), expression.i64(0)),
          ),
        ],
      }),
    );
    expect(records(compiled).some((record) => record[0] === opcode.readI32)).toBe(true);
    expect(minDataLength(compiled, 0)).toBe(93);
  });
});

describe('count loops and several loops', () => {
  const payAccounts = {
    systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
    source: { signer: true, writable: true },
    destination: { writable: true },
  };
  const pay = (to = account.fixed('destination')): Step =>
    systemTransfer({
      systemProgram: account.fixed('systemProgram'),
      from: account.fixed('source'),
      to,
      lamports: expression.input('amount'),
    });

  test('repeat lowers to one REPEAT record: body length, count register, maximum and carry mask', () => {
    const compiled = compileTemplate(
      defineTemplate({
        inputs: { rounds: { type: 'u64' }, amount: { type: 'u64' } },
        accounts: payAccounts,
        steps: [
          step.let('total', expression.u64(0)),
          step.repeat(
            expression.input('rounds'),
            [pay(), step.assign('total', expression.add(expression.variable('total'), expression.loopIndex()))],
            { max: 5, carry: ['total'], label: 'payRounds' },
          ),
        ],
      }),
    );
    const all = records(compiled);
    const repeatPc = all.findIndex((record) => record[0] === opcode.repeat);
    const repeat = all[repeatPc]!;
    // The two hoisted inputs are registers 0 and 1, and the constant total register 2.
    expect([...repeat.slice(1, 6)]).toEqual([0xff, all.length - repeatPc - 1, 0, 5, 0]);
    expect(readU64(repeat, 6)).toBe(1n << 2n);
    expect(compiled.sourceMap[repeatPc]).toEqual({ pc: repeatPc, path: 'steps[1]', label: 'payRounds' });
    expect(compiled.stats.maxExpandedCpis).toBe(5);
    expect(inspectTemplate(compiled.bytes)).toEqual(compiled.stats);
  });

  test('a template may hold several loops; the worst case adds them up and inspectTemplate agrees', () => {
    const compiled = compileTemplate(
      defineTemplate({
        inputs: { amount: { type: 'u64' } },
        accounts: payAccounts,
        batch: { maxIterations: 10, row: { recipient: { writable: true } } },
        steps: [
          pay(),
          step.forEach([pay(account.iteration('recipient'))]),
          step.repeat(expression.u64(3), [pay(), pay()], { max: 4 }),
          step.forEach([step.require(expression.greaterThan(expression.accountField(account.iteration('recipient'), 'lamports'), expression.u64(0)))]),
        ],
      }),
    );
    // One at the root, ten rows of one, and four passes of two.
    expect(compiled.stats.maxExpandedCpis).toBe(1 + 10 + 8);
    expect(inspectTemplate(compiled.bytes)).toEqual(compiled.stats);
    expect(records(compiled).map((record) => record[0]).filter((code) => code === opcode.forEach || code === opcode.repeat)).toEqual([
      opcode.forEach,
      opcode.repeat,
      opcode.forEach,
    ]);

    expect(() =>
      compileTemplate(
        defineTemplate({
          inputs: { amount: { type: 'u64' } },
          accounts: payAccounts,
          steps: [step.repeat(expression.u64(1), [pay(), pay()], { max: 33 })],
        }),
      ),
    ).toThrow('66 CPIs');
  });

  test('a repeat body has the loop index but no rows', () => {
    const rows = {
      inputs: { amount: { type: 'u64' } },
      accounts: payAccounts,
      batch: { maxIterations: 2, row: { recipient: { writable: true } }, rowInputs: { share: { type: 'u64' } } },
    } as const;
    const withRepeat = (body: Step[]) =>
      compileTemplate(
        defineTemplate({ ...rows, steps: [step.forEach([pay()]), step.repeat(expression.u64(2), body, { max: 2 })] }),
      );
    expect(() => withRepeat([step.require(expression.lessThan(expression.loopIndex(), expression.u64(2)))])).not.toThrow();
    expect(() => withRepeat([step.require(expression.equal(expression.rowInput('share'), expression.u64(1)))])).toThrow(
      'Row inputs are only valid inside forEach',
    );
    expect(() => withRepeat([pay(account.iteration('recipient'))])).toThrow('Iteration accounts are only valid inside forEach');
    expect(() =>
      compileTemplate(defineTemplate({ inputs: {}, accounts: {}, steps: [step.require(expression.equal(expression.loopIndex(), expression.u64(0)))] })),
    ).toThrow('loopIndex is only valid inside a loop');
  });

  test('the count is a u64 evaluated before the loop, and the maximum is 1 to 255', () => {
    const counted = (count: Expression, max: number) =>
      compileTemplate(
        defineTemplate({
          inputs: { rounds: { type: 'u64' }, signed: { type: 'i64' } },
          accounts: {},
          steps: [step.repeat(count, [step.require(expression.bool(true))], { max })],
        }),
      );
    // The count's own instructions come before the REPEAT.
    const compiled = counted(expression.divide(expression.input('rounds'), expression.u64(2)), 255);
    const codes = records(compiled).map((record) => record[0]);
    expect(codes.indexOf(opcode.divide)).toBeLessThan(codes.indexOf(opcode.repeat));
    expect(() => counted(expression.input('signed'), 1)).toThrow('repeat count requires u64');
    expect(() => counted(expression.loopIndex(), 1)).toThrow('loopIndex is only valid inside a loop');
    // The maximum is a schema rule on the step's `max`.
    expect(schemaIssues(() => counted(expression.input('rounds'), 0))).toMatchObject([
      { code: 'too_small', minimum: 1, path: ['steps', 0, 'max'], message: 'Too small: expected number to be >=1' },
    ]);
    expect(schemaIssues(() => counted(expression.input('rounds'), 256))).toMatchObject([
      { code: 'too_big', maximum: 255, path: ['steps', 0, 'max'], message: 'Too big: expected number to be <=255' },
    ]);
  });

  test('loops are top level, at most eight, and a batch needs a forEach', () => {
    const loop = () => step.repeat(expression.u64(1), [step.require(expression.bool(true))], { max: 1 });
    expect(() => defineTemplate({ accounts: {}, steps: Array.from({ length: 8 }, loop) })).not.toThrow();
    expect(() => defineTemplate({ accounts: {}, steps: Array.from({ length: 9 }, loop) })).toThrow('at most 8 top-level loops');
    expect(() =>
      defineTemplate({ accounts: {}, batch: { maxIterations: 1, row: { recipient: {} } }, steps: [step.forEach([loop()])] }),
    ).toThrow('Nested iteration');
    expect(() => defineTemplate({ accounts: {}, steps: [step.repeat(expression.u64(1), [loop()], { max: 1 })] })).toThrow('Nested iteration');
    expect(() => defineTemplate({ accounts: {}, batch: { maxIterations: 1, row: { recipient: {} } }, steps: [loop()] })).toThrow(
      'requires at least one top-level forEach',
    );
    expect(() =>
      defineTemplate({
        accounts: {},
        batch: { maxIterations: 1, row: { recipient: {} } },
        steps: [step.forEach([step.require(expression.bool(true))]), step.forEach([step.require(expression.bool(true))])],
      }),
    ).not.toThrow();
  });
});

describe('carried variables', () => {
  test('each carried variable gets its own register when it starts from a value something else reads', () => {
    const compiled = compileTemplate(
      defineTemplate({
        inputs: { start: { type: 'u64' } },
        accounts: {},
        batch: { maxIterations: 3, row: { holder: {} } },
        steps: [
          // Two variables start from one constant, which a later step reads again, and a third
          // starts from an input that a later step reads too.
          step.let('sum', expression.u64(0)),
          step.let('count', expression.u64(0)),
          step.let('floor', expression.input('start')),
          step.forEach(
            [
              step.assign('sum', expression.add(expression.variable('sum'), expression.accountField(account.iteration('holder'), 'lamports'))),
              step.assign('count', expression.add(expression.variable('count'), expression.u64(1))),
              step.assign('floor', expression.add(expression.variable('floor'), expression.u64(1))),
            ],
            { carry: ['sum', 'count', 'floor'] },
          ),
          step.require(expression.greaterThan(expression.variable('count'), expression.u64(0))),
          step.require(expression.greaterThanOrEqual(expression.variable('floor'), expression.input('start'))),
        ],
      }),
    );
    const all = records(compiled);
    // The input is register 0 and the constants 0 and 1 are registers 1 and 2. Each carried
    // variable is copied into a register of its own, 3 to 5, before the loop starts.
    expect(all.slice(3, 6).map((record) => [...record.slice(0, 3)])).toEqual([
      [opcode.move, 3, 1],
      [opcode.move, 4, 1],
      [opcode.move, 5, 0],
    ]);
    expect(all[6]![0]).toBe(opcode.forEach);
    expect(readU64(all[6]!, 6)).toBe((1n << 3n) | (1n << 4n) | (1n << 5n));
  });

  test('a carried variable that alone reads its starting value keeps its register', () => {
    const compiled = compileTemplate(
      defineTemplate({
        inputs: {},
        accounts: {},
        batch: { maxIterations: 3, row: { holder: {} } },
        steps: [
          step.let('total', expression.u64(0)),
          step.forEach(
            [step.assign('total', expression.add(expression.variable('total'), expression.accountField(account.iteration('holder'), 'lamports')))],
            { carry: ['total'] },
          ),
        ],
      }),
    );
    // The constant is register 0 and `total` alone reads it, so no copy comes before the loop.
    const all = records(compiled);
    expect(all[1]![0]).toBe(opcode.forEach);
    expect(readU64(all[1]!, 6)).toBe(1n << 0n);
  });
});

describe('introspection expressions', () => {
  const sysvar = account.fixed('instructions');
  const compileWith = (steps: Step[], accounts: TemplateInput['accounts'] = {}) =>
    compileTemplate(
      defineTemplate({
        inputs: { index: { type: 'u64' }, offset: { type: 'u64' } },
        accounts: { instructions: { address: INSTRUCTIONS_SYSVAR_ADDRESS_BYTES }, ...accounts },
        steps,
      }),
    );
  /** The a, b, c and immediate of every record with `wanted`'s opcode. */
  const operandsOf = (compiled: CompiledTemplate, wanted: number) =>
    records(compiled)
      .filter((record) => record[0] === wanted)
      .map((record) => [record[2], record[3], record[4], readU64(record, 6)]);

  test('each expression lowers to its opcode, with the sysvar in a and u64 registers in b and c', () => {
    const index = expression.input('index');
    const offset = expression.input('offset');
    const compiled = compileWith([
      step.let('count', expression.instructionCount(sysvar)),
      step.let('current', expression.currentInstructionIndex(sysvar)),
      step.let('program', expression.instructionProgram(sysvar, index)),
      step.let('accounts', expression.instructionAccountCount(sysvar, index)),
      step.let('key', expression.instructionAccount(sysvar, index, offset)),
      step.let('flags', expression.instructionAccountFlags(sysvar, index, offset)),
      step.let('length', expression.instructionDataLength(sysvar, index)),
      step.let('word', expression.instructionData(sysvar, index, offset, 'i32')),
      step.let('bytes', expression.instructionDataBytes(sysvar, index, offset, 12)),
      step.require(expression.equal(expression.bytesLength(expression.variable('bytes')), expression.u64(12))),
    ]);
    // Inputs load first: `index` into r0, `offset` into r1. Account 0 is the sysvar.
    const none = 0xff;
    expect(operandsOf(compiled, opcode.instructionCount)).toEqual([[0, none, none, 0n]]);
    expect(operandsOf(compiled, opcode.instructionIndex)).toEqual([[0, none, none, 0n]]);
    expect(operandsOf(compiled, opcode.instructionProgram)).toEqual([[0, 0, none, 0n]]);
    expect(operandsOf(compiled, opcode.instructionAccountCount)).toEqual([[0, 0, none, 0n]]);
    expect(operandsOf(compiled, opcode.instructionAccount)).toEqual([[0, 0, 1, 0n]]);
    expect(operandsOf(compiled, opcode.instructionAccountFlags)).toEqual([[0, 0, 1, 0n]]);
    expect(operandsOf(compiled, opcode.instructionDataLength)).toEqual([[0, 0, none, 0n]]);
    expect(operandsOf(compiled, opcode.readInstructionData)).toEqual([[0, 0, 1, BigInt(opcode.readI32)]]);
    expect(operandsOf(compiled, opcode.readInstructionBytes)).toEqual([[0, 0, 1, 12n]]);
    expect(operandsOf(compiled, opcode.bytesLength)).toHaveLength(1);
  });

  test('a number for an index, position or offset becomes a shared u64 constant', () => {
    const compiled = compileWith([
      step.require(
        expression.equal(
          expression.instructionData(sysvar, 2, 0, 'u8'),
          expression.instructionData(sysvar, 2, 1, 'u8'),
        ),
      ),
    ]);
    expect(records(compiled).filter((record) => record[0] === opcode.constU64)).toHaveLength(3);
  });

  test('results are typed: keys are pubkeys, an i32 read is an i64, byte reads are that long', () => {
    expect(() =>
      compileWith([
        step.require(
          expression.equal(expression.instructionProgram(sysvar, 0), expression.pubkey(ED25519_PROGRAM_ADDRESS_BYTES)),
        ),
        step.require(expression.lessThan(expression.instructionData(sysvar, 0, 0, 'i32'), expression.i64(0))),
      ]),
    ).not.toThrow();
    expect(() =>
      compileWith([step.require(expression.equal(expression.instructionAccount(sysvar, 0, 0), expression.u64(0)))]),
    ).toThrow(/matching types/);
    // A byte read forwarded to a CPI declares exactly its length.
    const compiled = compileWith(
      [
        step.invoke({
          program: account.fixed('program'),
          accounts: [],
          data: [data.encode('bytes', expression.instructionDataBytes(sysvar, 0, 0, 40))],
        }),
      ],
      { program: { executable: true, address: address(9) } },
    );
    expect(compiled.stats.maxCpiDataLength).toBe(40);
  });

  test('the sysvar must be a fixed account pinned to its address', () => {
    for (const accounts of [{ other: {} }, { other: { address: address(3) } }]) {
      expect(() =>
        compileTemplate(
          defineTemplate({
            accounts,
            steps: [step.require(expression.equal(expression.instructionCount(account.fixed('other')), expression.u64(1)))],
          }),
        ),
      ).toThrow(/Instructions sysvar/);
    }
    expect(() =>
      compileTemplate(
        defineTemplate({
          accounts: {},
          batch: { maxIterations: 1, row: { rowSysvar: { address: INSTRUCTIONS_SYSVAR_ADDRESS_BYTES } } },
          steps: [
            step.forEach([
              step.require(
                expression.equal(expression.instructionCount(account.iteration('rowSysvar')), expression.u64(1)),
              ),
            ]),
          ],
        }),
      ),
    ).toThrow(/Instructions sysvar/);
    expect(() =>
      compileWith([
        step.require(expression.equal(expression.instructionDataLength(sysvar, expression.i64(0)), expression.u64(1))),
      ]),
    ).toThrow(/instruction index requires u64/);
  });

  test('accountDataBytes reads a pinned, read-only account', () => {
    const read = (constraint: TemplateInput['accounts'][string]) =>
      compileTemplate(
        defineTemplate({
          accounts: { mint: constraint },
          steps: [
            step.require(
              expression.equal(expression.accountDataBytes(account.fixed('mint'), 44, 1), expression.bytes(Uint8Array.of(6))),
            ),
          ],
        }),
      );
    const compiled = read({ owner: TOKEN_PROGRAM_ADDRESS_BYTES });
    expect(operandsOf(compiled, opcode.readAccountBytes)).toEqual([[0, 0, 0xff, 1n]]);
    expect(() => read({})).toThrow(/pins neither owner nor address/);
    expect(() => read({ owner: TOKEN_PROGRAM_ADDRESS_BYTES, writable: true })).toThrow(/declared writable/);
  });

  test('bytesLength takes bytes; byte reads take 1 to 1024 bytes', () => {
    expect(() => compileWith([step.let('n', expression.bytesLength(expression.input('index')))])).toThrow(
      /bytesLength requires bytes/,
    );
    // Refused by the schema, as a Zod issue on the length itself.
    const lengthIssues = (length: number) => {
      try {
        compileWith([step.let('b', expression.instructionDataBytes(sysvar, 0, 0, length))]);
      } catch (error) {
        return (error as { issues?: { code: string; path: PropertyKey[] }[] }).issues?.map(({ code, path }) => ({
          code,
          path,
        }));
      }
      throw new Error(`a ${length}-byte read compiled`);
    };
    const path = ['steps', 0, 'value', 'length'];
    expect(lengthIssues(0)).toEqual([{ code: 'too_small', path }]);
    expect(lengthIssues(1025)).toEqual([{ code: 'too_big', path }]);
    expect(lengthIssues(1.5)).toEqual([{ code: 'invalid_type', path }]);
  });

  test('isSigner and isWritable test one flag bit', () => {
    const compiled = compileWith([
      step.require(expression.instructionAccountIsSigner(sysvar, 0, 1)),
      step.require(expression.not(expression.instructionAccountIsWritable(sysvar, 0, 1))),
    ]);
    // Each flag is its own read, masked by its own bit.
    expect(operandsOf(compiled, opcode.instructionAccountFlags)).toHaveLength(2);
    expect(records(compiled).filter((record) => record[0] === opcode.bitAnd)).toHaveLength(2);
  });
});

describe('ed25519Signature', () => {
  const sysvar = account.fixed('instructions');
  const quote = ed25519Signature({
    sysvar,
    index: expression.subtract(expression.currentInstructionIndex(sysvar), expression.u64(1)),
    signer: expression.accountField(account.fixed('maker'), 'key'),
    messageLength: 40,
    name: 'quote',
  });
  const compileQuote = (steps: Step[]) =>
    compileTemplate(
      defineTemplate({
        accounts: { instructions: { address: INSTRUCTIONS_SYSVAR_ADDRESS_BYTES }, maker: { signer: true } },
        steps,
      }),
    );

  test('its steps check the program, the header, and the key, and bind the index and the message', () => {
    const compiled = compileQuote([
      ...quote.steps,
      step.require(expression.greaterThan(quote.field(0, 'u64'), expression.u64(0)), 'pricePositive'),
    ]);
    const labels = new Set(compiled.sourceMap.map((entry) => entry.label));
    for (const label of [
      'quoteInstructionIndex',
      'quoteIsEd25519',
      'quoteIsOneSelfContainedSignature',
      'quoteIsBySigner',
      'quoteMessageOffset',
    ]) {
      expect(labels.has(label), label).toBe(true);
    }
    // The index is computed once, and every read of the Ed25519 instruction reuses it.
    expect(records(compiled).filter((record) => record[0] === opcode.instructionIndex)).toHaveLength(1);
    // The header, the key offset and the key, the message offset, and the field.
    const reads = records(compiled).filter((record) => record[0] === opcode.readInstructionData);
    expect(reads.map((record) => readU64(record, 6))).toEqual(
      [opcode.readU128, opcode.readU16, opcode.readPubkey, opcode.readU16, opcode.readU64].map(BigInt),
    );
  });

  test('the header check masks the count, the three instruction indexes and the message size', () => {
    const compiled = compileQuote(quote.steps);
    // The mask and the expected value are the two u128 constants, loaded from the blob.
    const blob = compiled.bytes.slice(compiled.bytes.length - blobLength(compiled));
    const constants = records(compiled)
      .filter((record) => record[0] === opcode.constU128)
      .map((record) => {
        const [offset, length] = rangeOf(record);
        return blob.slice(offset, offset + length).reduceRight((value, byte) => (value << 8n) | BigInt(byte), 0n);
      });
    const field = (offset: number, width: number, value: bigint) =>
      [((1n << BigInt(width * 8)) - 1n) << BigInt(offset * 8), value << BigInt(offset * 8)] as const;
    const parts = [field(0, 1, 1n), field(4, 2, 0xffffn), field(8, 2, 0xffffn), field(12, 2, 40n), field(14, 2, 0xffffn)];
    expect(constants).toEqual([
      parts.reduce((mask, [part]) => mask | part, 0n),
      parts.reduce((expected, [, part]) => expected | part, 0n),
    ]);
  });

  test('fields must lie inside the signed message', () => {
    expect(() => quote.field(32, 'u64')).not.toThrow();
    expect(() => quote.field(33, 'u64')).toThrow(/inside the 40-byte signed message/);
    expect(() => quote.field(9, 'pubkey')).toThrow(/inside/);
    expect(() => quote.field(-1, 'u8')).toThrow(/inside/);
  });

  test('the signer cannot be an input, which the transaction builder chooses', () => {
    for (const signer of [expression.input('maker'), expression.rowInput('maker')]) {
      expect(() => ed25519Signature({ sysvar, index: expression.u64(0), signer, messageLength: 40 })).toThrow(
        /builder cannot choose/,
      );
    }
  });

  test('a field read without the steps does not compile', () => {
    expect(() => compileQuote([step.require(expression.greaterThan(quote.field(0, 'u64'), expression.u64(0)))])).toThrow(
      /Unknown variable: quote/,
    );
  });
});

describe('registries: schema', () => {
  const limits = () =>
    defineTemplate({
      registries: { limits: { spent: 'u64', lastSpend: 'i64' } },
      accounts: {
        caller: { signer: true, writable: true },
        limits: account.registry('limits', { key: expression.accountKey('caller'), payer: 'caller' }),
        systemProgram: account.systemProgram(),
      },
      steps: [step.setRegistry('limits', 'spent', expression.u64(1))],
    });

  test('declares registries, registry accounts and the System program', () => {
    const template = limits();
    expect(template.registries).toEqual({ limits: { spent: 'u64', lastSpend: 'i64' } });
    expect(template.accounts.limits).toMatchObject({
      writable: true,
      signer: false,
      registry: { name: 'limits', payer: 'caller', key: expression.accountKey('caller') },
    });
    expect(template.accounts.systemProgram).toMatchObject({ executable: true, address: new Uint8Array(32) });
    expect(expression.accountKey('caller')).toEqual(expression.accountField(account.fixed('caller'), 'key'));
    expect(expression.registry('limits', 'spent')).toEqual({ kind: 'registry', account: 'limits', field: 'spent' });
    expect(step.setRegistry('limits', 'spent', expression.u64(2), 'charge')).toEqual({
      kind: 'setRegistry',
      account: 'limits',
      field: 'spent',
      value: expression.u64(2),
      label: 'charge',
    });
    expect(account.registry('limits', { payer: 'caller' })).toEqual({
      writable: true,
      registry: { name: 'limits', payer: 'caller' },
    });
  });

  test('a registry holds 1 to 512 bytes of the five writable types, and a template at most 8', () => {
    const withRegistries = (registries: Record<string, Record<string, string>>) => () =>
      defineTemplate({
        registries: registries as never,
        accounts: {},
        steps: [step.require(expression.bool(true))],
      });
    expect(withRegistries({ empty: {} })).toThrow(/1 to 512 bytes/);
    expect(withRegistries({ narrow: { count: 'u8' } })).toThrow(/"count"[\s\S]*Invalid option: expected one of/);
    expect(withRegistries({ bytes: { blob: 'bytes' } })).toThrow(/"blob"[\s\S]*Invalid option: expected one of/);
    const sixteenKeys = Object.fromEntries(Array.from({ length: 16 }, (_, i) => [`k${i}`, 'pubkey']));
    expect(withRegistries({ full: sixteenKeys })).not.toThrow();
    expect(withRegistries({ over: { ...sixteenKeys, flag: 'bool' } })).toThrow(/1 to 512 bytes/);
    const nine = Object.fromEntries(Array.from({ length: 9 }, (_, i) => [`r${i}`, { flag: 'bool' }]));
    expect(withRegistries(nine)).toThrow(/at most 8 registries/);
  });

  test('registry accounts are fixed accounts', () => {
    expect(() =>
      defineTemplate({
        registries: { limits: { spent: 'u64' } },
        accounts: { caller: { signer: true, writable: true } },
        batch: { maxIterations: 2, row: { entry: account.registry('limits', { payer: 'caller' }) } },
        steps: [step.forEach([step.require(expression.bool(true))])],
      }),
    ).toThrow(/registry accounts are fixed accounts/);
  });
});

describe('registries: compiler', () => {
  const SYSTEM = new Uint8Array(32);
  const base = (steps: Step[], extra: Partial<TemplateInput> = {}): TemplateInput => ({
    inputs: { owner: { type: 'pubkey' } },
    registries: { flags: { on: 'bool' }, limits: { spent: 'u64', lastSpend: 'i64', holder: 'pubkey' } },
    accounts: {
      caller: { signer: true, writable: true },
      mine: account.registry('limits', { key: expression.accountKey('caller'), payer: 'caller' }),
      theirs: account.registry('limits', { key: expression.input('owner'), payer: 'caller' }),
      global: account.registry('flags', { payer: 'caller' }),
      systemProgram: account.systemProgram(),
    },
    steps,
    ...extra,
  });

  test('opens every entry before the first step, in declaration order', () => {
    const compiled = compileTemplate(
      base([step.require(expression.equal(expression.registry('global', 'on'), expression.bool(false)))]),
    );
    const opens = records(compiled).filter((record) => record[0] === opcode.openRegistry);
    // mine: accounts 1, key register, payer 0; registry 1 (limits), 48 bytes, System program 4.
    expect(Array.from(opens[0]!.slice(0, 6))).toEqual([opcode.openRegistry, 0xff, 1, opens[0]![3], 0, 0]);
    expect(readU64(opens[0]!, 6)).toBe(1n | (48n << 8n) | (4n << 24n));
    expect(opens[1]![2]).toBe(2);
    expect(readU64(opens[1]!, 6)).toBe(1n | (48n << 8n) | (4n << 24n));
    // global: no key, registry 0, one byte.
    expect(opens[2]![2]).toBe(3);
    expect(opens[2]![3]).toBe(0xff);
    expect(readU64(opens[2]!, 6)).toBe(0n | (1n << 8n) | (4n << 24n));
    const pcs = records(compiled).map((record) => record[0]);
    expect(pcs.lastIndexOf(opcode.openRegistry)).toBeLessThan(pcs.indexOf(opcode.readRegistry));
    expect(compiled.stats.maxExpandedCpis).toBe(9);
    // Read back out of the bytes, each open counts three CPIs as well.
    expect(inspectTemplate(compiled.bytes)).toEqual(compiled.stats);
  });

  test('reads and writes fields at their packed offsets and types', () => {
    const compiled = compileTemplate(
      base([
        step.let('holder', expression.registry('theirs', 'holder')),
        step.setRegistry('mine', 'lastSpend', expression.clockUnixTimestamp()),
        step.setRegistry('mine', 'holder', expression.variable('holder')),
      ]),
    );
    const read = records(compiled).find((record) => record[0] === opcode.readRegistry)!;
    expect(read[2]).toBe(2);
    expect(readU64(read, 6)).toBe(16n | (BigInt(opcode.readPubkey) << 16n));
    const writes = records(compiled).filter((record) => record[0] === opcode.writeRegistry);
    expect(writes.map((record) => [record[1], record[3], readU64(record, 6)])).toEqual([
      [0xff, 1, 8n | (BigInt(opcode.readI64) << 16n)],
      [0xff, 1, 16n | (BigInt(opcode.readPubkey) << 16n)],
    ]);
  });

  test('refuses what the verifier or the run would refuse', () => {
    const fails = (input: TemplateInput, message: RegExp) =>
      expect(() => compileTemplate(input), String(message)).toThrow(message);
    const read = (accountName: string, field: string) => [
      step.require(expression.equal(expression.registry(accountName, field), expression.u64(0))),
    ];
    fails(base(read('caller', 'spent')), /caller is not a registry account/);
    fails(base(read('mine', 'missing')), /limits has no field missing/);
    fails(base([step.setRegistry('mine', 'spent', expression.i64(1))]), /spent is a u64/);
    const { systemProgram: _, ...withoutSystem } = base([]).accounts!;
    fails({ ...base(read('mine', 'spent')), accounts: withoutSystem }, /pinned to the System program/);
    fails(
      { ...base(read('mine', 'spent')), accounts: { ...base([]).accounts!, caller: { signer: true } } },
      /payer caller must be a fixed account declared signer and writable/,
    );
    fails(
      {
        ...base(read('mine', 'spent')),
        accounts: { ...base([]).accounts!, stray: account.registry('nothing', { payer: 'caller' }) },
      },
      /Unknown registry: nothing/,
    );
    fails(
      {
        ...base(read('mine', 'spent')),
        accounts: { ...base([]).accounts!, mine: { ...account.registry('limits', { payer: 'caller' }), signer: true } },
      },
      /mine must be declared only writable/,
    );
    fails(
      {
        ...base(read('mine', 'spent')),
        accounts: {
          ...base([]).accounts!,
          mine: { ...account.registry('limits', { payer: 'caller' }), executable: true },
        },
      },
      /mine must be declared only writable/,
    );
    fails(
      {
        ...base(read('mine', 'spent')),
        accounts: {
          ...base([]).accounts!,
          mine: { ...account.registry('limits', { payer: 'caller' }), address: SYSTEM },
        },
      },
      /mine must be declared only writable/,
    );
    fails(
      {
        ...base(read('mine', 'spent')),
        accounts: {
          ...base([]).accounts!,
          mine: { ...account.registry('limits', { payer: 'caller' }), owner: SYSTEM },
        },
      },
      /mine must be declared only writable/,
    );
    fails(
      {
        ...base(read('mine', 'spent')),
        accounts: {
          ...base([]).accounts!,
          mine: { ...account.registry('limits', { payer: 'caller' }), minDataLength: 1 },
        },
      },
      /mine must be declared only writable/,
    );
    fails(
      {
        ...base(read('mine', 'spent')),
        accounts: { ...base([]).accounts!, mine: { registry: { name: 'limits', payer: 'caller' } } },
      },
      /mine must be declared only writable/,
    );
    fails(
      base([
        step.invoke({
          program: account.fixed('systemProgram'),
          programAddress: SYSTEM,
          accounts: [{ account: account.fixed('mine'), signer: false, writable: true }],
          data: [data.literal(Uint8Array.of(2, 0, 0, 0))],
        }),
      ]),
      /mine is a registry entry: a CPI that passes it writable fails with RegistryReentry/,
    );
  });

  test('a key reads only the fields of entries declared before it', () => {
    const keyedBy = (entry: 'mine' | 'theirs', key: Expression): TemplateInput => {
      const input = base([step.require(expression.registry('global', 'on'))]);
      return {
        ...input,
        accounts: { ...input.accounts, [entry]: account.registry('limits', { key, payer: 'caller' }) },
      };
    };
    // `mine` opens first, so `theirs` can be keyed by the holder `mine` records.
    const chained = compileTemplate(keyedBy('theirs', expression.registry('mine', 'holder')));
    const kinds = records(chained).map((record) => record[0]);
    expect(kinds.indexOf(opcode.readRegistry)).toBeGreaterThan(kinds.indexOf(opcode.openRegistry));
    expect(kinds.indexOf(opcode.readRegistry)).toBeLessThan(kinds.lastIndexOf(opcode.openRegistry));
    // The other way round, the read would come before `theirs` opens, and finalize would refuse it.
    expect(() => compileTemplate(keyedBy('mine', expression.registry('theirs', 'holder')))).toThrow(
      "mine's key reads theirs, whose entry opens after mine's: declare theirs before mine",
    );
    expect(() => compileTemplate(keyedBy('mine', expression.registry('mine', 'holder')))).toThrow(
      "mine's key reads its own entry, which opens only once its key is known",
    );
    // Nested inside the key, too.
    const nested = expression.select(
      expression.registry('global', 'on'),
      expression.accountKey('caller'),
      expression.input('owner'),
    );
    expect(() => compileTemplate(keyedBy('theirs', nested))).toThrow(/^theirs's key reads global,/);
  });

  test("reads an entry's data only through its fields", () => {
    const message = /mine is a registry entry: read its fields with expression\.registry\('mine', field\)/;
    const mine = account.fixed('mine');
    const u64Read = expression.accountData(mine, 72, 'u64');
    expect(() => compileTemplate(base([step.require(expression.equal(u64Read, expression.u64(0)))]))).toThrow(message);
    const byteRead = expression.accountDataBytes(mine, 72, 8);
    expect(() => compileTemplate(base([step.let('raw', byteRead)]))).toThrow(message);
    // Its lamports are not a field.
    const lamports = expression.accountField(mine, 'lamports');
    expect(() => compileTemplate(base([step.require(expression.greaterThan(lamports, expression.u64(0)))]))).not.toThrow();
  });
});

describe('rateLimit', () => {
  // The spec's example, verbatim: 1 SOL a caller, refilling over a day. The cap and the rate are
  // literals; a caller who could pass them would set their own limit.
  const limited = () =>
    defineTemplate({
      inputs: { amount: { type: 'u64' } },
      registries: { limits: { spent: 'u64', lastSpend: 'i64' } },
      accounts: {
        caller: { signer: true, writable: true },
        limits: account.registry('limits', { key: expression.accountKey('caller'), payer: 'caller' }),
        systemProgram: account.systemProgram(),
      },
      steps: [
        ...rateLimit({
          registry: 'limits',
          cap: expression.u64(1_000_000_000),
          refillPerSecond: expression.u64(11_574),
          amount: expression.input('amount'),
        }),
      ],
    });

  test('refills in u128, requires withinRateLimit, and writes both fields back', () => {
    const steps = limited().steps;
    expect(steps.map((item) => (item.kind === 'let' ? `let ${item.name}` : item.kind))).toEqual([
      'let rateLimitLast',
      'let rateLimitNow',
      'let rateLimitSpent',
      'let rateLimitRefill',
      'let rateLimitTotal',
      'require',
      'setRegistry',
      'setRegistry',
    ]);
    // The run's time is the clock, but never earlier than the last spend, and it is what the run
    // writes back: after a clock that steps back, `lastSpend` stays put, so no later run refills
    // the same seconds twice, and the elapsed time is never below zero.
    const last = expression.variable('rateLimitLast');
    const now = expression.variable('rateLimitNow');
    expect(steps[0]).toMatchObject({ value: expression.registry('limits', 'lastSpend') });
    expect(steps[1]).toMatchObject({ value: expression.max(expression.clockUnixTimestamp(), last) });
    expect(steps[3]).toMatchObject({
      value: expression.multiply(
        expression.cast('u128', expression.subtract(now, last)),
        expression.cast('u128', expression.u64(11_574)),
      ),
    });
    expect(steps.at(-1)).toMatchObject({ kind: 'setRegistry', field: 'lastSpend', value: now });
    const compiled = compileTemplate(limited());
    // The caller supplies the amount alone.
    expect(compiled.inputOrder).toEqual(['amount']);
    expect(compiled.sourceMap.some((entry) => entry.label === 'withinRateLimit')).toBe(true);
    const kinds = records(compiled).map((record) => record[0]);
    expect(kinds.filter((kind) => kind === opcode.readRegistry)).toHaveLength(2);
    expect(kinds.filter((kind) => kind === opcode.writeRegistry)).toHaveLength(2);
    // Five u128 casts: the spent amount, the elapsed time, the rate, the new amount and the cap.
    expect(kinds.filter((kind) => kind === opcode.castU128)).toHaveLength(5);
    expect(compiled.stats.maxExpandedCpis).toBe(3);
  });

  test('names its variables and requirement after `name`, and takes other field names', () => {
    const steps = rateLimit({
      registry: 'limits',
      cap: expression.u64(10),
      refillPerSecond: expression.u64(1),
      amount: expression.u64(1),
      spent: 'used',
      lastSpend: 'at',
      name: 'daily',
    });
    expect(steps[0]).toMatchObject({ kind: 'let', name: 'dailyLast', value: expression.registry('limits', 'at') });
    expect(steps[1]).toMatchObject({ kind: 'let', name: 'dailyNow' });
    expect(steps.find((item) => item.kind === 'require')).toMatchObject({ label: 'withinDaily' });
    expect(steps.filter((item) => item.kind === 'setRegistry').map((item) => (item as { field: string }).field)).toEqual([
      'used',
      'at',
    ]);
  });

  test('refuses a cap built from an input, even nested inside an expression', () => {
    expect(() =>
      rateLimit({
        registry: 'limits',
        cap: expression.multiply(expression.u64(2), expression.input('cap')),
        refillPerSecond: expression.u64(11_574),
        amount: expression.input('amount'),
      }),
    ).toThrow('cap must be written inline, from literals and arithmetic');
  });

  test('refuses a refillPerSecond built from a row input', () => {
    expect(() =>
      rateLimit({
        registry: 'limits',
        cap: expression.u64(1_000_000_000),
        refillPerSecond: expression.rowInput('rate'),
        amount: expression.input('amount'),
      }),
    ).toThrow('refillPerSecond must be written inline, from literals and arithmetic');
  });

  test('refuses a cap that grows with the clock or a rate that follows a loop index', () => {
    expect(() =>
      rateLimit({
        registry: 'limits',
        cap: expression.add(expression.u64(1_000), expression.clockSlot()),
        refillPerSecond: expression.u64(11_574),
        amount: expression.input('amount'),
      }),
    ).toThrow('cap must be written inline, from literals and arithmetic');
    expect(() =>
      rateLimit({
        registry: 'limits',
        cap: expression.u64(1_000_000_000),
        refillPerSecond: expression.loopIndex(),
        amount: expression.input('amount'),
      }),
    ).toThrow('refillPerSecond must be written inline, from literals and arithmetic');
  });

  test('refuses a cap laundered through a variable, since its origin cannot be traced', () => {
    // `step.let('cap', expression.input('cap'))` would bind the same input to `cap` outside
    // rateLimit's view; the helper must refuse the `variable` read regardless of what it holds.
    expect(() =>
      rateLimit({
        registry: 'limits',
        cap: expression.variable('cap'),
        refillPerSecond: expression.u64(11_574),
        amount: expression.input('amount'),
      }),
    ).toThrow('cap must be written inline, from literals and arithmetic');
  });

  test('refuses a cap read from the Instructions sysvar', () => {
    expect(() =>
      rateLimit({
        registry: 'limits',
        cap: expression.instructionData(account.fixed('instructions'), expression.u64(0), expression.u64(0), 'u64'),
        refillPerSecond: expression.u64(11_574),
        amount: expression.input('amount'),
      }),
    ).toThrow('cap must be written inline, from literals and arithmetic');
  });

  test('refuses a cap read from an account field, such as the caller\'s own lamports', () => {
    expect(() =>
      rateLimit({
        registry: 'limits',
        cap: expression.accountField(account.fixed('caller'), 'lamports'),
        refillPerSecond: expression.u64(11_574),
        amount: expression.input('amount'),
      }),
    ).toThrow('cap must be written inline, from literals and arithmetic');
  });

  test('accepts a cap built from arithmetic over literals', () => {
    expect(() =>
      rateLimit({
        registry: 'limits',
        cap: expression.multiply(expression.u64(1_000), expression.u64(1_000_000)),
        refillPerSecond: expression.u64(11_574),
        amount: expression.input('amount'),
      }),
    ).not.toThrow();
  });

  test('accepts a cap read from a registry field', () => {
    expect(() =>
      rateLimit({
        registry: 'limits',
        cap: expression.registry('limits', 'spent'),
        refillPerSecond: expression.u64(11_574),
        amount: expression.input('amount'),
      }),
    ).not.toThrow();
  });
});
