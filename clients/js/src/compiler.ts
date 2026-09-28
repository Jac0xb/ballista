import { sha256 } from '@noble/hashes/sha2.js';

import { INSTRUCTIONS_SYSVAR_ADDRESS_BYTES } from './helpers.js';
import {
  TemplateSchema,
  readWidth,
  registrySize,
  type RegistryFieldType,
  type AccountConstraint,
  type AccountReference,
  type DataPart,
  type Expression,
  type InputDefinition,
  type Literal,
  type ReadType,
  type Step,
  type Template,
  type TemplateInput,
  type ValueType,
} from './schema.js';

export const TEMPLATE_PROGRAM_VERSION = 1;
export const MAX_TEMPLATE_PAYLOAD_LENGTH = 10_240;
export const MAX_RUNTIME_ACCOUNTS = 120;
export const MAX_ROW_INPUTS = 8;
export const MAX_INPUT_VALUES = 256;
export const MAX_ACCOUNT_GROUPS = 8;
export const MAX_INPUT_BYTES = 1_024;
export const MAX_REGISTERS = 64;
export const MAX_VM_INSTRUCTIONS = 128;
export const MAX_EXPANDED_CPIS = 64;
export const MAX_CPI_DATA_LENGTH = 4_096;
export const MAX_CPI_ACCOUNTS = 64;
export const MAX_PDA_SEEDS = 15;
export const MAX_PDA_SEED_LENGTH = 32;
export const MAX_RETURN_DATA_LENGTH = 1_024;
export const MAX_REGISTRIES = 8;
export const MAX_REGISTRY_OPENS = 8;
export const MAX_REGISTRY_SIZE = 512;
/** An entry's header before its fields: magic, version, registry index, template and key. */
export const REGISTRY_ENTRY_HEADER_LENGTH = 72;
/** The CPIs an open makes at most: a transfer, an allocate and an assign for a pre-funded entry. */
export const REGISTRY_OPEN_CPIS = 3;

/** Program header flag: emit a `BEV1` data log after every successful run. */
export const PROGRAM_FLAG_EMIT_EVENT = 1;
/** The run event's tag family: the first three bytes of `BEV1`, kept by every version. No emit may start with them. */
export const RUN_EVENT_TAG_FAMILY = Uint8Array.of(0x42, 0x45, 0x56);
/** The shortest literal tag an emit may start with. */
export const MIN_EMIT_TAG_LENGTH = 4;
/** Instruction flag on read opcodes: the offset comes from register `b`. */
export const INSTRUCTION_FLAG_DYNAMIC_OFFSET = 1;

const NO_INDEX = 0xff;
const ITERATION_ACCOUNT_BIT = 0x80;

const ACCOUNT_SIGNER = 1 << 0;
const ACCOUNT_WRITABLE = 1 << 1;
const ACCOUNT_EXECUTABLE = 1 << 2;

const valueTypeCode = { bool: 1, u64: 2, i64: 3, u128: 4, pubkey: 5, bytes: 6 } as const;

export const opcode = {
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
  returnData: 48,
  move: 49,
  createPda: 50,
  mulDiv: 51,
  mulDivCeil: 52,
  remainder: 53,
  shiftLeft: 54,
  shiftRight: 55,
  bitAnd: 56,
  bitOr: 57,
  bitXor: 58,
  powerOfTen: 59,
  readI32: 60,
  repeat: 61,
  emit: 62,
  setReturnData: 63,
  instructionCount: 64,
  instructionIndex: 65,
  instructionProgram: 66,
  instructionAccountCount: 67,
  instructionAccount: 68,
  instructionAccountFlags: 69,
  instructionDataLength: 70,
  readInstructionData: 71,
  readInstructionBytes: 72,
  readAccountBytes: 73,
  bytesLength: 74,
  openRegistry: 75,
  readRegistry: 76,
  writeRegistry: 77,
} as const;

/** The conjuncts of a requirement: `and(and(a, b), c)` is three separate assertions. */
function flattenConjunction(condition: Expression, into: Expression[] = []): Expression[] {
  if (condition.kind === 'binary' && condition.op === 'and') {
    flattenConjunction(condition.left, into);
    flattenConjunction(condition.right, into);
    return into;
  }
  into.push(condition);
  return into;
}

/** A constant's identity, so two uses of the same value share one register. */
function literalKey(value: Literal): string {
  const inner: unknown = value.value;
  if (inner instanceof Uint8Array) {
    return `${value.type}:${[...inner].map((byte) => byte.toString(16).padStart(2, '0')).join('')}`;
  }
  return `${value.type}:${String(inner)}`;
}

/**
 * The typed value of a literal expression in the plain object tree, or `undefined` for any other
 * node. `data.literal(bytes)` is a CPI data part, not an expression, and has no typed value.
 */
function typedLiteral(record: Record<string, unknown>): Literal | undefined {
  const inner = record.value as Literal | undefined;
  return record.kind === 'literal' && inner !== undefined && typeof inner === 'object' && 'type' in inner
    ? inner
    : undefined;
}

/** Every literal expression in the steps, in the order they appear, deduplicated by value. */
function collectLiterals(value: unknown, into = new Map<string, Literal>()): Map<string, Literal> {
  if (Array.isArray(value)) {
    for (const item of value) collectLiterals(item, into);
    return into;
  }
  if (value === null || typeof value !== 'object') return into;
  const record = value as Record<string, unknown>;
  const literal = typedLiteral(record);
  if (literal !== undefined) {
    const key = literalKey(literal);
    if (!into.has(key)) into.set(key, literal);
  }
  for (const item of Object.values(record)) collectLiterals(item, into);
  return into;
}

/**
 * How often each literal (keyed like `literalKey`) and each fixed input (as `input:<name>`)
 * appears in the steps. `sharesRegister` reads it.
 */
function countUses(value: unknown, into = new Map<string, number>()): Map<string, number> {
  if (Array.isArray(value)) {
    for (const item of value) countUses(item, into);
    return into;
  }
  if (value === null || typeof value !== 'object') return into;
  const record = value as Record<string, unknown>;
  const literal = typedLiteral(record);
  let key: string | undefined;
  if (literal !== undefined) {
    key = literalKey(literal);
  } else if (record.kind === 'input' && typeof record.name === 'string') {
    key = `input:${record.name}`;
  }
  if (key !== undefined) into.set(key, (into.get(key) ?? 0) + 1);
  for (const item of Object.values(record)) countUses(item, into);
  return into;
}

/** Every fixed input the steps refer to, found by walking the plain object tree. */
function collectInputNames(value: unknown, into: Set<string>): void {
  if (Array.isArray(value)) {
    for (const item of value) collectInputNames(item, into);
    return;
  }
  if (value === null || typeof value !== 'object') return;
  const record = value as Record<string, unknown>;
  if (record.kind === 'input' && typeof record.name === 'string') into.add(record.name);
  for (const item of Object.values(record)) collectInputNames(item, into);
}

const readOpcode: Record<ReadType, number> = {
  bool: opcode.readBool,
  u8: opcode.readU8,
  u16: opcode.readU16,
  u32: opcode.readU32,
  i32: opcode.readI32,
  u64: opcode.readU64,
  i64: opcode.readI64,
  u128: opcode.readU128,
  pubkey: opcode.readPubkey,
};

const readResultType: Record<ReadType, ValueType> = {
  bool: 'bool',
  u8: 'u64',
  u16: 'u64',
  u32: 'u64',
  i32: 'i64',
  u64: 'u64',
  i64: 'i64',
  u128: 'u128',
  pubkey: 'pubkey',
};

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

/**
 * The loop a step sits in: `rows` for a `forEach` body, which has row accounts, row inputs and a
 * loop index, and `count` for a `repeat` body, which has only the index. `undefined` at the root.
 */
type LoopKind = 'rows' | 'count';

/** Maps one emitted VM instruction back to the authoring step that produced it. */
export interface SourceMapEntry {
  /** Instruction index, which is also the program counter reported in run errors. */
  pc: number;
  /** Step path such as `steps[2]` or `steps[1].steps[0]`. */
  path: string;
  label?: string;
}

export interface CompileStats {
  payloadBytes: number;
  fixedAccounts: number;
  batchStride: number;
  batchMaxIterations: number;
  batchMinIterations: number;
  inputs: number;
  registers: number;
  instructions: number;
  cpis: number;
  maxExpandedCpis: number;
  maxCpiDataLength: number;
  emitEvent: boolean;
  rowInputs: number;
  accountGroups: number;
}

export interface CompiledTemplate {
  template: Template;
  bytes: Uint8Array;
  hash: Uint8Array;
  inputOrder: readonly string[];
  /** Row inputs in declaration order; each run row supplies these. */
  rowInputOrder: readonly string[];
  fixedAccountOrder: readonly string[];
  batchAccountOrder: readonly string[];
  /** Account groups in declaration order; the run data prefix and metas follow it. */
  accountGroupOrder: readonly string[];
  stats: CompileStats;
  sourceMap: readonly SourceMapEntry[];
}

class Compiler {
  readonly template: Template;
  readonly inputEntries: [string, InputDefinition][];
  readonly rowInputEntries: [string, InputDefinition][];
  readonly fixedEntries: [string, AccountConstraint][];
  readonly batchEntries: [string, AccountConstraint][];
  readonly inputIndices = new Map<string, number>();
  readonly rowInputIndices = new Map<string, number>();
  /// The register each referenced fixed input was loaded into, before the first step.
  readonly fixedInputs = new Map<string, ExpressionResult>();
  /// The register each distinct constant was materialized into, likewise before the first step.
  readonly constants = new Map<string, ExpressionResult>();
  readonly fixedIndices = new Map<string, number>();
  readonly batchIndices = new Map<string, number>();
  readonly accountGroupIndices = new Map<string, number>();
  readonly pubkeys: Uint8Array[] = [];
  readonly pubkeyIndices = new Map<string, number>();
  readonly blob: number[] = [];
  readonly accountRecords: Uint8Array[] = [];
  readonly inputRecords: Uint8Array[] = [];
  readonly instructions: Uint8Array[] = [];
  readonly cpis: Uint8Array[] = [];
  readonly cpiAccounts: Uint8Array[] = [];
  readonly dataSegments: Uint8Array[] = [];
  /** Highest byte any fixed-offset read touches per account, used to infer `minDataLength`. */
  readonly requiredDataLength = new Map<string, number>();
  readonly sourceMap: SourceMapEntry[] = [];
  /** Each registry's index, size and fields, in declaration order. */
  readonly registries = new Map<string, RegistryLayout>();
  /** How often each literal and fixed input appears; see `countUses`. */
  readonly uses: Map<string, number>;
  location: { path: string; label?: string } = { path: 'template' };
  nextRegister = 0;
  maxCpiDataLength = 0;
  /** Whether a `setReturnData` step has compiled; no invoke may follow it. */
  returnDataSet = false;

  constructor(template: Template) {
    this.template = template;
    this.inputEntries = Object.entries(template.inputs);
    this.rowInputEntries = Object.entries(template.batch?.rowInputs ?? {});
    this.fixedEntries = Object.entries(template.accounts);
    this.batchEntries = Object.entries(template.batch?.row ?? {});
    this.inputEntries.forEach(([name], index) => this.inputIndices.set(name, index));
    this.rowInputEntries.forEach(([name], index) => this.rowInputIndices.set(name, index));
    this.fixedEntries.forEach(([name], index) => this.fixedIndices.set(name, index));
    this.batchEntries.forEach(([name], index) => this.batchIndices.set(name, index));
    template.accountGroups.forEach((name, index) => this.accountGroupIndices.set(name, index));
    this.uses = countUses(template.steps);
    for (const [index, [name, fields]] of Object.entries(template.registries).entries()) {
      let offset = 0;
      const layout: RegistryLayout = { index, size: registrySize(fields), fields: new Map() };
      for (const [field, type] of Object.entries(fields)) {
        layout.fields.set(field, { offset, type });
        offset += readWidth[type];
      }
      this.registries.set(name, layout);
    }
  }

  compile(): CompiledTemplate {
    // Fixed inputs load once, before any step. Their value cannot change during a run, so a load
    // inside a batch body would repeat the same work on every row — and, because the loop
    // restores every register the body writes, it would also stop the executor from reusing the
    // invocation data it built for the previous row.
    const used = new Set<string>();
    collectInputNames(this.template.steps, used);
    for (const [name, definition] of this.inputEntries) {
      if (!used.has(name)) continue;
      const index = this.inputIndices.get(name)!;
      this.location = { path: `inputs.${name}` };
      this.fixedInputs.set(
        name,
        this.emit(opcode.loadInput, definition.type, definition.type === 'bytes' ? definition.maxLength : 0, index),
      );
    }
    // Constants are hoisted for the same reasons, and shared between uses: a literal inside a
    // batch body would otherwise be rebuilt on every row.
    let constantIndex = 0;
    for (const [key, value] of collectLiterals(this.template.steps)) {
      if (this.constants.has(key)) continue;
      this.location = { path: `constants[${constantIndex}]` };
      constantIndex += 1;
      this.constants.set(key, this.emitLiteral(value));
    }
    this.compileRegistryOpens();
    this.location = { path: 'template' };
    // Steps compile first so every static read has already raised its account's data floor.
    this.compileSteps(this.template.steps, undefined, new Map(), new Set(), 'steps');
    for (const [name, constraint] of this.fixedEntries) {
      this.accountRecords.push(this.compileAccountConstraint(constraint, this.requiredDataLength.get(fixedKey(name)) ?? 0));
    }
    for (const [name, constraint] of this.batchEntries) {
      this.accountRecords.push(this.compileAccountConstraint(constraint, this.requiredDataLength.get(rowKey(name)) ?? 0));
    }
    for (const [, input] of this.inputEntries) this.inputRecords.push(this.compileInput(input));
    for (const [, input] of this.rowInputEntries) this.inputRecords.push(this.compileInput(input));

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

    const opens = this.fixedEntries.filter(([, constraint]) => constraint.registry !== undefined).length;
    const maxExpandedCpis =
      worstCaseCpis(this.template.steps, this.template.batch?.maxIterations ?? 0) + REGISTRY_OPEN_CPIS * opens;
    if (maxExpandedCpis > MAX_EXPANDED_CPIS) {
      throw new RangeError(`Template can expand to ${maxExpandedCpis} CPIs; maximum is 64`);
    }

    const header = new Writer();
    header.raw([0x42, 0x56, 0x4d, 0x31]);
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
    header.u8(this.template.emitEvent ? PROGRAM_FLAG_EMIT_EVENT : 0);
    header.u16(this.blob.length);
    header.u8(this.template.batch?.minIterations ?? 0);
    header.u8(this.rowInputEntries.length);
    header.u8(this.template.accountGroups.length);
    header.raw([0]);

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
      rowInputOrder: this.rowInputEntries.map(([name]) => name),
      fixedAccountOrder: this.fixedEntries.map(([name]) => name),
      batchAccountOrder: this.batchEntries.map(([name]) => name),
      accountGroupOrder: [...this.template.accountGroups],
      stats: {
        payloadBytes: bytes.length,
        fixedAccounts: this.fixedEntries.length,
        batchStride: this.batchEntries.length,
        batchMaxIterations: this.template.batch?.maxIterations ?? 0,
        batchMinIterations: this.template.batch?.minIterations ?? 0,
        inputs: this.inputEntries.length,
        registers: this.nextRegister,
        instructions: this.instructions.length,
        cpis: this.cpis.length,
        maxExpandedCpis,
        maxCpiDataLength: this.maxCpiDataLength,
        emitEvent: this.template.emitEvent,
        rowInputs: this.rowInputEntries.length,
        accountGroups: this.template.accountGroups.length,
      },
      sourceMap: this.sourceMap,
    };
  }

  compileAccountConstraint(constraint: AccountConstraint, inferredMinDataLength: number): Uint8Array {
    const writer = new Writer();
    writer.u8(
      (constraint.signer ? ACCOUNT_SIGNER : 0) |
        (constraint.writable ? ACCOUNT_WRITABLE : 0) |
        (constraint.executable ? ACCOUNT_EXECUTABLE : 0),
    );
    writer.u8(constraint.address ? this.addPubkey(constraint.address) : NO_INDEX);
    writer.u8(constraint.owner ? this.addPubkey(constraint.owner) : NO_INDEX);
    writer.u8(0);
    writer.u32(Math.max(constraint.minDataLength, inferredMinDataLength));
    return writer.finish();
  }

  compileInput(input: InputDefinition): Uint8Array {
    const writer = new Writer();
    writer.u8(valueTypeCode[input.type]);
    writer.u8(0);
    writer.u16(input.type === 'bytes' ? input.maxLength : 0);
    return writer.finish();
  }

  /**
   * One `OPEN_REGISTRY` per registry account, in declaration order, after the hoisted inputs and
   * constants and before the first step: each key can be an input, an account's key or a constant,
   * and every open precedes any `setReturnData`, as the verifier requires.
   */
  compileRegistryOpens(): void {
    const entries = this.fixedEntries.filter(([, constraint]) => constraint.registry !== undefined);
    if (entries.length === 0) return;
    if (entries.length > MAX_REGISTRY_OPENS) {
      throw new RangeError(`A template opens at most ${MAX_REGISTRY_OPENS} registry entries`);
    }
    const systemProgram = this.fixedEntries.findIndex(
      ([, constraint]) => constraint.address !== undefined && constraint.address.every((byte) => byte === 0),
    );
    if (systemProgram < 0) {
      throw new TypeError(
        'Registry accounts need a fixed account pinned to the System program: declare one with account.systemProgram()',
      );
    }
    for (const [name, constraint] of entries) {
      const registry = constraint.registry!;
      this.location = { path: `accounts.${name}` };
      const layout = this.registries.get(registry.name);
      if (layout === undefined) throw new TypeError(`Unknown registry: ${registry.name}`);
      if (constraint.signer || constraint.executable || constraint.address || constraint.owner || constraint.minDataLength !== 0) {
        throw new TypeError(`${name} must be declared only writable: build it with account.registry`);
      }
      const payer = this.template.accounts[registry.payer];
      if (payer === undefined || !payer.signer || !payer.writable || payer.registry !== undefined) {
        throw new TypeError(`${name}'s payer ${registry.payer} must be a fixed account declared signer and writable`);
      }
      let key = NO_INDEX;
      if (registry.key !== undefined) {
        const value = this.compileExpression(registry.key, undefined, new Map());
        requireType(value, 'pubkey', `${name}'s key`);
        key = value.register;
      }
      const immediate = BigInt(layout.index) | (BigInt(layout.size) << 8n) | (BigInt(systemProgram) << 24n);
      this.pushInstruction(
        instructionRecord(opcode.openRegistry, NO_INDEX, this.fixedIndices.get(name)!, key, this.fixedIndices.get(registry.payer)!, immediate),
      );
    }
  }

  /** The field `field` of the registry entry in fixed account `accountName`. */
  registryField(accountName: string, field: string): RegistryField {
    const registry = this.template.accounts[accountName]?.registry;
    if (registry === undefined) throw new TypeError(`${accountName} is not a registry account`);
    const found = this.registries.get(registry.name)?.fields.get(field);
    if (found === undefined) throw new TypeError(`${registry.name} has no field ${field}`);
    return found;
  }

  compileSteps(steps: Step[], loop: LoopKind | undefined, bindings: Bindings, carried: Set<string>, path: string): void {
    let previous: Step | undefined;
    for (const [index, current] of steps.entries()) {
      const stepPath = `${path}[${index}]`;
      this.location = { path: stepPath, ...(current.label ? { label: current.label } : {}) };
      if (current.kind === 'forEach' || current.kind === 'repeat') {
        if (loop) throw new TypeError('Nested loops are not supported');
        let carry = 0n;
        const carriedNames = new Set<string>();
        for (const name of current.carry ?? []) {
          let binding = bindings.get(name);
          if (!binding) throw new TypeError(`Carried variable must be defined before the loop: ${name}`);
          // `assign` rewrites a carried variable's register on every pass, so nothing else may
          // read it. A `let` of a constant, an input or another variable shares that value's
          // register: copy it into one of the variable's own first.
          if (this.sharesRegister(name, binding.register, bindings)) {
            binding = this.emit(opcode.move, binding.type, binding.maxLength, binding.register);
            bindings.set(name, binding);
          }
          carriedNames.add(name);
          carry |= 1n << BigInt(binding.register);
        }
        // A REPEAT reads its count once, as the loop starts, so the count compiles at the root.
        // FOREACH leaves both operands unset.
        let count = NO_INDEX;
        let max = NO_INDEX;
        if (current.kind === 'repeat') {
          const value = this.compileExpression(current.count, undefined, bindings);
          requireType(value, 'u64', 'repeat count');
          count = value.register;
          max = current.max;
        }
        const operation = current.kind === 'repeat' ? opcode.repeat : opcode.forEach;
        const loopPc = this.pushInstruction(instructionRecord(operation, NO_INDEX, 0, count, max, carry));
        const bodyStart = this.instructions.length;
        const kind = current.kind === 'repeat' ? 'count' : 'rows';
        this.compileSteps(current.steps, kind, new Map(bindings), carriedNames, `${stepPath}.steps`);
        const bodyLength = this.instructions.length - bodyStart;
        if (bodyLength === 0 || bodyLength > 0xff) throw new RangeError(`Invalid ${current.kind} body length`);
        this.instructions[loopPc] = instructionRecord(operation, NO_INDEX, bodyLength, count, max, carry);
      } else if (current.kind === 'let') {
        if (bindings.has(current.name)) throw new TypeError(`Variable already defined: ${current.name}`);
        const value =
          current.value.kind === 'returnData'
            ? this.compileReturnData(current.value, previous)
            : this.compileExpression(current.value, loop, bindings);
        bindings.set(current.name, value);
      } else if (current.kind === 'assign') {
        if (!loop) throw new TypeError('assign is only valid inside a loop');
        const binding = bindings.get(current.name);
        if (!binding || !carried.has(current.name)) {
          throw new TypeError(`assign target must be listed in the loop's carry: ${current.name}`);
        }
        const value = this.compileExpression(current.value, loop, bindings);
        if (value.type !== binding.type || (value.type === 'bytes' && value.maxLength !== binding.maxLength)) {
          throw new TypeError(`assign to ${current.name} must keep its ${binding.type} type and size`);
        }
        this.pushInstruction(instructionRecord(opcode.move, binding.register, value.register));
      } else if (current.kind === 'require') {
        // `require(and(a, b))` is `require(a); require(b)`, which drops the `and` instruction
        // and the register it wrote. Nested ands flatten the same way. The failure is still one
        // error: whichever conjunct is false stops the run.
        for (const conjunct of flattenConjunction(current.condition)) {
          const condition = this.compileExpression(conjunct, loop, bindings);
          requireType(condition, 'bool', 'require condition');
          this.pushInstruction(instructionRecord(opcode.require, NO_INDEX, condition.register));
        }
      } else if (current.kind === 'emit' || current.kind === 'setReturnData') {
        this.compileOutput(current, loop, bindings);
      } else if (current.kind === 'setRegistry') {
        const field = this.registryField(current.account, current.field);
        const value = this.compileExpression(current.value, loop, bindings);
        if (value.type !== field.type) {
          throw new TypeError(`${current.account}.${current.field}: ${current.field} is a ${field.type}, not a ${value.type}`);
        }
        this.pushInstruction(
          instructionRecord(
            opcode.writeRegistry,
            NO_INDEX,
            value.register,
            this.fixedIndices.get(current.account)!,
            NO_INDEX,
            registryFieldImmediate(field),
          ),
        );
      } else {
        this.compileInvoke(current, loop, bindings);
      }
      this.location = { path: stepPath, ...(current.label ? { label: current.label } : {}) };
      previous = current;
    }
  }

  /**
   * Whether anything besides variable `name` reads `register`: another variable, or a hoisted
   * constant or fixed input that appears more than once in the template.
   */
  sharesRegister(name: string, register: number, bindings: Bindings): boolean {
    for (const [other, value] of bindings) {
      if (other !== name && value.register === register) return true;
    }
    for (const [key, value] of this.constants) {
      if (value.register === register) return (this.uses.get(key) ?? 0) > 1;
    }
    for (const [input, value] of this.fixedInputs) {
      if (value.register === register) return (this.uses.get(`input:${input}`) ?? 0) > 1;
    }
    return false;
  }

  compileInvoke(current: Extract<Step, { kind: 'invoke' }>, loop: LoopKind | undefined, bindings: Bindings): void {
    if (this.returnDataSet) {
      throw new TypeError('invoke cannot follow setReturnData: invoking a program clears the return data');
    }
    const programAccount = this.encodeAccountReference(current.program, loop);
    const programConstraint = this.constraintFor(current.program, loop);
    this.requirePinnedProgram(current.program, programConstraint, 'Invoke program');
    if (
      current.programAddress &&
      programConstraint.address &&
      !equalBytes(current.programAddress, programConstraint.address)
    ) {
      throw new TypeError(
        `Invoke targets program ${toHex(current.programAddress)} but account ${current.program.name} pins ${toHex(programConstraint.address)}`,
      );
    }
    if (current.accounts.length > MAX_CPI_ACCOUNTS) throw new RangeError('CPI passes more than 64 accounts');

    const accountStart = this.cpiAccounts.length;
    for (const account of current.accounts) {
      const reference = this.encodeAccountReference(account.account, loop);
      const constraint = this.constraintFor(account.account, loop);
      if (account.signer && !constraint.signer) throw new TypeError('CPI signer is not required by its account schema');
      if (account.writable && !constraint.writable) throw new TypeError('CPI writable account is not writable in its schema');
      if (account.writable && account.account.kind === 'account' && this.template.accounts[account.account.name]?.registry) {
        throw new TypeError(
          `${account.account.name} is a registry entry: a CPI that passes it writable fails with RegistryReentry`,
        );
      }
      this.cpiAccounts.push(Uint8Array.of(reference, (account.signer ? ACCOUNT_SIGNER : 0) | (account.writable ? ACCOUNT_WRITABLE : 0)));
    }

    const { segmentStart, maxLength: maxDataLength } = this.compileDataParts(current.data, loop, bindings);
    if (maxDataLength > MAX_CPI_DATA_LENGTH) throw new RangeError('CPI data can exceed 4096 bytes');
    this.maxCpiDataLength = Math.max(this.maxCpiDataLength, maxDataLength);

    let accountGroup = NO_INDEX;
    if (current.accountGroup !== undefined) {
      const index = this.accountGroupIndices.get(current.accountGroup);
      if (index === undefined) throw new TypeError(`Unknown account group: ${current.accountGroup}`);
      accountGroup = index;
    }
    const descriptor = new Writer();
    descriptor.u8(programAccount);
    descriptor.u8(accountGroup);
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
      const result = this.compileExpression(current.when, loop, bindings);
      requireType(result, 'bool', 'invoke guard');
      guard = result.register;
    }
    this.pushInstruction(instructionRecord(opcode.invoke, NO_INDEX, cpiIndex, guard));
  }

  /** EMIT and SET_RETURN_DATA: the parts are encoded as invocation data is, to at most 1,024 bytes. */
  compileOutput(current: Extract<Step, { kind: 'emit' | 'setReturnData' }>, loop: LoopKind | undefined, bindings: Bindings): void {
    if (current.kind === 'emit') {
      // A log line names the program that wrote it, Ballista, but not the template. Without a tag,
      // a template could log a byte-exact copy of the run event for any template address.
      const [tag] = current.parts;
      if (tag?.kind !== 'literal' || tag.bytes.length < MIN_EMIT_TAG_LENGTH) {
        throw new TypeError(
          `emit must start with a literal tag of at least ${MIN_EMIT_TAG_LENGTH} bytes, so its log cannot pass for Ballista's run event`,
        );
      }
      if (RUN_EVENT_TAG_FAMILY.every((byte, index) => tag.bytes[index] === byte)) {
        const family = String.fromCharCode(...RUN_EVENT_TAG_FAMILY);
        throw new TypeError(`emit tag cannot start with "${family}": that tag family is reserved for Ballista's run event`);
      }
    }
    if (current.kind === 'setReturnData') {
      // Solana clears return data whenever a program is invoked, so what a run returns is set
      // once, outside every loop, after its last invoke.
      if (loop) throw new TypeError('setReturnData is not allowed inside a loop');
      if (this.returnDataSet) throw new TypeError('setReturnData may appear only once');
      this.returnDataSet = true;
    }
    const { segmentStart, maxLength } = this.compileDataParts(current.parts, loop, bindings);
    if (maxLength > MAX_RETURN_DATA_LENGTH) {
      throw new RangeError(`${current.kind} can encode ${maxLength} bytes; maximum is ${MAX_RETURN_DATA_LENGTH}`);
    }
    const operation = opcode[current.kind];
    this.pushInstruction(
      instructionRecord(operation, NO_INDEX, NO_INDEX, NO_INDEX, NO_INDEX, rangeImmediate(segmentStart, current.parts.length)),
    );
  }

  compileReturnData(node: Extract<Expression, { kind: 'returnData' }>, previous: Step | undefined): ExpressionResult {
    if (!previous || previous.kind !== 'invoke' || previous.when) {
      throw new TypeError('returnData must be the value of a let step directly after an unconditional invoke');
    }
    if (node.offset + readWidth[node.type] > MAX_RETURN_DATA_LENGTH) {
      throw new RangeError(`returnData read extends past ${MAX_RETURN_DATA_LENGTH} bytes`);
    }
    return this.emit(opcode.returnData, readResultType[node.type], 0, readOpcode[node.type], NO_INDEX, NO_INDEX, BigInt(node.offset));
  }

  /**
   * Compiles a step's data parts, then appends their segments as one contiguous run and returns
   * where it starts. A part can push segments of its own while it compiles (a `pda` expression's
   * seeds), so appending each part's segment as soon as it compiled would leave those seeds inside
   * the step's range, and the step would encode a seed where it meant the part.
   */
  compileDataParts(parts: DataPart[], loop: LoopKind | undefined, bindings: Bindings): { segmentStart: number; maxLength: number } {
    const compiled = parts.map((part) => this.compileDataPart(part, loop, bindings));
    const segmentStart = this.dataSegments.length;
    for (const { record } of compiled) this.dataSegments.push(record);
    return { segmentStart, maxLength: compiled.reduce((total, { maxLength }) => total + maxLength, 0) };
  }

  compileDataPart(part: DataPart, loop: LoopKind | undefined, bindings: Bindings): { record: Uint8Array; maxLength: number } {
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

    const value = this.compileExpression(part.value, loop, bindings);
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

  compileExpression(current: Expression, loop: LoopKind | undefined, bindings: Bindings): ExpressionResult {
    if (current.kind === 'input') {
      const loaded = this.fixedInputs.get(current.name);
      if (loaded !== undefined) return loaded;
      const index = this.inputIndices.get(current.name);
      if (index === undefined) throw new TypeError(`Unknown input: ${current.name}`);
      const definition = this.inputEntries[index]![1];
      return this.emit(opcode.loadInput, definition.type, definition.type === 'bytes' ? definition.maxLength : 0, index);
    }
    if (current.kind === 'rowInput') {
      if (loop !== 'rows') throw new TypeError('Row inputs are only valid inside forEach');
      const index = this.rowInputIndices.get(current.name);
      if (index === undefined) throw new TypeError(`Unknown row input: ${current.name}`);
      const definition = this.rowInputEntries[index]![1];
      return this.emit(
        opcode.loadInput,
        definition.type,
        definition.type === 'bytes' ? definition.maxLength : 0,
        ITERATION_ACCOUNT_BIT | index,
      );
    }
    if (current.kind === 'variable') {
      const value = bindings.get(current.name);
      if (value === undefined) throw new TypeError(`Unknown variable: ${current.name}`);
      return value;
    }
    if (current.kind === 'literal') {
      const key = literalKey(current.value);
      const loaded = this.constants.get(key);
      if (loaded !== undefined) return loaded;
      const result = this.emitLiteral(current.value);
      this.constants.set(key, result);
      return result;
    }
    if (current.kind === 'accountField') {
      const accountReference = this.encodeAccountReference(current.account, loop);
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
      const accountReference = this.encodeAccountReference(current.account, loop);
      const constraint = this.constraintFor(current.account, loop);
      this.refuseRegistryData(current.account, constraint);
      this.requirePinnedForRead(current.account, constraint);
      const operation = readOpcode[current.type];
      const type = readResultType[current.type];
      if (typeof current.offset === 'number') {
        this.raiseDataFloor(current.account, current.offset + readWidth[current.type]);
        return this.emit(operation, type, 0, accountReference, NO_INDEX, NO_INDEX, BigInt(current.offset));
      }
      const offset = this.compileExpression(current.offset, loop, bindings);
      requireType(offset, 'u64', 'accountData offset');
      return this.emit(operation, type, 0, accountReference, offset.register, NO_INDEX, 0n, INSTRUCTION_FLAG_DYNAMIC_OFFSET);
    }
    if (current.kind === 'returnData') {
      throw new TypeError('returnData must be the value of a let step directly after an unconditional invoke');
    }
    if (current.kind === 'clock') {
      return current.field === 'slot'
        ? this.emit(opcode.clockSlot, 'u64')
        : this.emit(opcode.clockTimestamp, 'i64');
    }
    if (current.kind === 'loopIndex') {
      if (!loop) throw new TypeError('loopIndex is only valid inside a loop');
      return this.emit(opcode.loopIndex, 'u64');
    }
    if (current.kind === 'pda') {
      const programAccount = this.encodeAccountReference(current.program, loop);
      const programConstraint = this.constraintFor(current.program, loop);
      this.requirePinnedProgram(current.program, programConstraint, 'PDA program');
      if (current.seeds.length < 1 || current.seeds.length > MAX_PDA_SEEDS) {
        throw new RangeError(`PDA derivation requires 1 to ${MAX_PDA_SEEDS} seeds`);
      }
      // A supplied bump turns the canonical-bump search into a single derivation, which is the
      // difference between about 4,800 and 1,500 compute units.
      let bumpRegister = NO_INDEX;
      if (current.bump !== undefined) {
        const bump = this.compileExpression(current.bump, loop, bindings);
        requireType(bump, 'u64', 'PDA bump');
        bumpRegister = bump.register;
      }
      // Every seed compiles before any seed segment is appended. A seed that is itself a `pda`
      // pushes its own seeds while it compiles, and those must not land inside this derivation's
      // range, as data parts must not (see `compileDataParts`).
      const seeds = current.seeds.map((seed) => {
        const value = this.compileExpression(seed, loop, bindings);
        const seedLength = value.type === 'bytes' ? value.maxLength : fixedValueLength(value.type);
        if (seedLength > MAX_PDA_SEED_LENGTH) {
          throw new RangeError(`PDA seed can exceed ${MAX_PDA_SEED_LENGTH} bytes`);
        }
        return value;
      });
      const segmentStart = this.dataSegments.length;
      for (const value of seeds) this.dataSegments.push(this.compileSeedSegment(value));
      return this.emit(
        current.bump === undefined ? opcode.derivePda : opcode.createPda,
        'pubkey',
        0,
        programAccount,
        bumpRegister,
        NO_INDEX,
        rangeImmediate(segmentStart, current.seeds.length),
      );
    }
    if (current.kind === 'not') {
      const value = this.compileExpression(current.value, loop, bindings);
      requireType(value, 'bool', 'not');
      return this.emit(opcode.not, 'bool', 0, value.register);
    }
    if (current.kind === 'multiplyDivide') {
      const left = this.compileExpression(current.left, loop, bindings);
      const right = this.compileExpression(current.right, loop, bindings);
      const divisor = this.compileExpression(current.divisor, loop, bindings);
      if (left.type !== right.type || left.type !== divisor.type || !isUnsigned(left.type)) {
        throw new TypeError('multiplyDivide requires three u64 or three u128 operands');
      }
      const operation = current.rounding === 'up' ? opcode.mulDivCeil : opcode.mulDiv;
      return this.emit(operation, left.type, 0, left.register, right.register, divisor.register);
    }
    if (current.kind === 'powerOfTen') {
      const exponent = this.compileExpression(current.exponent, loop, bindings);
      requireType(exponent, 'u64', 'powerOfTen');
      return this.emit(opcode.powerOfTen, 'u128', 0, exponent.register);
    }
    if (current.kind === 'cast') {
      const value = this.compileExpression(current.value, loop, bindings);
      if (!isNumeric(value.type)) throw new TypeError('cast requires a numeric expression');
      const operation = { u64: opcode.castU64, i64: opcode.castI64, u128: opcode.castU128 }[current.to];
      return this.emit(operation, current.to, 0, value.register);
    }
    if (current.kind === 'instructionCount' || current.kind === 'currentInstructionIndex') {
      const sysvar = this.encodeSysvar(current.sysvar);
      const operation = current.kind === 'instructionCount' ? opcode.instructionCount : opcode.instructionIndex;
      return this.emit(operation, 'u64', 0, sysvar);
    }
    if (current.kind === 'instruction') {
      const sysvar = this.encodeSysvar(current.sysvar);
      const index = this.compileExpression(current.index, loop, bindings);
      requireType(index, 'u64', 'instruction index');
      const fields = {
        program: [opcode.instructionProgram, 'pubkey'],
        accountCount: [opcode.instructionAccountCount, 'u64'],
        dataLength: [opcode.instructionDataLength, 'u64'],
      } as const;
      const [operation, type] = fields[current.field];
      return this.emit(operation, type, 0, sysvar, index.register);
    }
    if (current.kind === 'instructionAccount') {
      const sysvar = this.encodeSysvar(current.sysvar);
      const index = this.compileExpression(current.index, loop, bindings);
      const position = this.compileExpression(current.position, loop, bindings);
      requireType(index, 'u64', 'instruction index');
      requireType(position, 'u64', 'instruction account position');
      return current.field === 'key'
        ? this.emit(opcode.instructionAccount, 'pubkey', 0, sysvar, index.register, position.register)
        : this.emit(opcode.instructionAccountFlags, 'u64', 0, sysvar, index.register, position.register);
    }
    if (current.kind === 'instructionData' || current.kind === 'instructionDataBytes') {
      const sysvar = this.encodeSysvar(current.sysvar);
      const index = this.compileExpression(current.index, loop, bindings);
      const offset = this.compileExpression(current.offset, loop, bindings);
      requireType(index, 'u64', 'instruction index');
      requireType(offset, 'u64', `${current.kind} offset`);
      if (current.kind === 'instructionData') {
        const type = readResultType[current.type];
        const selector = BigInt(readOpcode[current.type]);
        return this.emit(opcode.readInstructionData, type, 0, sysvar, index.register, offset.register, selector);
      }
      return this.emit(
        opcode.readInstructionBytes,
        'bytes',
        current.length,
        sysvar,
        index.register,
        offset.register,
        BigInt(current.length),
      );
    }
    if (current.kind === 'accountDataBytes') {
      const accountReference = this.encodeAccountReference(current.account, loop);
      const constraint = this.constraintFor(current.account, loop);
      this.refuseRegistryData(current.account, constraint);
      this.requirePinnedForRead(current.account, constraint);
      if (constraint.writable) {
        throw new TypeError(
          `accountDataBytes reads only accounts this instruction cannot write; ${current.account.name} is declared writable`,
        );
      }
      const offset = this.compileExpression(current.offset, loop, bindings);
      requireType(offset, 'u64', 'accountDataBytes offset');
      return this.emit(
        opcode.readAccountBytes,
        'bytes',
        current.length,
        accountReference,
        offset.register,
        NO_INDEX,
        BigInt(current.length),
      );
    }
    if (current.kind === 'bytesLength') {
      const value = this.compileExpression(current.value, loop, bindings);
      requireType(value, 'bytes', 'bytesLength');
      return this.emit(opcode.bytesLength, 'u64', 0, value.register);
    }
    if (current.kind === 'registry') {
      const field = this.registryField(current.account, current.field);
      return this.emit(
        opcode.readRegistry,
        field.type,
        0,
        this.fixedIndices.get(current.account)!,
        NO_INDEX,
        NO_INDEX,
        registryFieldImmediate(field),
      );
    }
    if (current.kind === 'select') {
      const condition = this.compileExpression(current.condition, loop, bindings);
      const ifTrue = this.compileExpression(current.ifTrue, loop, bindings);
      const ifFalse = this.compileExpression(current.ifFalse, loop, bindings);
      requireType(condition, 'bool', 'select condition');
      requireType(ifFalse, ifTrue.type, 'select branches');
      return this.emit(opcode.select, ifTrue.type, Math.max(ifTrue.maxLength, ifFalse.maxLength), condition.register, ifTrue.register, ifFalse.register);
    }

    const left = this.compileExpression(current.left, loop, bindings);
    const right = this.compileExpression(current.right, loop, bindings);
    const operation = opcode[current.op];
    if (current.op === 'shiftLeft' || current.op === 'shiftRight') {
      if (!isUnsigned(left.type)) {
        throw new TypeError(`${current.op} requires a u64 or u128 value`);
      }
      requireType(right, 'u64', `${current.op} amount`);
      return this.emit(operation, left.type, 0, left.register, right.register);
    }
    if (current.op === 'bitAnd' || current.op === 'bitOr' || current.op === 'bitXor') {
      if (left.type !== right.type || !isUnsigned(left.type)) {
        throw new TypeError(`${current.op} requires matching u64 or u128 operands`);
      }
      return this.emit(operation, left.type, 0, left.register, right.register);
    }
    if (['add', 'subtract', 'multiply', 'divide', 'min', 'max', 'remainder'].includes(current.op)) {
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

  /** Programs that are invoked or derive PDAs must be pinned unless the author opts out. */
  requirePinnedProgram(reference: AccountReference, constraint: AccountConstraint, role: string): void {
    if (!constraint.executable) throw new TypeError(`${role} account must require executable=true`);
    if (!constraint.address && !constraint.unsafeUnpinned) {
      throw new TypeError(
        `${role} account ${reference.name} must pin an address; set unsafeUnpinned: true to accept any program`,
      );
    }
  }

  /** Introspection reads the Instructions sysvar through a fixed account pinned to its address. */
  encodeSysvar(reference: AccountReference): number {
    const constraint = reference.kind === 'account' ? this.constraintFor(reference, undefined) : undefined;
    if (!constraint?.address || !equalBytes(constraint.address, INSTRUCTIONS_SYSVAR_ADDRESS_BYTES)) {
      throw new TypeError(
        `Account ${reference.name} must be a fixed account pinned to the Instructions sysvar (INSTRUCTIONS_SYSVAR_ADDRESS_BYTES)`,
      );
    }
    return this.encodeAccountReference(reference, undefined);
  }

  /**
   * A registry entry's data is read only through its fields, with `expression.registry`: the
   * verifier refuses any other data read of an account an open names, wherever it sits.
   */
  refuseRegistryData(reference: AccountReference, constraint: AccountConstraint): void {
    if (constraint.registry !== undefined) {
      throw new TypeError(
        `${reference.name} is a registry entry: read its fields with expression.registry('${reference.name}', field)`,
      );
    }
  }

  /** Data reads only mean something when the account's layout is known, which needs a pin. */
  requirePinnedForRead(reference: AccountReference, constraint: AccountConstraint): void {
    if (!constraint.owner && !constraint.address && !constraint.unsafeUnpinned) {
      throw new TypeError(
        `Account ${reference.name} is read as data but pins neither owner nor address; set unsafeUnpinned: true to read untrusted data`,
      );
    }
  }

  raiseDataFloor(reference: AccountReference, end: number): void {
    const key = reference.kind === 'account' ? fixedKey(reference.name) : rowKey(reference.name);
    this.requiredDataLength.set(key, Math.max(this.requiredDataLength.get(key) ?? 0, end));
  }

  pushInstruction(record: Uint8Array): number {
    const pc = this.instructions.length;
    this.instructions.push(record);
    this.sourceMap.push({ pc, ...this.location });
    return pc;
  }

  emit(
    operation: number,
    type: ValueType,
    maxLength = 0,
    a = NO_INDEX,
    b = NO_INDEX,
    c = NO_INDEX,
    immediate = 0n,
    flags = 0,
  ): ExpressionResult {
    const register = this.nextRegister;
    this.nextRegister += 1;
    if (register >= MAX_REGISTERS) throw new RangeError('Template uses more than 64 registers');
    this.pushInstruction(instructionRecord(operation, register, a, b, c, immediate, flags));
    return { register, type, maxLength };
  }

  emitLiteral(value: Literal): ExpressionResult {
    switch (value.type) {
      case 'bool':
        return this.emit(opcode.constBool, 'bool', 0, value.value ? 1 : 0);
      case 'u64':
        return this.emit(opcode.constU64, 'u64', 0, NO_INDEX, NO_INDEX, NO_INDEX, value.value);
      case 'i64':
        return this.emit(opcode.constI64, 'i64', 0, NO_INDEX, NO_INDEX, NO_INDEX, value.value);
      case 'u128': {
        const offset = this.addBlob(encodeBigint(value.value, 16));
        return this.emit(opcode.constU128, 'u128', 0, NO_INDEX, NO_INDEX, NO_INDEX, blobImmediate(offset, 16));
      }
      case 'pubkey':
        return this.emit(opcode.constPubkey, 'pubkey', 0, this.addPubkey(value.value));
      case 'bytes': {
        const offset = this.addBlob(value.value);
        return this.emit(opcode.constBytes, 'bytes', value.value.length, NO_INDEX, NO_INDEX, NO_INDEX, blobImmediate(offset, value.value.length));
      }
    }
  }

  encodeAccountReference(reference: AccountReference, loop: LoopKind | undefined): number {
    if (reference.kind === 'account') {
      const index = this.fixedIndices.get(reference.name);
      if (index === undefined) throw new TypeError(`Unknown fixed account: ${reference.name}`);
      return index;
    }
    if (loop !== 'rows') throw new TypeError('Iteration accounts are only valid inside forEach');
    const index = this.batchIndices.get(reference.name);
    if (index === undefined) throw new TypeError(`Unknown batch account: ${reference.name}`);
    return ITERATION_ACCOUNT_BIT | index;
  }

  constraintFor(reference: AccountReference, loop: LoopKind | undefined): AccountConstraint {
    if (reference.kind === 'account') {
      const index = this.fixedIndices.get(reference.name);
      if (index === undefined) throw new TypeError(`Unknown fixed account: ${reference.name}`);
      return this.fixedEntries[index]![1];
    }
    if (loop !== 'rows') throw new TypeError('Iteration accounts are only valid inside forEach');
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

function fixedKey(name: string): string {
  return `account:${name}`;
}

function rowKey(name: string): string {
  return `row:${name}`;
}

interface RegistryField {
  offset: number;
  type: RegistryFieldType;
}

interface RegistryLayout {
  index: number;
  size: number;
  fields: Map<string, RegistryField>;
}

/** `READ_REGISTRY`'s and `WRITE_REGISTRY`'s immediate: the offset, then the read opcode at byte 2. */
function registryFieldImmediate(field: RegistryField): bigint {
  return BigInt(field.offset) | (BigInt(readOpcode[field.type]) << 16n);
}

function instructionRecord(
  operation: number,
  dst: number,
  a = NO_INDEX,
  b = NO_INDEX,
  c = NO_INDEX,
  immediate = 0n,
  flags = 0,
): Uint8Array {
  const writer = new Writer();
  writer.u8(operation);
  writer.u8(dst);
  writer.u8(a);
  writer.u8(b);
  writer.u8(c);
  writer.u8(flags);
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

/** The two types the bitwise, shift, and multiply-divide opcodes accept; `i64` is signed and excluded. */
function isUnsigned(type: ValueType): boolean {
  return type === 'u64' || type === 'u128';
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
    if (current.kind === 'forEach' || current.kind === 'repeat') return total + countCpis(current.steps);
    return total;
  }, 0);
}

/**
 * The most invocations a run can reach: each loop's body runs its maximum number of times, the
 * batch's for a `forEach` and its own `max` for a `repeat`, and every other invoke once.
 */
function worstCaseCpis(steps: Step[], batchMaxIterations: number): number {
  return steps.reduce((total, current) => {
    if (current.kind === 'forEach') return total + countCpis(current.steps) * batchMaxIterations;
    if (current.kind === 'repeat') return total + countCpis(current.steps) * current.max;
    return total + countCpis([current]);
  }, 0);
}

function equalBytes(left: Uint8Array, right: Uint8Array): boolean {
  return left.length === right.length && left.every((byte, index) => byte === right[index]);
}

function toHex(value: Uint8Array): string {
  return [...value].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}
