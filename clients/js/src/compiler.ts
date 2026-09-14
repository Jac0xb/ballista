import { sha256 } from '@noble/hashes/sha2.js';

import {
  TemplateSchema,
  type AccountConstraint,
  type AccountReference,
  type DataPart,
  type Expression,
  type InputDefinition,
  type Step,
  type Template,
  type TemplateInput,
  type ValueType,
} from './schema.js';

export const TEMPLATE_PROGRAM_VERSION = 2;
export const MAX_TEMPLATE_PAYLOAD_LENGTH = 10_240;
export const MAX_RUNTIME_ACCOUNTS = 60;
export const MAX_INPUT_BYTES = 1_024;
export const MAX_REGISTERS = 64;
export const MAX_VM_INSTRUCTIONS = 128;
export const MAX_EXPANDED_CPIS = 64;
export const MAX_CPI_DATA_LENGTH = 4_096;
export const MAX_PDA_SEEDS = 15;
export const MAX_PDA_SEED_LENGTH = 32;

const NO_INDEX = 0xff;
const ITERATION_ACCOUNT_BIT = 0x80;

const ACCOUNT_SIGNER = 1 << 0;
const ACCOUNT_WRITABLE = 1 << 1;
const ACCOUNT_EXECUTABLE = 1 << 2;

const valueTypeCode = { bool: 1, u64: 2, i64: 3, u128: 4, pubkey: 5, bytes: 6 } as const;

const opcode = {
  loadInput: 1,
  constBool: 2,
  constU64: 3,
  constI64: 4,
  constU128: 5,
  constPubkey: 6,
  constBytes: 7,
  accountKey: 8,
  accountOwner: 9,
  accountLamports: 10,
  accountDataLength: 11,
  accountIsEmpty: 12,
  readU64: 13,
  readI64: 14,
  readU128: 15,
  readPubkey: 16,
  clockSlot: 17,
  clockTimestamp: 18,
  add: 19,
  subtract: 20,
  multiply: 21,
  divide: 22,
  equal: 23,
  notEqual: 24,
  lessThan: 25,
  lessThanOrEqual: 26,
  greaterThan: 27,
  greaterThanOrEqual: 28,
  and: 29,
  or: 30,
  not: 31,
  min: 32,
  max: 33,
  select: 34,
  castU64: 35,
  castI64: 36,
  castU128: 37,
  loopIndex: 38,
  require: 40,
  invoke: 41,
  forEach: 42,
  readU8: 43,
  readU16: 44,
  readU32: 45,
  readBool: 46,
  derivePda: 47,
} as const;

const dataKind = {
  literal: 0,
  u8: 1,
  u16: 2,
  u32: 3,
  u64: 4,
  i64: 5,
  u128: 6,
  pubkey: 7,
  bool: 8,
  bytes: 9,
} as const;

class Writer {
  readonly bytes: number[] = [];

  u8(value: number): void {
    if (!Number.isInteger(value) || value < 0 || value > 0xff) throw new RangeError('Expected u8');
    this.bytes.push(value);
  }

  u16(value: number): void {
    if (!Number.isInteger(value) || value < 0 || value > 0xffff) throw new RangeError('Expected u16');
    this.bytes.push(value & 0xff, (value >>> 8) & 0xff);
  }

  u32(value: number): void {
    if (!Number.isInteger(value) || value < 0 || value > 0xffff_ffff) throw new RangeError('Expected u32');
    this.bytes.push(value & 0xff, (value >>> 8) & 0xff, (value >>> 16) & 0xff, (value >>> 24) & 0xff);
  }

  bigint(value: bigint, byteLength: number): void {
    const encoded = BigInt.asUintN(byteLength * 8, value);
    for (let index = 0; index < byteLength; index += 1) {
      this.u8(Number((encoded >> BigInt(index * 8)) & 0xffn));
    }
  }

  raw(value: Uint8Array | readonly number[]): void {
    this.bytes.push(...value);
  }

  finish(): Uint8Array {
    return Uint8Array.from(this.bytes);
  }
}

interface ExpressionResult {
  register: number;
  type: ValueType;
  maxLength: number;
}

type Bindings = Map<string, ExpressionResult>;

export interface CompileStats {
  payloadBytes: number;
  fixedAccounts: number;
  batchStride: number;
  batchMaxIterations: number;
  inputs: number;
  registers: number;
  instructions: number;
  cpis: number;
  maxExpandedCpis: number;
  maxCpiDataLength: number;
}

export interface CompiledTemplate {
  template: Template;
  bytes: Uint8Array;
  hash: Uint8Array;
  inputOrder: readonly string[];
  fixedAccountOrder: readonly string[];
  batchAccountOrder: readonly string[];
  stats: CompileStats;
}

class Compiler {
  readonly template: Template;
  readonly inputEntries: [string, InputDefinition][];
  readonly fixedEntries: [string, AccountConstraint][];
  readonly batchEntries: [string, AccountConstraint][];
  readonly inputIndices = new Map<string, number>();
  readonly fixedIndices = new Map<string, number>();
  readonly batchIndices = new Map<string, number>();
  readonly pubkeys: Uint8Array[] = [];
  readonly pubkeyIndices = new Map<string, number>();
  readonly blob: number[] = [];
  readonly accountRecords: Uint8Array[] = [];
  readonly inputRecords: Uint8Array[] = [];
  readonly instructions: Uint8Array[] = [];
  readonly cpis: Uint8Array[] = [];
  readonly cpiAccounts: Uint8Array[] = [];
  readonly dataSegments: Uint8Array[] = [];
  nextRegister = 0;
  maxCpiDataLength = 0;

  constructor(template: Template) {
    this.template = template;
    this.inputEntries = Object.entries(template.inputs);
    this.fixedEntries = Object.entries(template.accounts);
    this.batchEntries = Object.entries(template.batch?.row ?? {});
    this.inputEntries.forEach(([name], index) => this.inputIndices.set(name, index));
    this.fixedEntries.forEach(([name], index) => this.fixedIndices.set(name, index));
    this.batchEntries.forEach(([name], index) => this.batchIndices.set(name, index));
  }

  compile(): CompiledTemplate {
    for (const [, constraint] of [...this.fixedEntries, ...this.batchEntries]) {
      this.accountRecords.push(this.compileAccountConstraint(constraint));
    }
    for (const [, input] of this.inputEntries) this.inputRecords.push(this.compileInput(input));
    this.compileSteps(this.template.steps, false, new Map());

    if (this.nextRegister > MAX_REGISTERS) throw new RangeError('Template uses more than 64 registers');
    if (this.instructions.length > MAX_VM_INSTRUCTIONS) {
      throw new RangeError('Template uses more than 128 VM instructions');
    }
    if (this.cpis.length > 0xff || this.cpiAccounts.length > 0xffff || this.dataSegments.length > 0xffff) {
      throw new RangeError('Template table count exceeds the wire format');
    }
    if (this.pubkeys.length > 0xff || this.blob.length > 0xffff) {
      throw new RangeError('Template constants exceed the wire format');
    }

    const rootCpis = countCpis(this.template.steps.filter((item) => item.kind !== 'forEach'));
    const loop = this.template.steps.find((item) => item.kind === 'forEach');
    const loopCpis = loop?.kind === 'forEach' ? countCpis(loop.steps) : 0;
    const maxExpandedCpis = rootCpis + loopCpis * (this.template.batch?.maxIterations ?? 0);
    if (maxExpandedCpis > MAX_EXPANDED_CPIS) {
      throw new RangeError(`Template can expand to ${maxExpandedCpis} CPIs; maximum is 64`);
    }

    const header = new Writer();
    header.raw([0x42, 0x56, 0x4d, 0x32]);
    header.u8(TEMPLATE_PROGRAM_VERSION);
    header.u8(this.fixedEntries.length);
    header.u8(this.batchEntries.length);
    header.u8(this.template.batch?.maxIterations ?? 0);
    header.u8(this.inputEntries.length);
    header.u8(this.nextRegister);
    header.u8(this.instructions.length);
    header.u8(this.cpis.length);
    header.u16(this.cpiAccounts.length);
    header.u16(this.dataSegments.length);
    header.u8(this.pubkeys.length);
    header.u8(0);
    header.u16(this.blob.length);
    header.raw([0, 0, 0, 0]);

    const output = new Writer();
    output.raw(header.finish());
    for (const records of [
      this.accountRecords,
      this.inputRecords,
      this.instructions,
      this.cpis,
      this.cpiAccounts,
      this.dataSegments,
    ]) {
      for (const record of records) output.raw(record);
    }
    for (const pubkey of this.pubkeys) output.raw(pubkey);
    output.raw(this.blob);
    const bytes = output.finish();
    if (bytes.length > MAX_TEMPLATE_PAYLOAD_LENGTH) {
      throw new RangeError(`Compiled template is ${bytes.length} bytes; maximum is 10240`);
    }

    return {
      template: this.template,
      bytes,
      hash: sha256(bytes),
      inputOrder: this.inputEntries.map(([name]) => name),
      fixedAccountOrder: this.fixedEntries.map(([name]) => name),
      batchAccountOrder: this.batchEntries.map(([name]) => name),
      stats: {
        payloadBytes: bytes.length,
        fixedAccounts: this.fixedEntries.length,
        batchStride: this.batchEntries.length,
        batchMaxIterations: this.template.batch?.maxIterations ?? 0,
        inputs: this.inputEntries.length,
        registers: this.nextRegister,
        instructions: this.instructions.length,
        cpis: this.cpis.length,
        maxExpandedCpis,
        maxCpiDataLength: this.maxCpiDataLength,
      },
    };
  }

  compileAccountConstraint(constraint: AccountConstraint): Uint8Array {
    const writer = new Writer();
    writer.u8(
      (constraint.signer ? ACCOUNT_SIGNER : 0) |
        (constraint.writable ? ACCOUNT_WRITABLE : 0) |
        (constraint.executable ? ACCOUNT_EXECUTABLE : 0),
    );
    writer.u8(constraint.address ? this.addPubkey(constraint.address) : NO_INDEX);
    writer.u8(constraint.owner ? this.addPubkey(constraint.owner) : NO_INDEX);
    writer.u8(0);
    writer.u32(constraint.minDataLength);
    return writer.finish();
  }

  compileInput(input: InputDefinition): Uint8Array {
    const writer = new Writer();
    writer.u8(valueTypeCode[input.type]);
    writer.u8(0);
    writer.u16(input.type === 'bytes' ? input.maxLength : 0);
    return writer.finish();
  }

  compileSteps(steps: Step[], inLoop: boolean, bindings: Bindings): void {
    for (const current of steps) {
      if (current.kind === 'forEach') {
        if (inLoop) throw new TypeError('Nested forEach is not supported');
        const index = this.instructions.length;
        this.instructions.push(instructionRecord(opcode.forEach, NO_INDEX, 0));
        const bodyStart = this.instructions.length;
        this.compileSteps(current.steps, true, new Map(bindings));
        const bodyLength = this.instructions.length - bodyStart;
        if (bodyLength === 0 || bodyLength > 0xff) throw new RangeError('Invalid forEach body length');
        this.instructions[index] = instructionRecord(opcode.forEach, NO_INDEX, bodyLength);
      } else if (current.kind === 'let') {
        if (bindings.has(current.name)) throw new TypeError(`Variable already defined: ${current.name}`);
        bindings.set(current.name, this.compileExpression(current.value, inLoop, bindings));
      } else if (current.kind === 'require') {
        const condition = this.compileExpression(current.condition, inLoop, bindings);
        requireType(condition, 'bool', 'require condition');
        this.instructions.push(instructionRecord(opcode.require, NO_INDEX, condition.register));
      } else {
        this.compileInvoke(current, inLoop, bindings);
      }
    }
  }

  compileInvoke(current: Extract<Step, { kind: 'invoke' }>, inLoop: boolean, bindings: Bindings): void {
    const programAccount = this.encodeAccountReference(current.program, inLoop);
    const programConstraint = this.constraintFor(current.program, inLoop);
    if (!programConstraint.executable) throw new TypeError('Invoke program account must require executable=true');

    const accountStart = this.cpiAccounts.length;
    for (const account of current.accounts) {
      const reference = this.encodeAccountReference(account.account, inLoop);
      const constraint = this.constraintFor(account.account, inLoop);
      if (account.signer && !constraint.signer) throw new TypeError('CPI signer is not required by its account schema');
      if (account.writable && !constraint.writable) throw new TypeError('CPI writable account is not writable in its schema');
      this.cpiAccounts.push(Uint8Array.of(reference, (account.signer ? ACCOUNT_SIGNER : 0) | (account.writable ? ACCOUNT_WRITABLE : 0)));
    }

    const segmentStart = this.dataSegments.length;
    let maxDataLength = 0;
    for (const part of current.data) {
      const result = this.compileDataPart(part, inLoop, bindings);
      this.dataSegments.push(result.record);
      maxDataLength += result.maxLength;
    }
    if (maxDataLength > MAX_CPI_DATA_LENGTH) throw new RangeError('CPI data can exceed 4096 bytes');
    this.maxCpiDataLength = Math.max(this.maxCpiDataLength, maxDataLength);

    const descriptor = new Writer();
    descriptor.u8(programAccount);
    descriptor.u8(0);
    descriptor.u16(accountStart);
    descriptor.u8(current.accounts.length);
    descriptor.u8(current.data.length);
    descriptor.u16(segmentStart);
    descriptor.u16(maxDataLength);
    descriptor.raw([0, 0]);
    const cpiIndex = this.cpis.length;
    this.cpis.push(descriptor.finish());

    let guard = NO_INDEX;
    if (current.when) {
      const result = this.compileExpression(current.when, inLoop, bindings);
      requireType(result, 'bool', 'invoke guard');
      guard = result.register;
    }
    this.instructions.push(instructionRecord(opcode.invoke, NO_INDEX, cpiIndex, guard));
  }

  compileDataPart(part: DataPart, inLoop: boolean, bindings: Bindings): { record: Uint8Array; maxLength: number } {
    const writer = new Writer();
    if (part.kind === 'literal') {
      const offset = this.addBlob(part.bytes);
      writer.u8(dataKind.literal);
      writer.u8(NO_INDEX);
      writer.u16(offset);
      writer.u16(part.bytes.length);
      writer.raw([0, 0]);
      return { record: writer.finish(), maxLength: part.bytes.length };
    }

    const value = this.compileExpression(part.value, inLoop, bindings);
    const expected: Record<typeof part.encoding, ValueType | 'unsigned'> = {
      u8: 'unsigned',
      u16: 'unsigned',
      u32: 'unsigned',
      u64: 'unsigned',
      i64: 'i64',
      u128: 'u128',
      pubkey: 'pubkey',
      bool: 'bool',
      bytes: 'bytes',
    };
    if (expected[part.encoding] === 'unsigned') {
      if (value.type !== 'u64' && value.type !== 'u128') throw new TypeError(`${part.encoding} encoding requires u64 or u128`);
    } else {
      requireType(value, expected[part.encoding] as ValueType, `${part.encoding} encoding`);
    }
    const maxLength = part.encoding === 'bytes' ? value.maxLength : { u8: 1, u16: 2, u32: 4, u64: 8, i64: 8, u128: 16, pubkey: 32, bool: 1, bytes: 0 }[part.encoding];
    writer.u8(dataKind[part.encoding]);
    writer.u8(value.register);
    writer.u16(0);
    writer.u16(0);
    writer.raw([0, 0]);
    return { record: writer.finish(), maxLength };
  }

  compileExpression(current: Expression, inLoop: boolean, bindings: Bindings): ExpressionResult {
    if (current.kind === 'input') {
      const index = this.inputIndices.get(current.name);
      if (index === undefined) throw new TypeError(`Unknown input: ${current.name}`);
      const definition = this.inputEntries[index]![1];
      return this.emit(opcode.loadInput, definition.type, definition.type === 'bytes' ? definition.maxLength : 0, index);
    }
    if (current.kind === 'variable') {
      const value = bindings.get(current.name);
      if (value === undefined) throw new TypeError(`Unknown variable: ${current.name}`);
      return value;
    }
    if (current.kind === 'literal') {
      switch (current.value.type) {
        case 'bool':
          return this.emit(opcode.constBool, 'bool', 0, current.value.value ? 1 : 0);
        case 'u64':
          return this.emit(opcode.constU64, 'u64', 0, NO_INDEX, NO_INDEX, NO_INDEX, current.value.value);
        case 'i64':
          return this.emit(opcode.constI64, 'i64', 0, NO_INDEX, NO_INDEX, NO_INDEX, current.value.value);
        case 'u128': {
          const encoded = encodeBigint(current.value.value, 16);
          const offset = this.addBlob(encoded);
          return this.emit(opcode.constU128, 'u128', 0, NO_INDEX, NO_INDEX, NO_INDEX, blobImmediate(offset, 16));
        }
        case 'pubkey':
          return this.emit(opcode.constPubkey, 'pubkey', 0, this.addPubkey(current.value.value));
        case 'bytes': {
          const offset = this.addBlob(current.value.value);
          return this.emit(opcode.constBytes, 'bytes', current.value.value.length, NO_INDEX, NO_INDEX, NO_INDEX, blobImmediate(offset, current.value.value.length));
        }
      }
    }
    if (current.kind === 'accountField') {
      const accountReference = this.encodeAccountReference(current.account, inLoop);
      const fields = {
        key: [opcode.accountKey, 'pubkey'],
        owner: [opcode.accountOwner, 'pubkey'],
        lamports: [opcode.accountLamports, 'u64'],
        dataLength: [opcode.accountDataLength, 'u64'],
        isEmpty: [opcode.accountIsEmpty, 'bool'],
      } as const;
      const [operation, type] = fields[current.field];
      return this.emit(operation, type, 0, accountReference);
    }
    if (current.kind === 'accountData') {
      const accountReference = this.encodeAccountReference(current.account, inLoop);
      const reads = {
        bool: [opcode.readBool, 'bool'],
        u8: [opcode.readU8, 'u64'],
        u16: [opcode.readU16, 'u64'],
        u32: [opcode.readU32, 'u64'],
        u64: [opcode.readU64, 'u64'],
        i64: [opcode.readI64, 'i64'],
        u128: [opcode.readU128, 'u128'],
        pubkey: [opcode.readPubkey, 'pubkey'],
      } as const;
      const [operation, type] = reads[current.type];
      return this.emit(operation, type, 0, accountReference, NO_INDEX, NO_INDEX, BigInt(current.offset));
    }
    if (current.kind === 'clock') {
      return current.field === 'slot'
        ? this.emit(opcode.clockSlot, 'u64')
        : this.emit(opcode.clockTimestamp, 'i64');
    }
    if (current.kind === 'loopIndex') {
      if (!inLoop) throw new TypeError('loopIndex is only valid inside forEach');
      return this.emit(opcode.loopIndex, 'u64');
    }
    if (current.kind === 'pda') {
      const programAccount = this.encodeAccountReference(current.program, inLoop);
      const programConstraint = this.constraintFor(current.program, inLoop);
      if (!programConstraint.executable) throw new TypeError('PDA program account must require executable=true');
      if (current.seeds.length < 1 || current.seeds.length > MAX_PDA_SEEDS) {
        throw new RangeError(`PDA derivation requires 1 to ${MAX_PDA_SEEDS} seeds`);
      }
      const segmentStart = this.dataSegments.length;
      for (const seed of current.seeds) {
        const value = this.compileExpression(seed, inLoop, bindings);
        const seedLength = value.type === 'bytes' ? value.maxLength : fixedValueLength(value.type);
        if (seedLength > MAX_PDA_SEED_LENGTH) {
          throw new RangeError(`PDA seed can exceed ${MAX_PDA_SEED_LENGTH} bytes`);
        }
        this.dataSegments.push(this.compileSeedSegment(value));
      }
      return this.emit(
        opcode.derivePda,
        'pubkey',
        0,
        programAccount,
        NO_INDEX,
        NO_INDEX,
        rangeImmediate(segmentStart, current.seeds.length),
      );
    }
    if (current.kind === 'not') {
      const value = this.compileExpression(current.value, inLoop, bindings);
      requireType(value, 'bool', 'not');
      return this.emit(opcode.not, 'bool', 0, value.register);
    }
    if (current.kind === 'cast') {
      const value = this.compileExpression(current.value, inLoop, bindings);
      if (!isNumeric(value.type)) throw new TypeError('cast requires a numeric expression');
      const operation = { u64: opcode.castU64, i64: opcode.castI64, u128: opcode.castU128 }[current.to];
      return this.emit(operation, current.to, 0, value.register);
    }
    if (current.kind === 'select') {
      const condition = this.compileExpression(current.condition, inLoop, bindings);
      const ifTrue = this.compileExpression(current.ifTrue, inLoop, bindings);
      const ifFalse = this.compileExpression(current.ifFalse, inLoop, bindings);
      requireType(condition, 'bool', 'select condition');
      requireType(ifFalse, ifTrue.type, 'select branches');
      return this.emit(opcode.select, ifTrue.type, Math.max(ifTrue.maxLength, ifFalse.maxLength), condition.register, ifTrue.register, ifFalse.register);
    }

    const left = this.compileExpression(current.left, inLoop, bindings);
    const right = this.compileExpression(current.right, inLoop, bindings);
    const operation = opcode[current.op];
    if (['add', 'subtract', 'multiply', 'divide', 'min', 'max'].includes(current.op)) {
      if (left.type !== right.type || !isNumeric(left.type)) throw new TypeError(`${current.op} requires matching numeric types`);
      return this.emit(operation, left.type, 0, left.register, right.register);
    }
    if (current.op === 'and' || current.op === 'or') {
      requireType(left, 'bool', current.op);
      requireType(right, 'bool', current.op);
      return this.emit(operation, 'bool', 0, left.register, right.register);
    }
    if (left.type !== right.type) throw new TypeError(`${current.op} requires matching types`);
    if (!['equal', 'notEqual'].includes(current.op) && !isNumeric(left.type)) {
      throw new TypeError(`${current.op} requires numeric operands`);
    }
    return this.emit(operation, 'bool', 0, left.register, right.register);
  }

  compileSeedSegment(value: ExpressionResult): Uint8Array {
    const kind: Record<ValueType, number> = {
      bool: dataKind.bool,
      u64: dataKind.u64,
      i64: dataKind.i64,
      u128: dataKind.u128,
      pubkey: dataKind.pubkey,
      bytes: dataKind.bytes,
    };
    const writer = new Writer();
    writer.u8(kind[value.type]);
    writer.u8(value.register);
    writer.u16(0);
    writer.u16(0);
    writer.raw([0, 0]);
    return writer.finish();
  }

  emit(
    operation: number,
    type: ValueType,
    maxLength = 0,
    a = NO_INDEX,
    b = NO_INDEX,
    c = NO_INDEX,
    immediate = 0n,
  ): ExpressionResult {
    const register = this.nextRegister;
    this.nextRegister += 1;
    if (register >= MAX_REGISTERS) throw new RangeError('Template uses more than 64 registers');
    this.instructions.push(instructionRecord(operation, register, a, b, c, immediate));
    return { register, type, maxLength };
  }

  encodeAccountReference(reference: AccountReference, inLoop: boolean): number {
    if (reference.kind === 'account') {
      const index = this.fixedIndices.get(reference.name);
      if (index === undefined) throw new TypeError(`Unknown fixed account: ${reference.name}`);
      return index;
    }
    if (!inLoop) throw new TypeError('Iteration accounts are only valid inside forEach');
    const index = this.batchIndices.get(reference.name);
    if (index === undefined) throw new TypeError(`Unknown batch account: ${reference.name}`);
    return ITERATION_ACCOUNT_BIT | index;
  }

  constraintFor(reference: AccountReference, inLoop: boolean): AccountConstraint {
    if (reference.kind === 'account') {
      const index = this.fixedIndices.get(reference.name);
      if (index === undefined) throw new TypeError(`Unknown fixed account: ${reference.name}`);
      return this.fixedEntries[index]![1];
    }
    if (!inLoop) throw new TypeError('Iteration accounts are only valid inside forEach');
    const index = this.batchIndices.get(reference.name);
    if (index === undefined) throw new TypeError(`Unknown batch account: ${reference.name}`);
    return this.batchEntries[index]![1];
  }

  addPubkey(value: Uint8Array): number {
    if (value.length !== 32) throw new RangeError('Pubkeys must contain 32 bytes');
    const key = toHex(value);
    const existing = this.pubkeyIndices.get(key);
    if (existing !== undefined) return existing;
    const index = this.pubkeys.length;
    if (index >= NO_INDEX) throw new RangeError('Template contains too many pubkey constants');
    this.pubkeys.push(value.slice());
    this.pubkeyIndices.set(key, index);
    return index;
  }

  addBlob(value: Uint8Array): number {
    const offset = this.blob.length;
    this.blob.push(...value);
    if (this.blob.length > 0xffff) throw new RangeError('Template blob exceeds 65535 bytes');
    return offset;
  }
}

export function compileTemplate(input: TemplateInput | Template): CompiledTemplate {
  return new Compiler(TemplateSchema.parse(input)).compile();
}

function instructionRecord(
  operation: number,
  dst: number,
  a = NO_INDEX,
  b = NO_INDEX,
  c = NO_INDEX,
  immediate = 0n,
): Uint8Array {
  const writer = new Writer();
  writer.u8(operation);
  writer.u8(dst);
  writer.u8(a);
  writer.u8(b);
  writer.u8(c);
  writer.u8(0);
  writer.bigint(immediate, 8);
  writer.raw([0, 0]);
  return writer.finish();
}

function requireType(value: ExpressionResult, expected: ValueType, context: string): void {
  if (value.type !== expected) throw new TypeError(`${context} requires ${expected}; received ${value.type}`);
}

function isNumeric(type: ValueType): boolean {
  return type === 'u64' || type === 'i64' || type === 'u128';
}

function fixedValueLength(type: Exclude<ValueType, 'bytes'>): number {
  return { bool: 1, u64: 8, i64: 8, u128: 16, pubkey: 32 }[type];
}

function rangeImmediate(offset: number, length: number): bigint {
  return BigInt(offset) | (BigInt(length) << 32n);
}

const blobImmediate = rangeImmediate;

function encodeBigint(value: bigint, byteLength: number): Uint8Array {
  const writer = new Writer();
  writer.bigint(value, byteLength);
  return writer.finish();
}

function countCpis(steps: Step[]): number {
  return steps.reduce((total, current) => {
    if (current.kind === 'invoke') return total + 1;
    if (current.kind === 'forEach') return total + countCpis(current.steps);
    return total;
  }, 0);
}

function toHex(value: Uint8Array): string {
  return [...value].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}
