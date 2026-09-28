import * as z from 'zod';

const identifier = z.string().regex(/^[A-Za-z_][A-Za-z0-9_]*$/);
const u32 = z.number().int().min(0).max(0xffff_ffff);
const bytes32 = z.instanceof(Uint8Array).refine((value) => value.length === 32, {
  error: 'Expected 32 bytes',
});
const label = z.string().min(1).max(64).optional();
/** A byte read's length: 1 to 1,024, the limit on every `bytes` value. */
const byteReadLength = z.number().int().min(1).max(1_024);
const bigintLike = z
  .union([z.bigint(), z.number().int().safe()])
  .transform((value) => BigInt(value));
const rangedBigint = (minimum: bigint, maximum: bigint) =>
  bigintLike.refine((value) => value >= minimum && value <= maximum, {
    error: `Expected an integer from ${minimum} to ${maximum}`,
  });

export const ValueTypeSchema = z.enum(['bool', 'u64', 'i64', 'u128', 'pubkey', 'bytes']);
export type ValueType = z.infer<typeof ValueTypeSchema>;

/** Widths a template can read from account data, return data, or instruction data. */
export const ReadTypeSchema = z.enum(['bool', 'u8', 'u16', 'u32', 'i32', 'u64', 'i64', 'u128', 'pubkey']);
export type ReadType = z.infer<typeof ReadTypeSchema>;

/** Bytes each read type occupies. Frozen, because the SDK's bounds checks depend on these widths. */
export const readWidth: Readonly<Record<ReadType, number>> = Object.freeze({
  bool: 1,
  u8: 1,
  u16: 2,
  u32: 4,
  i32: 4,
  u64: 8,
  i64: 8,
  u128: 16,
  pubkey: 32,
});

export const InputSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('bool') }).strict(),
  z.object({ type: z.literal('u64') }).strict(),
  z.object({ type: z.literal('i64') }).strict(),
  z.object({ type: z.literal('u128') }).strict(),
  z.object({ type: z.literal('pubkey') }).strict(),
  z.object({ type: z.literal('bytes'), maxLength: z.number().int().min(1).max(1024) }).strict(),
]);
export type InputDefinition = z.infer<typeof InputSchema>;

export const LiteralSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('bool'), value: z.boolean() }).strict(),
  z.object({ type: z.literal('u64'), value: rangedBigint(0n, (1n << 64n) - 1n) }).strict(),
  z
    .object({
      type: z.literal('i64'),
      value: rangedBigint(-(1n << 63n), (1n << 63n) - 1n),
    })
    .strict(),
  z.object({ type: z.literal('u128'), value: rangedBigint(0n, (1n << 128n) - 1n) }).strict(),
  z.object({ type: z.literal('pubkey'), value: bytes32 }).strict(),
  z
    .object({
      type: z.literal('bytes'),
      value: z.instanceof(Uint8Array).refine((value) => value.length <= 1024),
    })
    .strict(),
]);
export type Literal = z.infer<typeof LiteralSchema>;

export type AccountReference =
  | { kind: 'account'; name: string }
  | { kind: 'iterationAccount'; name: string };

export const AccountReferenceSchema: z.ZodType<AccountReference> = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('account'), name: identifier }).strict(),
  z.object({ kind: z.literal('iterationAccount'), name: identifier }).strict(),
]);

export const AccountConstraintSchema = z
  .object({
    signer: z.boolean().default(false),
    writable: z.boolean().default(false),
    executable: z.boolean().default(false),
    address: bytes32.optional(),
    owner: bytes32.optional(),
    minDataLength: u32.default(0),
    /**
     * Opts this account out of the compiler's pin requirements. Programs that are invoked or used
     * to derive PDAs normally need `address`; accounts whose data is read need `owner` or
     * `address`. With this flag the template trusts whatever the caller supplies for the account.
     */
    unsafeUnpinned: z.boolean().default(false),
  })
  .strict();
export type AccountConstraint = z.infer<typeof AccountConstraintSchema>;

export type Expression =
  | { kind: 'input'; name: string }
  /** A batch row input of the current iteration; valid inside `forEach` only. */
  | { kind: 'rowInput'; name: string }
  | { kind: 'variable'; name: string }
  | { kind: 'literal'; value: Literal }
  | {
      kind: 'accountField';
      account: AccountReference;
      field: 'key' | 'owner' | 'lamports' | 'dataLength' | 'isEmpty';
    }
  | {
      kind: 'accountData';
      account: AccountReference;
      /** A fixed byte offset, or a `u64` expression evaluated at run time. */
      offset: number | Expression;
      type: ReadType;
    }
  | {
      /** A typed read of the return data set by the invoke immediately before this step. */
      kind: 'returnData';
      offset: number;
      type: ReadType;
    }
  | { kind: 'clock'; field: 'slot' | 'unixTimestamp' }
  | { kind: 'loopIndex' }
  | { kind: 'pda'; program: AccountReference; seeds: Expression[]; bump?: Expression }
  | {
      kind: 'binary';
      op:
        | 'add'
        | 'subtract'
        | 'multiply'
        | 'divide'
        | 'min'
        | 'max'
        | 'equal'
        | 'notEqual'
        | 'lessThan'
        | 'lessThanOrEqual'
        | 'greaterThan'
        | 'greaterThanOrEqual'
        | 'and'
        | 'or'
        | 'remainder'
        | 'shiftLeft'
        | 'shiftRight'
        | 'bitAnd'
        | 'bitOr'
        | 'bitXor';
      left: Expression;
      right: Expression;
    }
  | {
      /** `left × right ÷ divisor` with the product computed exactly. */
      kind: 'multiplyDivide';
      left: Expression;
      right: Expression;
      divisor: Expression;
      rounding: 'down' | 'up';
    }
  | { kind: 'powerOfTen'; exponent: Expression }
  | { kind: 'not'; value: Expression }
  | { kind: 'select'; condition: Expression; ifTrue: Expression; ifFalse: Expression }
  | { kind: 'cast'; to: 'u64' | 'i64' | 'u128'; value: Expression }
  /** How many instructions the transaction holds, from the Instructions sysvar `sysvar` names. */
  | { kind: 'instructionCount'; sysvar: AccountReference }
  /** The index of the instruction running this template. */
  | { kind: 'currentInstructionIndex'; sysvar: AccountReference }
  | {
      /** A field of the transaction's instruction at `index`. */
      kind: 'instruction';
      sysvar: AccountReference;
      index: Expression;
      field: 'program' | 'accountCount' | 'dataLength';
    }
  | {
      /** Account `position` of instruction `index`: its key, or its flags (bit 0 signer, bit 1 writable). */
      kind: 'instructionAccount';
      sysvar: AccountReference;
      index: Expression;
      position: Expression;
      field: 'key' | 'flags';
    }
  | {
      /** A typed read from instruction `index`'s data at a `u64` offset. */
      kind: 'instructionData';
      sysvar: AccountReference;
      index: Expression;
      offset: Expression;
      type: ReadType;
    }
  | {
      /** Exactly `length` bytes of instruction `index`'s data from a `u64` offset. */
      kind: 'instructionDataBytes';
      sysvar: AccountReference;
      index: Expression;
      offset: Expression;
      length: number;
    }
  | {
      /** Exactly `length` bytes of a read-only account's data from a `u64` offset. */
      kind: 'accountDataBytes';
      account: AccountReference;
      offset: Expression;
      length: number;
    }
  | { kind: 'bytesLength'; value: Expression };

export const ExpressionSchema: z.ZodType<Expression> = z.lazy(() =>
  z.discriminatedUnion('kind', [
    z.object({ kind: z.literal('input'), name: identifier }).strict(),
    z.object({ kind: z.literal('rowInput'), name: identifier }).strict(),
    z.object({ kind: z.literal('variable'), name: identifier }).strict(),
    z.object({ kind: z.literal('literal'), value: LiteralSchema }).strict(),
    z
      .object({
        kind: z.literal('accountField'),
        account: AccountReferenceSchema,
        field: z.enum(['key', 'owner', 'lamports', 'dataLength', 'isEmpty']),
      })
      .strict(),
    z
      .object({
        kind: z.literal('accountData'),
        account: AccountReferenceSchema,
        offset: z.union([u32, ExpressionSchema]),
        type: ReadTypeSchema,
      })
      .strict(),
    z
      .object({
        kind: z.literal('returnData'),
        offset: u32,
        type: ReadTypeSchema,
      })
      .strict(),
    z.object({ kind: z.literal('clock'), field: z.enum(['slot', 'unixTimestamp']) }).strict(),
    z.object({ kind: z.literal('loopIndex') }).strict(),
    z
      .object({
        kind: z.literal('pda'),
        program: AccountReferenceSchema,
        seeds: z.array(ExpressionSchema).min(1).max(15),
        bump: ExpressionSchema.optional(),
      })
      .strict(),
    z
      .object({
        kind: z.literal('binary'),
        op: z.enum([
          'add',
          'subtract',
          'multiply',
          'divide',
          'min',
          'max',
          'equal',
          'notEqual',
          'lessThan',
          'lessThanOrEqual',
          'greaterThan',
          'greaterThanOrEqual',
          'and',
          'or',
          'remainder',
          'shiftLeft',
          'shiftRight',
          'bitAnd',
          'bitOr',
          'bitXor',
        ]),
        left: ExpressionSchema,
        right: ExpressionSchema,
      })
      .strict(),
    z
      .object({
        kind: z.literal('multiplyDivide'),
        left: ExpressionSchema,
        right: ExpressionSchema,
        divisor: ExpressionSchema,
        rounding: z.enum(['down', 'up']),
      })
      .strict(),
    z.object({ kind: z.literal('powerOfTen'), exponent: ExpressionSchema }).strict(),
    z.object({ kind: z.literal('not'), value: ExpressionSchema }).strict(),
    z
      .object({
        kind: z.literal('select'),
        condition: ExpressionSchema,
        ifTrue: ExpressionSchema,
        ifFalse: ExpressionSchema,
      })
      .strict(),
    z
      .object({
        kind: z.literal('cast'),
        to: z.enum(['u64', 'i64', 'u128']),
        value: ExpressionSchema,
      })
      .strict(),
    z.object({ kind: z.literal('instructionCount'), sysvar: AccountReferenceSchema }).strict(),
    z.object({ kind: z.literal('currentInstructionIndex'), sysvar: AccountReferenceSchema }).strict(),
    z
      .object({
        kind: z.literal('instruction'),
        sysvar: AccountReferenceSchema,
        index: ExpressionSchema,
        field: z.enum(['program', 'accountCount', 'dataLength']),
      })
      .strict(),
    z
      .object({
        kind: z.literal('instructionAccount'),
        sysvar: AccountReferenceSchema,
        index: ExpressionSchema,
        position: ExpressionSchema,
        field: z.enum(['key', 'flags']),
      })
      .strict(),
    z
      .object({
        kind: z.literal('instructionData'),
        sysvar: AccountReferenceSchema,
        index: ExpressionSchema,
        offset: ExpressionSchema,
        type: ReadTypeSchema,
      })
      .strict(),
    z
      .object({
        kind: z.literal('instructionDataBytes'),
        sysvar: AccountReferenceSchema,
        index: ExpressionSchema,
        offset: ExpressionSchema,
        length: byteReadLength,
      })
      .strict(),
    z
      .object({
        kind: z.literal('accountDataBytes'),
        account: AccountReferenceSchema,
        offset: ExpressionSchema,
        length: byteReadLength,
      })
      .strict(),
    z.object({ kind: z.literal('bytesLength'), value: ExpressionSchema }).strict(),
  ]),
);

export type DataPart =
  | { kind: 'literal'; bytes: Uint8Array }
  | {
      kind: 'encoded';
      encoding: 'u8' | 'u16' | 'u32' | 'u64' | 'i64' | 'u128' | 'pubkey' | 'bool' | 'bytes';
      value: Expression;
    };

export const DataPartSchema: z.ZodType<DataPart> = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('literal'), bytes: z.instanceof(Uint8Array) }).strict(),
  z
    .object({
      kind: z.literal('encoded'),
      encoding: z.enum(['u8', 'u16', 'u32', 'u64', 'i64', 'u128', 'pubkey', 'bool', 'bytes']),
      value: ExpressionSchema,
    })
    .strict(),
]);

export const InvokeAccountSchema = z
  .object({
    account: AccountReferenceSchema,
    signer: z.boolean().default(false),
    writable: z.boolean().default(false),
  })
  .strict();

export type Step =
  | { kind: 'require'; condition: Expression; label?: string }
  | { kind: 'let'; name: string; value: Expression; label?: string }
  | {
      /** Reassigns a variable listed in the enclosing loop's `carry`. */
      kind: 'assign';
      name: string;
      value: Expression;
      label?: string;
    }
  | {
      kind: 'invoke';
      program: AccountReference;
      accounts: z.infer<typeof InvokeAccountSchema>[];
      data: DataPart[];
      when?: Expression;
      /** A declared account group whose members follow `accounts` in the CPI. */
      accountGroup?: string;
      /** The program this step is written for; compilation fails if the account pins another. */
      programAddress?: Uint8Array;
      label?: string;
    }
  | {
      /**
       * Logs the encoded parts as one `Program data:` field. The first part must be a literal tag
       * of at least `MIN_EMIT_TAG_LENGTH` (4) bytes that does not start with `RUN_EVENT_TAG_FAMILY`
       * ("BEV"), so the log cannot pass for Ballista's run event.
       */
      kind: 'emit';
      parts: DataPart[];
      label?: string;
    }
  | {
      /**
       * Sets the encoded parts as the run's return data. Once per template, outside every loop, and
       * after the last invoke, because invoking a program clears return data.
       */
      kind: 'setReturnData';
      parts: DataPart[];
      label?: string;
    }
  | {
      kind: 'forEach';
      steps: Step[];
      /** Variables defined before the loop whose values flow across iterations and out of it. */
      carry?: string[];
      label?: string;
    }
  | {
      /**
       * Runs `steps` `count` times. `count` is a u64 evaluated once, before the first pass; a run
       * whose count is above `max` fails with `LoopCountExceeded`.
       */
      kind: 'repeat';
      count: Expression;
      /** The most passes the loop may make, 1 to 255. The worst-case CPI count assumes all of them. */
      max: number;
      steps: Step[];
      /** Variables defined before the loop whose values flow across passes and out of it. */
      carry?: string[];
      label?: string;
    };

export const StepSchema: z.ZodType<Step> = z.lazy(() =>
  z.discriminatedUnion('kind', [
    z.object({ kind: z.literal('require'), condition: ExpressionSchema, label }).strict(),
    z.object({ kind: z.literal('let'), name: identifier, value: ExpressionSchema, label }).strict(),
    z.object({ kind: z.literal('assign'), name: identifier, value: ExpressionSchema, label }).strict(),
    z
      .object({
        kind: z.literal('invoke'),
        program: AccountReferenceSchema,
        accounts: z.array(InvokeAccountSchema).max(64),
        data: z.array(DataPartSchema).max(64),
        when: ExpressionSchema.optional(),
        accountGroup: identifier.optional(),
        programAddress: bytes32.optional(),
        label,
      })
      .strict(),
    z.object({ kind: z.literal('emit'), parts: z.array(DataPartSchema).min(1).max(64), label }).strict(),
    z.object({ kind: z.literal('setReturnData'), parts: z.array(DataPartSchema).min(1).max(64), label }).strict(),
    z
      .object({
        kind: z.literal('forEach'),
        steps: z.array(StepSchema).min(1).max(64),
        carry: z.array(identifier).max(64).optional(),
        label,
      })
      .strict(),
    z
      .object({
        kind: z.literal('repeat'),
        count: ExpressionSchema,
        max: z.number().int().min(1).max(255),
        steps: z.array(StepSchema).min(1).max(64),
        carry: z.array(identifier).max(64).optional(),
        label,
      })
      .strict(),
  ]),
);

const namedInputs = z.record(identifier, InputSchema);
const namedAccounts = z.record(identifier, AccountConstraintSchema);

export const TemplateSchema = z
  .object({
    version: z.literal(1).default(1),
    inputs: namedInputs.default({}),
    accounts: namedAccounts,
    batch: z
      .object({
        maxIterations: z.number().int().min(1).max(60),
        /** Runs with fewer rows than this fail instead of succeeding vacuously. */
        minIterations: z.number().int().min(0).max(60).default(0),
        row: namedAccounts.refine((row) => Object.keys(row).length >= 1 && Object.keys(row).length <= 8),
        /** Inputs carried once per iteration, after the fixed inputs in the run data. */
        rowInputs: namedInputs.default({}).refine((inputs) => Object.keys(inputs).length <= 8, {
          error: 'A batch row carries at most 8 inputs',
        }),
      })
      .strict()
      .refine((batch) => batch.minIterations <= batch.maxIterations, {
        error: 'minIterations cannot exceed maxIterations',
        path: ['minIterations'],
      })
      .optional(),
    /** Emit a `BEV1` data log after every successful run. */
    emitEvent: z.boolean().default(false),
    /**
     * Caller-sized groups of accounts, supplied at run time after the batch rows. A CPI names one
     * to forward its members after the CPI's declared accounts. Members carry no constraints,
     * cannot be read, and never sign.
     */
    accountGroups: z
      .array(identifier)
      .max(8)
      .default([])
      .refine((groups) => new Set(groups).size === groups.length, { error: 'Account group names must be unique' }),
    steps: z.array(StepSchema).min(1).max(128),
  })
  .strict()
  .superRefine((template, context) => {
    const rowInputCount = Object.keys(template.batch?.rowInputs ?? {}).length;
    if (Object.keys(template.inputs).length + rowInputCount > 32) {
      context.addIssue({ code: 'custom', message: 'Templates support at most 32 inputs including row inputs', path: ['inputs'] });
    }
    if (Object.keys(template.inputs).length + rowInputCount * (template.batch?.maxIterations ?? 0) > 256) {
      context.addIssue({
        code: 'custom',
        message: 'Fixed inputs plus row inputs times the maximum iterations exceed 256 values',
        path: ['batch', 'rowInputs'],
      });
    }
    const groupNames = new Set(template.accountGroups);
    const checkGroups = (steps: Step[], path: string) => {
      for (const [index, item] of steps.entries()) {
        if (item.kind === 'invoke' && item.accountGroup !== undefined && !groupNames.has(item.accountGroup)) {
          context.addIssue({ code: 'custom', message: `Unknown account group: ${item.accountGroup}`, path: [path, index, 'accountGroup'] });
        }
        if (item.kind === 'forEach' || item.kind === 'repeat') checkGroups(item.steps, `${path}.${index}.steps`);
      }
    };
    checkGroups(template.steps, 'steps');
    const stride = template.batch ? Object.keys(template.batch.row).length : 0;
    if (Object.keys(template.accounts).length + stride * (template.batch?.maxIterations ?? 0) > 120) {
      context.addIssue({
        code: 'custom',
        message: 'Fixed accounts plus the maximum batch range exceeds 120 runtime accounts',
        path: ['accounts'],
      });
    }
    const loops = template.steps.filter(isLoop);
    // Every forEach iterates the batch rows, so a batch needs at least one and a forEach needs a batch.
    if ((template.batch === undefined) !== loops.every((loop) => loop.kind !== 'forEach')) {
      context.addIssue({
        code: 'custom',
        message: 'A batch schema requires at least one top-level forEach step, and forEach requires a batch schema',
        path: ['steps'],
      });
    }
    if (loops.length > 8) {
      context.addIssue({ code: 'custom', message: 'A template holds at most 8 top-level loops', path: ['steps'] });
    }
    for (const loop of loops) {
      if (loop.steps.some(isLoop)) {
        context.addIssue({ code: 'custom', message: 'Nested iteration is not supported', path: ['steps'] });
      }
      if (new Set(loop.carry ?? []).size !== (loop.carry ?? []).length) {
        context.addIssue({ code: 'custom', message: 'Carried variables must be unique', path: ['steps'] });
      }
    }
    if (template.steps.some((step) => step.kind === 'assign')) {
      context.addIssue({ code: 'custom', message: 'assign is only valid inside a loop', path: ['steps'] });
    }
  });

/** The two loop steps: `forEach` over the batch rows, and `repeat` over a count. */
function isLoop(step: Step): step is Extract<Step, { kind: 'forEach' | 'repeat' }> {
  return step.kind === 'forEach' || step.kind === 'repeat';
}

export type Template = z.infer<typeof TemplateSchema>;
export type TemplateInput = z.input<typeof TemplateSchema>;

export function defineTemplate(input: TemplateInput): Template {
  return TemplateSchema.parse(input);
}

export const account = {
  fixed: (name: string): AccountReference => AccountReferenceSchema.parse({ kind: 'account', name }),
  iteration: (name: string): AccountReference =>
    AccountReferenceSchema.parse({ kind: 'iterationAccount', name }),
};

const literal = (value: Literal): Expression => ({ kind: 'literal', value: LiteralSchema.parse(value) });
const binary = (op: Extract<Expression, { kind: 'binary' }>['op']) =>
  (left: Expression, right: Expression): Expression => ({ kind: 'binary', op, left, right });
/** An index, position or offset: a number becomes a `u64` constant. */
const u64Operand = (value: number | Expression): Expression =>
  typeof value === 'number' ? literal({ type: 'u64', value: BigInt(value) }) : value;
const instructionField = (field: Extract<Expression, { kind: 'instruction' }>['field']) =>
  (sysvar: AccountReference, index: number | Expression): Expression => ({
    kind: 'instruction',
    sysvar,
    index: u64Operand(index),
    field,
  });
/** Whether a flag bit of account `position` of instruction `index` is set. */
const instructionAccountFlag = (bit: bigint) =>
  (sysvar: AccountReference, index: number | Expression, position: number | Expression): Expression => ({
    kind: 'binary',
    op: 'notEqual',
    left: {
      kind: 'binary',
      op: 'bitAnd',
      left: {
        kind: 'instructionAccount',
        sysvar,
        index: u64Operand(index),
        position: u64Operand(position),
        field: 'flags',
      },
      right: literal({ type: 'u64', value: bit }),
    },
    right: literal({ type: 'u64', value: 0n }),
  });

export const expression = {
  input: (name: string): Expression => ({ kind: 'input', name }),
  rowInput: (name: string): Expression => ({ kind: 'rowInput', name }),
  variable: (name: string): Expression => ({ kind: 'variable', name }),
  snapshot: (name: string): Expression => ({ kind: 'variable', name }),
  bool: (value: boolean): Expression => literal({ type: 'bool', value }),
  u64: (value: bigint | number): Expression => literal({ type: 'u64', value: BigInt(value) }),
  i64: (value: bigint | number): Expression => literal({ type: 'i64', value: BigInt(value) }),
  u128: (value: bigint | number): Expression => literal({ type: 'u128', value: BigInt(value) }),
  pubkey: (value: Uint8Array): Expression => literal({ type: 'pubkey', value: new Uint8Array(value) }),
  bytes: (value: Uint8Array): Expression => literal({ type: 'bytes', value: new Uint8Array(value) }),
  accountField: (
    accountReference: AccountReference,
    field: Extract<Expression, { kind: 'accountField' }>['field'],
  ): Expression => ({ kind: 'accountField', account: accountReference, field }),
  accountData: (
    accountReference: AccountReference,
    offset: number | Expression,
    type: ReadType,
  ): Expression => ({ kind: 'accountData', account: accountReference, offset, type }),
  returnData: (type: ReadType, offset = 0): Expression => ({ kind: 'returnData', offset, type }),
  clockSlot: (): Expression => ({ kind: 'clock', field: 'slot' }),
  clockUnixTimestamp: (): Expression => ({ kind: 'clock', field: 'unixTimestamp' }),
  loopIndex: (): Expression => ({ kind: 'loopIndex' }),
  pda: (program: AccountReference, seeds: Expression[], bump?: Expression): Expression =>
    bump === undefined ? { kind: 'pda', program, seeds } : { kind: 'pda', program, seeds, bump },
  add: binary('add'),
  subtract: binary('subtract'),
  multiply: binary('multiply'),
  divide: binary('divide'),
  min: binary('min'),
  max: binary('max'),
  remainder: binary('remainder'),
  shiftLeft: binary('shiftLeft'),
  shiftRight: binary('shiftRight'),
  bitAnd: binary('bitAnd'),
  bitOr: binary('bitOr'),
  bitXor: binary('bitXor'),
  multiplyDivide: (
    left: Expression,
    right: Expression,
    divisor: Expression,
    rounding: 'down' | 'up' = 'down',
  ): Expression => ({ kind: 'multiplyDivide', left, right, divisor, rounding }),
  powerOfTen: (exponent: Expression): Expression => ({ kind: 'powerOfTen', exponent }),
  equal: binary('equal'),
  notEqual: binary('notEqual'),
  lessThan: binary('lessThan'),
  lessThanOrEqual: binary('lessThanOrEqual'),
  greaterThan: binary('greaterThan'),
  greaterThanOrEqual: binary('greaterThanOrEqual'),
  and: binary('and'),
  or: binary('or'),
  not: (value: Expression): Expression => ({ kind: 'not', value }),
  select: (condition: Expression, ifTrue: Expression, ifFalse: Expression): Expression => ({
    kind: 'select',
    condition,
    ifTrue,
    ifFalse,
  }),
  cast: (to: 'u64' | 'i64' | 'u128', value: Expression): Expression => ({ kind: 'cast', to, value }),
  instructionCount: (sysvar: AccountReference): Expression => ({ kind: 'instructionCount', sysvar }),
  currentInstructionIndex: (sysvar: AccountReference): Expression => ({ kind: 'currentInstructionIndex', sysvar }),
  instructionProgram: instructionField('program'),
  instructionAccountCount: instructionField('accountCount'),
  instructionDataLength: instructionField('dataLength'),
  instructionAccount: (
    sysvar: AccountReference,
    index: number | Expression,
    position: number | Expression,
  ): Expression => ({
    kind: 'instructionAccount',
    sysvar,
    index: u64Operand(index),
    position: u64Operand(position),
    field: 'key',
  }),
  /** Bit 0 is set when the account signs the instruction, bit 1 when it is writable. */
  instructionAccountFlags: (
    sysvar: AccountReference,
    index: number | Expression,
    position: number | Expression,
  ): Expression => ({
    kind: 'instructionAccount',
    sysvar,
    index: u64Operand(index),
    position: u64Operand(position),
    field: 'flags',
  }),
  instructionAccountIsSigner: instructionAccountFlag(1n),
  instructionAccountIsWritable: instructionAccountFlag(2n),
  instructionData: (
    sysvar: AccountReference,
    index: number | Expression,
    offset: number | Expression,
    type: ReadType,
  ): Expression => ({ kind: 'instructionData', sysvar, index: u64Operand(index), offset: u64Operand(offset), type }),
  instructionDataBytes: (
    sysvar: AccountReference,
    index: number | Expression,
    offset: number | Expression,
    length: number,
  ): Expression => ({
    kind: 'instructionDataBytes',
    sysvar,
    index: u64Operand(index),
    offset: u64Operand(offset),
    length,
  }),
  accountDataBytes: (accountReference: AccountReference, offset: number | Expression, length: number): Expression => ({
    kind: 'accountDataBytes',
    account: accountReference,
    offset: u64Operand(offset),
    length,
  }),
  bytesLength: (value: Expression): Expression => ({ kind: 'bytesLength', value }),
};

export const data = {
  literal: (bytes: Uint8Array): DataPart => ({ kind: 'literal', bytes }),
  encode: (encoding: Extract<DataPart, { kind: 'encoded' }>['encoding'], value: Expression): DataPart => ({
    kind: 'encoded',
    encoding,
    value,
  }),
};

export const step = {
  require: (condition: Expression, label?: string): Step => ({
    kind: 'require',
    condition,
    ...(label ? { label } : {}),
  }),
  let: (name: string, value: Expression, label?: string): Step => ({
    kind: 'let',
    name,
    value,
    ...(label ? { label } : {}),
  }),
  snapshot: (name: string, value: Expression, label?: string): Step => ({
    kind: 'let',
    name,
    value,
    ...(label ? { label } : {}),
  }),
  assign: (name: string, value: Expression, label?: string): Step => ({
    kind: 'assign',
    name,
    value,
    ...(label ? { label } : {}),
  }),
  invoke: (input: Omit<Extract<Step, { kind: 'invoke' }>, 'kind'>): Step => ({ kind: 'invoke', ...input }),
  /**
   * Logs the parts, encoded as invocation data is, as one `Program data:` field. The first part
   * must be a literal tag of at least 4 bytes that does not start with "BEV", the run event's.
   */
  emit: (parts: DataPart[], label?: string): Step => ({ kind: 'emit', parts, ...(label ? { label } : {}) }),
  /** Sets the parts, encoded as invocation data is, as the run's return data. */
  setReturnData: (parts: DataPart[], label?: string): Step => ({
    kind: 'setReturnData',
    parts,
    ...(label ? { label } : {}),
  }),
  forEach: (steps: Step[], options: { carry?: string[]; label?: string } = {}): Step => ({
    kind: 'forEach',
    steps,
    ...(options.carry ? { carry: options.carry } : {}),
    ...(options.label ? { label: options.label } : {}),
  }),
  /** Runs `steps` `count` times, at most `options.max`; `count` is a u64 read once, before the first pass. */
  repeat: (count: Expression, steps: Step[], options: { max: number; carry?: string[]; label?: string }): Step => ({
    kind: 'repeat',
    count,
    max: options.max,
    steps,
    ...(options.carry ? { carry: options.carry } : {}),
    ...(options.label ? { label: options.label } : {}),
  }),
};
