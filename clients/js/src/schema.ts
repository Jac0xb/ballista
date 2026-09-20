import * as z from 'zod';

const identifier = z.string().regex(/^[A-Za-z_][A-Za-z0-9_]*$/);
const u32 = z.number().int().min(0).max(0xffff_ffff);
const bytes32 = z.instanceof(Uint8Array).refine((value) => value.length === 32, {
  error: 'Expected 32 bytes',
});
const label = z.string().min(1).max(64).optional();
const bigintLike = z
  .union([z.bigint(), z.number().int().safe()])
  .transform((value) => BigInt(value));
const rangedBigint = (minimum: bigint, maximum: bigint) =>
  bigintLike.refine((value) => value >= minimum && value <= maximum, {
    error: `Expected an integer from ${minimum} to ${maximum}`,
  });

export const ValueTypeSchema = z.enum(['bool', 'u64', 'i64', 'u128', 'pubkey', 'bytes']);
export type ValueType = z.infer<typeof ValueTypeSchema>;

/** Widths a template can read from account data or return data. */
export const ReadTypeSchema = z.enum(['bool', 'u8', 'u16', 'u32', 'u64', 'i64', 'u128', 'pubkey']);
export type ReadType = z.infer<typeof ReadTypeSchema>;

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
  | { kind: 'pda'; program: AccountReference; seeds: Expression[] }
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
        | 'or';
      left: Expression;
      right: Expression;
    }
  | { kind: 'not'; value: Expression }
  | { kind: 'select'; condition: Expression; ifTrue: Expression; ifFalse: Expression }
  | { kind: 'cast'; to: 'u64' | 'i64' | 'u128'; value: Expression };

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
        ]),
        left: ExpressionSchema,
        right: ExpressionSchema,
      })
      .strict(),
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
      kind: 'forEach';
      steps: Step[];
      /** Variables defined before the loop whose values flow across iterations and out of it. */
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
    z
      .object({
        kind: z.literal('forEach'),
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
    version: z.literal(3).default(3),
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
        if (item.kind === 'forEach') checkGroups(item.steps, `${path}.${index}.steps`);
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
    const forEachSteps = template.steps.filter((step) => step.kind === 'forEach');
    if ((template.batch === undefined) !== (forEachSteps.length === 0) || forEachSteps.length > 1) {
      context.addIssue({
        code: 'custom',
        message: 'A batch schema requires exactly one top-level forEach step',
        path: ['steps'],
      });
    }
    for (const forEach of forEachSteps) {
      if (forEach.kind === 'forEach' && forEach.steps.some((step) => step.kind === 'forEach')) {
        context.addIssue({ code: 'custom', message: 'Nested iteration is not supported', path: ['steps'] });
      }
      if (forEach.kind === 'forEach' && new Set(forEach.carry ?? []).size !== (forEach.carry ?? []).length) {
        context.addIssue({ code: 'custom', message: 'Carried variables must be unique', path: ['steps'] });
      }
    }
    if (template.steps.some((step) => step.kind === 'assign')) {
      context.addIssue({ code: 'custom', message: 'assign is only valid inside forEach', path: ['steps'] });
    }
  });

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
  pda: (program: AccountReference, seeds: Expression[]): Expression => ({ kind: 'pda', program, seeds }),
  add: binary('add'),
  subtract: binary('subtract'),
  multiply: binary('multiply'),
  divide: binary('divide'),
  min: binary('min'),
  max: binary('max'),
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
  forEach: (steps: Step[], options: { carry?: string[]; label?: string } = {}): Step => ({
    kind: 'forEach',
    steps,
    ...(options.carry ? { carry: options.carry } : {}),
    ...(options.label ? { label: options.label } : {}),
  }),
};
