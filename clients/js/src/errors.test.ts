import { describe, expect, test } from 'vitest';

import {
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  decodeBallistaError,
  defineTemplate,
  explainRunError,
  expression,
  step,
  systemTransfer,
} from './index.js';

const budgeted = compileTemplate(
  defineTemplate({
    inputs: { amount: { type: 'u64' }, budget: { type: 'u64' } },
    accounts: {
      systemProgram: { executable: true, address: SYSTEM_PROGRAM_ADDRESS_BYTES },
      source: { signer: true, writable: true },
    },
    batch: { maxIterations: 3, row: { recipient: { writable: true } } },
    steps: [
      step.let('total', expression.u64(0)),
      step.forEach(
        [
          systemTransfer({
            systemProgram: account.fixed('systemProgram'),
            from: account.fixed('source'),
            to: account.iteration('recipient'),
            lamports: expression.input('amount'),
            label: 'pay',
          }),
          step.assign('total', expression.add(expression.variable('total'), expression.input('amount'))),
        ],
        { carry: ['total'] },
      ),
      step.require(expression.lessThanOrEqual(expression.variable('total'), expression.input('budget')), 'withinBudget'),
    ],
  }),
);

describe('error decoding', () => {
  test('splits runtime and verifier codes into kind and context', () => {
    expect(decodeBallistaError(6015)).toEqual({ code: 6015, kind: 6015, name: 'RequirementFailed', context: 0, source: 'runtime' });
    expect(decodeBallistaError((7 << 16) | 6015)).toMatchObject({ name: 'RequirementFailed', context: 7 });
    expect(decodeBallistaError((3 << 16) | 6115)).toMatchObject({ name: 'InvalidCpi', context: 3, source: 'verifier' });
    expect(decodeBallistaError(6127)).toMatchObject({ name: 'InvalidMinIterations' });
    expect(decodeBallistaError(6001)).toMatchObject({ name: 'InvalidTemplateAccount' });
    expect(decodeBallistaError(1)).toBeUndefined();
    expect(decodeBallistaError(6099)).toBeUndefined();
    expect(decodeBallistaError(6128)).toBeUndefined();
    expect(decodeBallistaError(-1)).toBeUndefined();
  });

  test('explains a failed require by its step and label', () => {
    const requirePc = budgeted.sourceMap.find((entry) => entry.label === 'withinBudget')!.pc;
    const explanation = explainRunError((requirePc << 16) | 6015, budgeted);
    expect(explanation).toMatchObject({
      error: { name: 'RequirementFailed', context: requirePc },
      step: { pc: requirePc, path: 'steps[2]', label: 'withinBudget' },
      message: `RequirementFailed at steps[2] (withinBudget)`,
    });
  });

  test('explains account, input, and range failures by name', () => {
    expect(explainRunError((1 << 16) | 6020, budgeted)).toMatchObject({
      account: { index: 1, name: 'source' },
      message: 'AccountConstraintFailed: account source does not satisfy its constraint',
    });
    expect(explainRunError((4 << 16) | 6020, budgeted)).toMatchObject({
      account: { index: 4, name: 'recipient', row: 2 },
      message: 'AccountConstraintFailed: account recipient in row 2 does not satisfy its constraint',
    });
    expect(explainRunError((1 << 16) | 6008, budgeted)).toMatchObject({
      input: 'budget',
      message: 'InvalidRunInputs: input budget could not be decoded',
    });
    expect(explainRunError((2 << 16) | 6008, budgeted)?.message).toBe('InvalidRunInputs: unexpected trailing input bytes');
    expect(explainRunError((4 << 16) | 6010, budgeted)?.message).toBe(
      'InvalidAccountRange: 4 runtime accounts or iterations were supplied',
    );
    expect(explainRunError(6115, budgeted)?.message).toBe('InvalidCpi (verifier context 0)');
    expect(explainRunError(1, budgeted)).toBeUndefined();
  });

  test('falls back to the instruction index when a program counter has no source entry', () => {
    expect(explainRunError((200 << 16) | 6013, budgeted)?.message).toBe('ArithmeticOverflow at instruction 200');
  });
});
