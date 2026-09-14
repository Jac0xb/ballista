import * as z from 'zod';

const identifier = z.string().regex(/^[A-Za-z_][A-Za-z0-9_]*$/);
const u32 = z.number().int().min(0).max(0xffff_ffff);
const bytes32 = z.instanceof(Uint8Array).refine((value) => value.length === 32, {
  error: 'Expected 32 bytes',
});
const bigintLike = z
  .union([z.bigint(), z.number().int().safe()])
  .transform((value) => BigInt(value));
const rangedBigint = (minimum: bigint, maximum: bigint) =>
  bigintLike.refine((value) => value >= minimum && value <= maximum, {
    error: `Expected an integer from ${minimum} to ${maximum}`,
  });

export const ValueTypeSchema = z.enum(['bool', 'u64', 'i64', 'u128', 'pubkey', 'bytes']);
export type ValueType = z.infer<typeof ValueTypeSchema>;

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
  })
  .strict();
export type AccountConstraint = z.infer<typeof AccountConstraintSchema>;

export type Expression =
  | { kind: 'input'; name: string }
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
      offset: number;
      type: 'bool' | 'u8' | 'u16' | 'u32' | 'u64' | 'i64' | 'u128' | 'pubkey';
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
        offset: u32,
        type: z.enum(['bool', 'u8', 'u16', 'u32', 'u64', 'i64', 'u128', 'pubkey']),
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
  | { kind: 'require'; condition: Expression }
  | { kind: 'let'; name: string; value: Expression }
  | {
      kind: 'invoke';
      program: AccountReference;
      accounts: z.infer<typeof InvokeAccountSchema>[];
      data: DataPart[];
      when?: Expression;
    }
  | { kind: 'forEach'; steps: Step[] };

export const StepSchema: z.ZodType<Step> = z.lazy(() =>
  z.discriminatedUnion('kind', [
    z.object({ kind: z.literal('require'), condition: ExpressionSchema }).strict(),
    z.object({ kind: z.literal('let'), name: identifier, value: ExpressionSchema }).strict(),
    z
      .object({
        kind: z.literal('invoke'),
        program: AccountReferenceSchema,
        accounts: z.array(InvokeAccountSchema).max(64),
        data: z.array(DataPartSchema).max(64),
        when: ExpressionSchema.optional(),
      })
      .strict(),
    z.object({ kind: z.literal('forEach'), steps: z.array(StepSchema).min(1).max(64) }).strict(),
  ]),
);

const namedInputs = z.record(identifier, InputSchema);
const namedAccounts = z.record(identifier, AccountConstraintSchema);

export const TemplateSchema = z
  .object({
    version: z.literal(2).default(2),
    inputs: namedInputs.default({}),
    accounts: namedAccounts,
    batch: z
      .object({
        maxIterations: z.number().int().min(1).max(60),
        row: namedAccounts.refine((row) => Object.keys(row).length >= 1 && Object.keys(row).length <= 8),
      })
      .strict()
      .optional(),
    steps: z.array(StepSchema).min(1).max(128),
  })
  .strict()
  .superRefine((template, context) => {
    if (Object.keys(template.inputs).length > 32) {
      context.addIssue({ code: 'custom', message: 'Templates support at most 32 inputs', path: ['inputs'] });
    }
    const stride = template.batch ? Object.keys(template.batch.row).length : 0;
    if (Object.keys(template.accounts).length + stride * (template.batch?.maxIterations ?? 0) > 60) {
      context.addIssue({
        code: 'custom',
        message: 'Fixed accounts plus the maximum batch range exceeds 60 runtime accounts',
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
    offset: number,
    type: Extract<Expression, { kind: 'accountData' }>['type'],
  ): Expression => ({ kind: 'accountData', account: accountReference, offset, type }),
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
  require: (condition: Expression): Step => ({ kind: 'require', condition }),
  let: (name: string, value: Expression): Step => ({ kind: 'let', name, value }),
  snapshot: (name: string, value: Expression): Step => ({ kind: 'let', name, value }),
  invoke: (input: Omit<Extract<Step, { kind: 'invoke' }>, 'kind'>): Step => ({ kind: 'invoke', ...input }),
  forEach: (steps: Step[]): Step => ({ kind: 'forEach', steps }),
};
