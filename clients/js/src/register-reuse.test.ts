/**
 * Register reuse. A template whose values fit in 64 registers takes one per value, in order, so its
 * bytes never change. One that needs more is renumbered: a value takes the lowest register whose
 * value has been read for the last time. These tests read the compiled bytes directly, so they do
 * not share the compiler's own idea of which operands are registers.
 */
import { describe, expect, test } from 'vitest';

import {
  account,
  compileTemplate,
  data,
  defineTemplate,
  expression,
  inspectTemplate,
  opcode,
  step,
  type CompiledTemplate,
  type Step,
  type TemplateInput,
} from './index.js';

const NO_INDEX = 0xff;

interface Instruction {
  pc: number;
  opcode: number;
  dst: number;
  a: number;
  b: number;
  c: number;
  immediate: bigint;
}

/** The instructions, CPI descriptors and data segments of a compiled template. */
function decode(compiled: CompiledTemplate) {
  const bytes = compiled.bytes;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const [fixedAccounts, batchStride, inputs, count, cpiCount, rowInputs] = [5, 6, 8, 10, 11, 21].map((at) => bytes[at]!);
  let offset = 24 + (fixedAccounts! + batchStride!) * 8 + (inputs! + rowInputs!) * 4;
  const instructions: Instruction[] = Array.from({ length: count! }, (_, pc) => {
    const at = offset + pc * 16;
    const [code, dst, a, b, c] = bytes.slice(at, at + 5);
    return { pc, opcode: code!, dst: dst!, a: a!, b: b!, c: c!, immediate: view.getBigUint64(at + 6, true) };
  });
  offset += count! * 16;
  const cpis = Array.from({ length: cpiCount! }, (_, index) => {
    const at = offset + index * 12;
    return { segmentCount: bytes[at + 5]!, segmentStart: view.getUint16(at + 6, true) };
  });
  offset += cpiCount! * 12 + view.getUint16(12, true) * 2;
  const segments = Array.from({ length: view.getUint16(14, true) }, (_, index) => ({
    kind: bytes[offset + index * 8]!,
    register: bytes[offset + index * 8 + 1]!,
  }));
  return { instructions, cpis, segments };
}

/** The instructions in `[from, to)` that write `register`. */
function writesTo(instructions: Instruction[], register: number, from: number, to: number): Instruction[] {
  return instructions.filter(({ pc, dst }) => pc >= from && pc < to && dst === register);
}

const lamports = (name: string) => expression.accountField(account.fixed(name), 'lamports');

/** `count` checks that read two balances and compare them: three values each, each read once. */
function balanceChecks(count: number, prefix = 'check'): Step[] {
  return Array.from({ length: count }, (_, index) =>
    step.require(expression.lessThanOrEqual(lamports('left'), lamports('right')), `${prefix}${index}`),
  );
}

const accounts = { left: {}, right: {} } as const;

function compile(template: TemplateInput): CompiledTemplate {
  const compiled = compileTemplate(defineTemplate(template));
  // The header, which `inspectTemplate` reads back, declares the count after reuse.
  expect(inspectTemplate(compiled.bytes)).toEqual(compiled.stats);
  return compiled;
}

describe('register reuse', () => {
  test('a template whose values fit in 64 registers takes one per value, in order', () => {
    // 21 checks of three values, and one more: exactly 64.
    const compiled = compile({ accounts, steps: [...balanceChecks(21), step.let('slot', expression.clockSlot())] });
    expect(compiled.stats.registers).toBe(64);
    const written = decode(compiled).instructions.filter(({ dst }) => dst !== NO_INDEX).map(({ dst }) => dst);
    expect(written).toEqual(Array.from({ length: 64 }, (_, register) => register));
  });

  test('past 64, a value takes the lowest register whose value was read for the last time', () => {
    // 22 checks of three values: 66, each read once, by the next instruction.
    const compiled = compile({ accounts, steps: balanceChecks(22) });
    expect(compiled.stats.registers).toBe(3);
    // Each check reads the two balances into registers 0 and 1 and compares them into 2. The
    // comparison reads 0 and 1 for the last time, but an instruction never writes the register of
    // a value it reads, so the result takes 2.
    const shape = decode(compiled).instructions.slice(0, 8).map(({ opcode: code, dst, a, b }) => [code, dst, a, b]);
    expect(shape).toEqual([
      [opcode.accountLamports, 0, 0, NO_INDEX],
      [opcode.accountLamports, 1, 1, NO_INDEX],
      [opcode.lessThanOrEqual, 2, 0, 1],
      [opcode.require, NO_INDEX, 2, NO_INDEX],
      [opcode.accountLamports, 0, 0, NO_INDEX],
      [opcode.accountLamports, 1, 1, NO_INDEX],
      [opcode.lessThanOrEqual, 2, 0, 1],
      [opcode.require, NO_INDEX, 2, NO_INDEX],
    ]);
  });

  test('inputs and constants load before the first step and keep their register until their last read', () => {
    const compiled = compile({
      inputs: { floor: { type: 'u64' } },
      accounts,
      steps: [
        step.require(expression.lessThanOrEqual(expression.input('floor'), lamports('left')), 'aboveTheFloor'),
        ...balanceChecks(20),
        step.require(expression.equal(lamports('right'), expression.u64(7)), 'seven'),
      ],
    });
    const { instructions } = decode(compiled);
    const [floor, seven] = instructions;
    expect([floor!.opcode, seven!.opcode]).toEqual([opcode.loadInput, opcode.constU64]);
    const lastCheck = instructions.findLast(({ opcode: code }) => code === opcode.equal)!;
    // The constant keeps its register from its load to the last step, which reads it.
    expect(lastCheck.b).toBe(seven!.dst);
    expect(writesTo(instructions, seven!.dst, seven!.pc + 1, lastCheck.pc)).toEqual([]);
    // The input is read once, in the first step; after that its register holds other values.
    const firstCheck = instructions.find(({ opcode: code }) => code === opcode.lessThanOrEqual)!;
    expect(firstCheck.a).toBe(floor!.dst);
    expect(writesTo(instructions, floor!.dst, floor!.pc + 1, firstCheck.pc)).toEqual([]);
    expect(writesTo(instructions, floor!.dst, firstCheck.pc + 1, instructions.length).length).toBeGreaterThan(0);
    expect(compiled.stats.registers).toBe(4);
  });

  describe('loops', () => {
    // `stride` is read only inside the loop. `total`, `rounds` and `kept` are carried: the body
    // assigns the first two and never touches `kept`, and only `total` is read after the loop.
    const loop = () => {
      const compiled = compile({
        inputs: { passes: { type: 'u64' }, stride: { type: 'u64' } },
        accounts,
        steps: [
          ...balanceChecks(16, 'before'),
          step.let('total', expression.u64(0)),
          step.let('rounds', expression.u64(0)),
          step.let('kept', expression.u64(0)),
          step.repeat(
            expression.input('passes'),
            [
              step.let('term', expression.multiply(expression.loopIndex(), expression.input('stride'))),
              step.assign('total', expression.add(expression.variable('total'), expression.variable('term'))),
              step.assign('rounds', expression.add(expression.variable('rounds'), expression.u64(1))),
              ...balanceChecks(2, 'during'),
            ],
            { max: 8, carry: ['total', 'rounds', 'kept'] },
          ),
          ...balanceChecks(2, 'after'),
          step.setReturnData([data.encode('u64', expression.variable('total'))]),
        ],
      });
      const program = decode(compiled);
      const repeat = program.instructions.find(({ opcode: code }) => code === opcode.repeat)!;
      const end = repeat.pc + 1 + repeat.a;
      const body = program.instructions.slice(repeat.pc + 1, end);
      // The carried registers, from the mask.
      const carried = [...Array(64).keys()].filter((register) => ((repeat.immediate >> BigInt(register)) & 1n) === 1n);
      return { compiled, program, repeat, end, body, carried };
    };

    test('a value from before the loop that the loop reads keeps its register to the end of the loop', () => {
      const { compiled, program, end, body } = loop();
      expect(compiled.stats.registers).toBeLessThanOrEqual(64);
      const stride = program.instructions.find(({ opcode: code, a }) => code === opcode.loadInput && a === 1)!;
      // Its one read is the body's first multiply, early in the body. No later instruction of the
      // body writes over it: the next pass reads it again.
      const read = body.find(({ opcode: code }) => code === opcode.multiply)!;
      expect(read.b).toBe(stride.dst);
      expect(end - read.pc).toBeGreaterThan(5);
      expect(writesTo(program.instructions, stride.dst, stride.pc + 1, end)).toEqual([]);
    });

    test('the body reuses registers that values from before the loop no longer need', () => {
      const { program, repeat, body } = loop();
      const before = new Set(program.instructions.slice(0, repeat.pc).map(({ dst }) => dst));
      expect(body.some(({ dst }) => dst !== NO_INDEX && before.has(dst))).toBe(true);
    });

    test('a carried value keeps its register through the loop, and the mask names it after renumbering', () => {
      const { program, end, body, carried } = loop();
      expect(carried).toHaveLength(3);
      // In the body, only an assignment, a move, writes a carried register: one each for `total`
      // and `rounds`, none for `kept`. Neither `rounds` nor `kept` is read after the loop, yet
      // nothing else takes their registers before the body ends.
      const writes = carried.map((register) => body.filter(({ dst }) => dst === register).map(({ opcode: code }) => code));
      expect(writes.toSorted((x, y) => x.length - y.length)).toEqual([[], [opcode.move], [opcode.move]]);
      // `total` is read after the loop, by the return data, and nothing writes over it before.
      const output = program.instructions.find(({ opcode: code }) => code === opcode.setReturnData)!;
      const total = program.segments[Number(output.immediate & 0xffff_ffffn)]!.register;
      expect(carried).toContain(total);
      expect(writesTo(program.instructions, total, end, output.pc)).toEqual([]);
    });
  });

  test("an invoke's data keeps its register past the values its guard computes", () => {
    const compiled = compile({
      accounts: { program: { executable: true, address: new Uint8Array(32).fill(9) }, ...accounts },
      steps: [
        ...balanceChecks(21),
        step.invoke({
          program: account.fixed('program'),
          accounts: [],
          data: [data.encode('u64', expression.subtract(lamports('left'), expression.u64(5)))],
          when: expression.lessThan(lamports('right'), lamports('left')),
          label: 'pay',
        }),
      ],
    });
    expect(compiled.stats.registers).toBeLessThanOrEqual(64);
    const { instructions, cpis, segments } = decode(compiled);
    const amount = instructions.find(({ opcode: code }) => code === opcode.subtract)!;
    const invoke = instructions.find(({ opcode: code }) => code === opcode.invoke)!;
    // The guard's three values are computed after the amount and before the invoke reads both.
    expect(invoke.pc - amount.pc).toBe(4);
    expect(segments[cpis[invoke.a]!.segmentStart]!.register).toBe(amount.dst);
    expect(writesTo(instructions, amount.dst, amount.pc + 1, invoke.pc)).toEqual([]);
  });

  test("an output's parts keep their registers past a derivation among them", () => {
    const compiled = compile({
      accounts: { program: { executable: true, address: new Uint8Array(32).fill(9) }, ...accounts },
      steps: [
        ...balanceChecks(21),
        step.setReturnData([
          data.encode('u64', expression.subtract(lamports('left'), expression.u64(5))),
          data.encode('pubkey', expression.pda(account.fixed('program'), [expression.accountKey('right'), expression.accountKey('left')])),
        ]),
      ],
    });
    const { instructions, segments } = decode(compiled);
    const amount = instructions.find(({ opcode: code }) => code === opcode.subtract)!;
    const derive = instructions.find(({ opcode: code }) => code === opcode.derivePda)!;
    const output = instructions.find(({ opcode: code }) => code === opcode.setReturnData)!;
    const parts = segments.slice(Number(output.immediate & 0xffff_ffffn));
    expect(parts.map(({ register }) => register)).toEqual([amount.dst, derive.dst]);
    expect(writesTo(instructions, amount.dst, amount.pc + 1, output.pc)).toEqual([]);
    // Both seeds are read when the derivation runs: the first survives the second's read.
    const seeds = segments.slice(Number(derive.immediate & 0xffff_ffffn), Number(derive.immediate & 0xffff_ffffn) + 2);
    const keys = instructions.filter(({ opcode: code }) => code === opcode.accountKey);
    expect(seeds.map(({ register }) => register)).toEqual(keys.map(({ dst }) => dst));
    expect(new Set(seeds.map(({ register }) => register)).size).toBe(2);
  });

  test('more than 64 values in use at once fail, naming the step where they are', () => {
    // Sixty-four constants that one invoke reads, and a sixty-fifth that a later step reads.
    const template = () =>
      compileTemplate(
        defineTemplate({
          accounts: { program: { executable: true, address: new Uint8Array(32).fill(9) }, ...accounts },
          steps: [
            step.invoke({
              program: account.fixed('program'),
              accounts: [],
              data: Array.from({ length: 64 }, (_, index) => data.encode('u64', expression.u64(index))),
              label: 'payAll',
            }),
            step.require(expression.equal(lamports('left'), expression.u64(100)), 'hundred'),
          ],
        }),
      );
    expect(template).toThrow('Template uses more than 64 registers: 65 values are in use at once at steps[0] (payAll)');
  });

  test('more than 128 values are past the instruction limit', () => {
    const template = () => compileTemplate(defineTemplate({ accounts, steps: balanceChecks(43) }));
    expect(template).toThrow('Template uses more than 128 VM instructions');
  });
});
