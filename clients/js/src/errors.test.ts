import { describe, expect, test } from 'vitest';

import {
  BALLISTA_PROGRAM_ADDRESS,
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  account,
  compileTemplate,
  decodeBallistaError,
  decodeBallistaFailure,
  defineTemplate,
  explainRunError,
  expression,
  failedProgram,
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
    expect(decodeBallistaError(6128)).toMatchObject({ name: 'TooManyAccountGroups' });
    expect(decodeBallistaError(6001)).toMatchObject({ name: 'InvalidTemplateAccount' });
    expect(decodeBallistaError((65 << 16) | 6021)).toMatchObject({ name: 'CpiAccountLimitExceeded', context: 65 });
    expect(decodeBallistaError((4 << 16) | 6022)).toMatchObject({ name: 'LoopCountExceeded', context: 4, source: 'runtime' });
    expect(decodeBallistaError((2 << 16) | 6129)).toMatchObject({ name: 'InvalidLoop', context: 2, source: 'verifier' });
    expect(decodeBallistaError(1)).toBeUndefined();
    expect(decodeBallistaError((4 << 16) | 6023)).toMatchObject({ name: 'InstructionOutOfRange', context: 4 });
    expect(decodeBallistaError(6024)).toMatchObject({ name: 'WritableAccountBytesRead', source: 'runtime' });
    expect(decodeBallistaError(6131)).toMatchObject({ name: 'InvalidIntrospection', source: 'verifier' });
    expect(decodeBallistaError((3 << 16) | 6025)).toMatchObject({ name: 'InvalidRegistryEntry', context: 3, source: 'runtime' });
    expect(decodeBallistaError(6026)).toMatchObject({ name: 'RegistryReentry', source: 'runtime' });
    expect(decodeBallistaError(6132)).toMatchObject({ name: 'InvalidRegistry', source: 'verifier' });
    expect(decodeBallistaError(6027)).toBeUndefined();
    expect(decodeBallistaError(6099)).toBeUndefined();
    expect(decodeBallistaError((5 << 16) | 6130)).toMatchObject({ name: 'InvalidOutput', context: 5, source: 'verifier' });
    expect(decodeBallistaError(6133)).toMatchObject({ name: 'InvalidAccountGroup', source: 'verifier' });
    expect(decodeBallistaError(6134)).toBeUndefined();
    expect(decodeBallistaError(-1)).toBeUndefined();
  });

  test('explains a failed require by its step and label', () => {
    const requirePc = budgeted.sourceMap.filter((entry) => entry.label === 'withinBudget').at(-1)!.pc;
    const explanation = explainRunError((requirePc << 16) | 6015, budgeted);
    expect(explanation).toMatchObject({
      error: { name: 'RequirementFailed', context: requirePc },
      step: { pc: requirePc, path: 'steps[2]', label: 'withinBudget' },
      message: `RequirementFailed at steps[2] (withinBudget)`,
    });
  });

  test('names a row input and its row, and gives both readings on a row boundary', () => {
    const rows = compileTemplate(
      defineTemplate({
        inputs: { fee: { type: 'u64' } },
        accounts: {},
        batch: { maxIterations: 4, row: { recipient: {} }, rowInputs: { amount: { type: 'u64' }, memo: { type: 'u64' } } },
        steps: [step.forEach([step.require(expression.greaterThanOrEqual(expression.rowInput('amount'), expression.rowInput('memo')))])],
      }),
    );
    expect(explainRunError(6008, rows)).toMatchObject({ input: 'fee', message: 'InvalidRunInputs: input fee could not be decoded' });
    // Mid-row: only a decode failure reads this way.
    expect(explainRunError((2 << 16) | 6008, rows)).toMatchObject({
      input: 'memo',
      inputRow: 0,
      message: 'InvalidRunInputs: input memo in row 0 could not be decoded',
    });
    expect(explainRunError((4 << 16) | 6008, rows)?.message).toBe('InvalidRunInputs: input memo in row 1 could not be decoded');
    // On a row boundary, trailing bytes after the previous row report the same index.
    expect(explainRunError((1 << 16) | 6008, rows)?.message).toBe(
      'InvalidRunInputs: input amount in row 0 could not be decoded, or unexpected trailing input bytes',
    );
    expect(explainRunError((3 << 16) | 6008, rows)).toMatchObject({
      input: 'amount',
      inputRow: 1,
      message: 'InvalidRunInputs: input amount in row 1 could not be decoded, or unexpected trailing input bytes after row 0',
    });

    const grouped = compileTemplate(
      defineTemplate({
        accounts: {},
        accountGroups: ['extra'],
        steps: [step.require(expression.greaterThan(expression.groupLength('extra'), expression.u64(0)))],
      }),
    );
    expect(explainRunError(6008, grouped)?.message).toBe(
      'InvalidRunInputs: unexpected trailing input bytes, or the account-group length prefix is short',
    );
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

  test('explains a count above its maximum by the repeat step', () => {
    const counted = compileTemplate(
      defineTemplate({
        inputs: { rounds: { type: 'u64' } },
        accounts: {},
        steps: [step.repeat(expression.input('rounds'), [step.require(expression.bool(true))], { max: 3, label: 'rounds' })],
      }),
    );
    const repeatPc = counted.sourceMap.find((entry) => entry.label === 'rounds')!.pc;
    expect(explainRunError((repeatPc << 16) | 6022, counted)?.message).toBe('LoopCountExceeded at steps[0] (rounds)');
  });

  test('falls back to the instruction index when a program counter has no source entry', () => {
    expect(explainRunError((200 << 16) | 6013, budgeted)?.message).toBe('ArithmeticOverflow at instruction 200');
  });
});

describe('codes from Kit', () => {
  // Kit's RPC turns every integer it was not told to keep as a number into a `bigint`, a
  // simulation's `Custom` code included, though its type says `number`.
  test('decode as a bigint as they do as a number', () => {
    expect(decodeBallistaError((7n << 16n) | 6015n)).toEqual(decodeBallistaError((7 << 16) | 6015));
    expect(decodeBallistaError((7n << 16n) | 6015n)).toMatchObject({ code: 464_767, name: 'RequirementFailed', context: 7 });
    expect(decodeBallistaError(0xffff_17d4n)).toMatchObject({ name: 'Truncated', context: 0xffff });
    expect(decodeBallistaError(1n)).toBeUndefined();
    expect(decodeBallistaError(-1n)).toBeUndefined();
    expect(decodeBallistaError(0x1_0000_177fn)).toBeUndefined();
  });

  test('explain as a bigint as they do as a number', () => {
    const requirePc = budgeted.sourceMap.filter((entry) => entry.label === 'withinBudget').at(-1)!.pc;
    expect(explainRunError((BigInt(requirePc) << 16n) | 6015n, budgeted)?.message).toBe(
      'RequirementFailed at steps[2] (withinBudget)',
    );
  });
});

describe('which program failed', () => {
  const JUPITER = 'JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4';
  const OTHER_DEPLOYMENT = 'BLSTAtest1111111111111111111111111111111111';
  /** Jupiter's slippage check refuses a route that a run called: Jupiter's own 6001. */
  const jupiterRefused = [
    `Program ${BALLISTA_PROGRAM_ADDRESS} invoke [1]`,
    `Program ${JUPITER} invoke [2]`,
    'Program log: Instruction: Route',
    'Program log: AnchorError occurred. Error Code: SlippageToleranceExceeded. Error Number: 6001. Error Message: Slippage tolerance exceeded.',
    `Program ${JUPITER} consumed 31337 of 180000 compute units`,
    `Program ${JUPITER} failed: custom program error: 0x1771`,
    `Program ${BALLISTA_PROGRAM_ADDRESS} consumed 52000 of 200000 compute units`,
    `Program ${BALLISTA_PROGRAM_ADDRESS} failed: custom program error: 0x1771`,
  ];
  /** Ballista's own `require` fails at the budget check. */
  const requirePc = budgeted.sourceMap.filter((entry) => entry.label === 'withinBudget').at(-1)!.pc;
  const ballistaRefused = [
    `Program ${BALLISTA_PROGRAM_ADDRESS} invoke [1]`,
    `Program 11111111111111111111111111111111 invoke [2]`,
    `Program 11111111111111111111111111111111 success`,
    // `sol_log_64`: the program counter, the opcode, its operands and its destination.
    `Program log: 0x${requirePc.toString(16)}, 0x28, 0xff, 0x9, 0xff`,
    `Program ${BALLISTA_PROGRAM_ADDRESS} consumed 4100 of 200000 compute units`,
    `Program ${BALLISTA_PROGRAM_ADDRESS} failed: custom program error: 0x${((requirePc << 16) | 6015).toString(16)}`,
  ];
  const ballistaCode = (requirePc << 16) | 6015;

  test('is the first program to log a failure', () => {
    expect(failedProgram(jupiterRefused)).toBe(JUPITER);
    expect(failedProgram(ballistaRefused)).toBe(BALLISTA_PROGRAM_ADDRESS);
    expect(failedProgram([`Program ${BALLISTA_PROGRAM_ADDRESS} invoke [1]`, 'Log truncated'])).toBeUndefined();
    expect(failedProgram([])).toBeUndefined();
    // A program's own log text cannot pose as the runtime's line.
    expect(failedProgram([`Program log: Program ${JUPITER} failed: custom program error: 0x1`])).toBeUndefined();
  });

  test('decodes a code as Ballista\'s only when Ballista failed', () => {
    expect(decodeBallistaError(6001)).toMatchObject({ name: 'InvalidTemplateAccount' });
    expect(decodeBallistaFailure(6001, jupiterRefused)).toBeUndefined();
    expect(decodeBallistaFailure(6001n, jupiterRefused)).toBeUndefined();
    expect(decodeBallistaFailure(ballistaCode, ballistaRefused)).toMatchObject({
      name: 'RequirementFailed',
      context: requirePc,
    });
    expect(decodeBallistaFailure(ballistaCode, [])).toBeUndefined();
  });

  test('takes another deployment of Ballista', () => {
    const logs = ballistaRefused.map((line) => line.replaceAll(BALLISTA_PROGRAM_ADDRESS, OTHER_DEPLOYMENT));
    expect(decodeBallistaFailure(ballistaCode, logs)).toBeUndefined();
    expect(decodeBallistaFailure(ballistaCode, logs, OTHER_DEPLOYMENT)).toMatchObject({ name: 'RequirementFailed' });
  });

  // Bug (docs/superpowers/specs/2026-10-03-safety-properties.md, P91 and finding F9): in a nested
  // run the inner run logs its failure first, so `failedProgram` finds Ballista and
  // `explainRunError` maps the inner template's program counter onto the outer template's source
  // map. Skipped until the SDK tells the two runs apart; it fails today, naming the outer
  // `withinBudget` step.
  test.skip('does not explain a failure inside a nested run by a step of the outer template', () => {
    const failed = `failed: custom program error: 0x${ballistaCode.toString(16)}`;
    const nestedRefused = [
      `Program ${BALLISTA_PROGRAM_ADDRESS} invoke [1]`,
      `Program ${BALLISTA_PROGRAM_ADDRESS} invoke [2]`,
      `Program log: 0x${requirePc.toString(16)}, 0x28, 0xff, 0x9, 0xff`,
      `Program ${BALLISTA_PROGRAM_ADDRESS} consumed 900 of 190000 compute units`,
      `Program ${BALLISTA_PROGRAM_ADDRESS} ${failed}`,
      `Program ${BALLISTA_PROGRAM_ADDRESS} consumed 4100 of 200000 compute units`,
      `Program ${BALLISTA_PROGRAM_ADDRESS} ${failed}`,
    ];
    // The `require` that failed is the inner template's; it only shares the outer one's pc.
    expect(explainRunError(ballistaCode, budgeted, { logs: nestedRefused })?.step).toBeUndefined();
  });

  test('explains a run error only when Ballista failed, given the logs', () => {
    expect(explainRunError(6001, budgeted)?.message).toMatch(/^InvalidTemplateAccount/);
    expect(explainRunError(6001, budgeted, { logs: jupiterRefused })).toBeUndefined();
    expect(explainRunError(BigInt(ballistaCode), budgeted, { logs: ballistaRefused })?.message).toBe(
      'RequirementFailed at steps[2] (withinBudget)',
    );
  });
});
