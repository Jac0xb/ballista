import { describe, expect, test } from 'vitest';

import {
  account,
  buildRunInstruction,
  compileTemplate,
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
} from './index.js';

const address = (byte: number) => new Uint8Array(32).fill(byte);
const hex = (bytes: Uint8Array) => [...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join('');

const transfer = defineTemplate({
  inputs: { amount: { type: 'u64' } },
  accounts: {
    systemProgram: { executable: true, address: address(1) },
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
  test('matches the Rust system-transfer fixture', () => {
    const compiled = compileTemplate(transfer);
    expect(hex(compiled.bytes)).toBe(
      '42564d3202030000010102010200020001000400000000000400ff000000000003ffff000000000002ffff000000000002000000010000ffff000000000000000000000029ff00ffff000000000000000000000000000000020200000c0000000103020200ff0000040000000400000000000000010101010101010101010101010101010101010101010101010101010101010102000000',
    );
    expect(hex(compiled.hash)).toBe('89f90fd4ff059d728bfbd09f58c2a02ed458ff59bd262eea495319f69c326d96');
    expect(compiled.stats).toMatchObject({ payloadBytes: 152, instructions: 2, cpis: 1 });
    expect(inspectTemplate(compiled.bytes)).toEqual(compiled.stats);
  });

  test('compiles a constant-size 30-recipient batch template', () => {
    const batch = defineTemplate({
      inputs: { amount: { type: 'u64' } },
      accounts: {
        systemProgram: { executable: true, address: address(1) },
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
        systemProgram: { address: address(1) },
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
        associatedTokenProgram: { executable: true, address: address(1) },
        tokenProgram: { executable: true, address: address(2) },
        systemProgram: { executable: true, address: address(3) },
        mint: { address: address(4), owner: address(2), minDataLength: 82 },
        payer: { signer: true, writable: true, owner: address(3) },
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
      accounts: { program: { executable: true } },
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
        systemProgram: { executable: true, address: address(1) },
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
