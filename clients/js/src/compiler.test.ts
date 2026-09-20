import { describe, expect, test } from 'vitest';

import {
  ASSOCIATED_TOKEN_PROGRAM_ADDRESS_BYTES,
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  account,
  assertAta,
  buildRunInstruction,
  compileTemplate,
  data,
  decodeTemplateAccount,
  defineTemplate,
  encodeRun,
  ensureAssociatedTokenAccount,
  expression,
  inspectTemplate,
  planTemplateUpload,
  resumeTemplateUpload,
  step,
  systemTransfer,
  type CompiledTemplate,
} from './index.js';

const address = (byte: number) => new Uint8Array(32).fill(byte);

const HEADER_LENGTH = 24;
const ACCOUNT_RECORD_LENGTH = 8;
const INPUT_RECORD_LENGTH = 4;
const INSTRUCTION_LENGTH = 16;

/** Byte offset of instruction `pc` inside a compiled payload. */
function instructionOffset(compiled: CompiledTemplate, pc: number): number {
  const accounts = compiled.stats.fixedAccounts + compiled.stats.batchStride;
  return HEADER_LENGTH + accounts * ACCOUNT_RECORD_LENGTH + compiled.stats.inputs * INPUT_RECORD_LENGTH + pc * INSTRUCTION_LENGTH;
}

function readU64(bytes: Uint8Array, offset: number): bigint {
  return new DataView(bytes.buffer, bytes.byteOffset).getBigUint64(offset, true);
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

describe('Ballista 0.3 compiler', () => {
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
    expect(compiled.bytes[4]).toBe(3);
    expect(inspectTemplate(compiled.bytes)).toEqual(compiled.stats);
    expect(compiled.sourceMap).toEqual([
      { pc: 0, path: 'steps[0]' },
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

    expect(compileTemplate(checkedTransfer).stats).toMatchObject({ instructions: 8, registers: 6 });
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
    expect(readU64(compiled.bytes, record + 6)).toBe(1n << 0n);
    const move = instructionOffset(compiled, forEachPc + 3);
    expect(compiled.bytes[move]).toBe(49);
    expect(compiled.bytes[move + 1]).toBe(0);
    expect(compiled.sourceMap.at(-1)).toEqual({ pc: 7, path: 'steps[2]', label: 'withinBudget' });

    expect(() =>
      defineTemplate({
        accounts: {},
        steps: [step.let('total', expression.u64(0)), step.assign('total', expression.u64(1))],
      }),
    ).toThrow('assign is only valid inside forEach');
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
    const read = instructionOffset(compiled, 1);
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
    const read = instructionOffset(compiled, 1);
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
      { pc: 0, path: 'steps[0]', label: 'loadAmount' },
      { pc: 1, path: 'steps[1]', label: 'payEveryone' },
      { pc: 2, path: 'steps[1].steps[0]', label: 'positive' },
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
