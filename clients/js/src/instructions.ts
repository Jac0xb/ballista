import {
  MAX_INPUT_BYTES,
  MAX_TEMPLATE_PAYLOAD_LENGTH,
  PROGRAM_FLAG_EMIT_EVENT,
  TEMPLATE_PROGRAM_VERSION,
  type CompiledTemplate,
  type CompileStats,
} from './compiler.js';
import type { AccountConstraint, InputDefinition } from './schema.js';

export const BALLISTA_PROGRAM_ADDRESS = 'BLSTAxXJ6fXnsQ2hxZmFQ1MYQaxpdqAtRNuo6ckY2mfD';
export const TEMPLATE_ACCOUNT_VERSION = 2;
export const TEMPLATE_ACCOUNT_HEADER_LENGTH = 80;
export const TEMPLATE_STATE_UPLOADING = 0;
export const TEMPLATE_STATE_FINALIZED = 1;

export const INSTRUCTION_CREATE_TEMPLATE = 0;
export const INSTRUCTION_BEGIN_TEMPLATE = 1;
export const INSTRUCTION_WRITE_TEMPLATE_CHUNK = 2;
export const INSTRUCTION_FINALIZE_TEMPLATE = 3;
export const INSTRUCTION_CANCEL_TEMPLATE = 4;
export const INSTRUCTION_RUN = 5;

export interface UploadInstruction {
  kind: 'create' | 'begin' | 'write' | 'finalize';
  data: Uint8Array;
  offset?: number;
}

export interface TemplateUploadPlan {
  mode: 'oneShot' | 'chunked';
  instructions: readonly UploadInstruction[];
  payloadLength: number;
  payloadHash: Uint8Array;
}

export interface DecodedTemplateAccount {
  version: 2;
  state: 0 | 1;
  bump: number;
  creator: Uint8Array;
  templateId: number;
  payloadLength: number;
  writtenLength: number;
  payloadHash: Uint8Array;
  payload: Uint8Array;
}

export interface AccountBinding {
  address: Uint8Array;
}

export interface InstructionAccountDescriptor {
  address: Uint8Array;
  signer: boolean;
  writable: boolean;
}

export interface InstructionDescriptor {
  programAddress: Uint8Array;
  accounts: readonly InstructionAccountDescriptor[];
  data: Uint8Array;
}

export type RunInputValue = boolean | bigint | number | Uint8Array;

export function encodeCreateTemplate(compiled: CompiledTemplate, templateId: number): Uint8Array {
  const writer = new Writer();
  writer.u8(INSTRUCTION_CREATE_TEMPLATE);
  writer.u16(templateId);
  writer.raw(compiled.hash);
  writer.raw(compiled.bytes);
  return writer.finish();
}

export function encodeBeginTemplate(compiled: CompiledTemplate, templateId: number): Uint8Array {
  const writer = new Writer();
  writer.u8(INSTRUCTION_BEGIN_TEMPLATE);
  writer.u16(templateId);
  writer.u32(compiled.bytes.length);
  writer.raw(compiled.hash);
  return writer.finish();
}

export function encodeWriteTemplateChunk(offset: number, bytes: Uint8Array): Uint8Array {
  if (bytes.length === 0) throw new RangeError('Template chunks cannot be empty');
  const writer = new Writer();
  writer.u8(INSTRUCTION_WRITE_TEMPLATE_CHUNK);
  writer.u32(offset);
  writer.raw(bytes);
  return writer.finish();
}

export function encodeFinalizeTemplate(): Uint8Array {
  return Uint8Array.of(INSTRUCTION_FINALIZE_TEMPLATE);
}

export function encodeCancelTemplate(): Uint8Array {
  return Uint8Array.of(INSTRUCTION_CANCEL_TEMPLATE);
}

export function planTemplateUpload(
  compiled: CompiledTemplate,
  templateId: number,
  options: { maxInstructionDataBytes?: number } = {},
): TemplateUploadPlan {
  validateTemplateId(templateId);
  const maxInstructionDataBytes = options.maxInstructionDataBytes ?? 3_500;
  if (maxInstructionDataBytes < 64) throw new RangeError('Instruction data limit is too small');
  const oneShot = encodeCreateTemplate(compiled, templateId);
  if (oneShot.length <= maxInstructionDataBytes) {
    return {
      mode: 'oneShot',
      instructions: [{ kind: 'create', data: oneShot }],
      payloadLength: compiled.bytes.length,
      payloadHash: compiled.hash,
    };
  }

  const chunkLength = maxInstructionDataBytes - 5;
  const instructions: UploadInstruction[] = [
    { kind: 'begin', data: encodeBeginTemplate(compiled, templateId) },
  ];
  for (let offset = 0; offset < compiled.bytes.length; offset += chunkLength) {
    const bytes = compiled.bytes.slice(offset, Math.min(offset + chunkLength, compiled.bytes.length));
    instructions.push({ kind: 'write', offset, data: encodeWriteTemplateChunk(offset, bytes) });
  }
  instructions.push({ kind: 'finalize', data: encodeFinalizeTemplate() });
  return {
    mode: 'chunked',
    instructions,
    payloadLength: compiled.bytes.length,
    payloadHash: compiled.hash,
  };
}

export function resumeTemplateUpload(
  compiled: CompiledTemplate,
  account: DecodedTemplateAccount | Uint8Array,
  options: { maxInstructionDataBytes?: number } = {},
): TemplateUploadPlan {
  const decoded = account instanceof Uint8Array ? decodeTemplateAccount(account) : account;
  if (decoded.state !== TEMPLATE_STATE_UPLOADING) throw new TypeError('Template is already finalized');
  if (decoded.payloadLength !== compiled.bytes.length || !equalBytes(decoded.payloadHash, compiled.hash)) {
    throw new TypeError('On-chain upload does not match the compiled template');
  }
  if (!equalBytes(decoded.payload.slice(0, decoded.writtenLength), compiled.bytes.slice(0, decoded.writtenLength))) {
    throw new TypeError('Previously uploaded template bytes do not match');
  }

  const maxInstructionDataBytes = options.maxInstructionDataBytes ?? 3_500;
  const chunkLength = maxInstructionDataBytes - 5;
  if (chunkLength <= 0) throw new RangeError('Instruction data limit is too small');
  const instructions: UploadInstruction[] = [];
  for (let offset = decoded.writtenLength; offset < compiled.bytes.length; offset += chunkLength) {
    const bytes = compiled.bytes.slice(offset, Math.min(offset + chunkLength, compiled.bytes.length));
    instructions.push({ kind: 'write', offset, data: encodeWriteTemplateChunk(offset, bytes) });
  }
  instructions.push({ kind: 'finalize', data: encodeFinalizeTemplate() });
  return {
    mode: 'chunked',
    instructions,
    payloadLength: compiled.bytes.length,
    payloadHash: compiled.hash,
  };
}

export function encodeRunInputs(
  compiled: CompiledTemplate,
  values: Readonly<Record<string, RunInputValue>>,
): Uint8Array {
  const writer = new Writer();
  for (const name of compiled.inputOrder) {
    if (!(name in values)) throw new TypeError(`Missing input: ${name}`);
    encodeInput(writer, compiled.template.inputs[name]!, values[name]!);
  }
  const unknown = Object.keys(values).filter((name) => !compiled.inputOrder.includes(name));
  if (unknown.length > 0) throw new TypeError(`Unknown inputs: ${unknown.join(', ')}`);
  const bytes = writer.finish();
  if (bytes.length > MAX_INPUT_BYTES) throw new RangeError('Encoded run inputs exceed 1024 bytes');
  return bytes;
}

export function encodeRun(
  compiled: CompiledTemplate,
  values: Readonly<Record<string, RunInputValue>>,
): Uint8Array {
  const inputs = encodeRunInputs(compiled, values);
  return Uint8Array.of(INSTRUCTION_RUN, ...inputs);
}

export function buildRunInstruction(input: {
  compiled: CompiledTemplate;
  programAddress: Uint8Array;
  templateAddress: Uint8Array;
  inputs?: Readonly<Record<string, RunInputValue>>;
  accounts: Readonly<Record<string, AccountBinding>>;
  batchRows?: readonly Readonly<Record<string, AccountBinding>>[];
}): InstructionDescriptor {
  assertAddress(input.programAddress, 'program address');
  assertAddress(input.templateAddress, 'template address');
  const accounts: InstructionAccountDescriptor[] = [
    { address: input.templateAddress.slice(), signer: false, writable: false },
  ];
  for (const name of input.compiled.fixedAccountOrder) {
    const binding = input.accounts[name];
    if (!binding) throw new TypeError(`Missing account binding: ${name}`);
    accounts.push(bindingDescriptor(binding, input.compiled.template.accounts[name]!, name));
  }
  const unknownAccounts = Object.keys(input.accounts).filter(
    (name) => !input.compiled.fixedAccountOrder.includes(name),
  );
  if (unknownAccounts.length > 0) throw new TypeError(`Unknown account bindings: ${unknownAccounts.join(', ')}`);

  const rows = input.batchRows ?? [];
  if (!input.compiled.template.batch && rows.length > 0) throw new TypeError('Template has no batch range');
  if (rows.length > (input.compiled.template.batch?.maxIterations ?? 0)) {
    throw new RangeError('Batch row count exceeds the template maximum');
  }
  if (rows.length < (input.compiled.template.batch?.minIterations ?? 0)) {
    throw new RangeError('Batch row count is below the template minimum');
  }
  for (const [rowIndex, row] of rows.entries()) {
    for (const name of input.compiled.batchAccountOrder) {
      const binding = row[name];
      if (!binding) throw new TypeError(`Missing batch account ${name} in row ${rowIndex}`);
      accounts.push(bindingDescriptor(binding, input.compiled.template.batch!.row[name]!, name));
    }
    const unknown = Object.keys(row).filter((name) => !input.compiled.batchAccountOrder.includes(name));
    if (unknown.length > 0) throw new TypeError(`Unknown batch accounts in row ${rowIndex}: ${unknown.join(', ')}`);
  }
  if (accounts.length - 1 > 60) throw new RangeError('Run uses more than 60 runtime account slots');

  return {
    programAddress: input.programAddress.slice(),
    accounts,
    data: encodeRun(input.compiled, input.inputs ?? {}),
  };
}

export function decodeTemplateAccount(data: Uint8Array): DecodedTemplateAccount {
  if (data.length < TEMPLATE_ACCOUNT_HEADER_LENGTH) throw new RangeError('Template account is truncated');
  const reader = new Reader(data);
  if (reader.u8() !== 1) throw new TypeError('Invalid template account discriminator');
  if (reader.u8() !== TEMPLATE_ACCOUNT_VERSION) throw new TypeError('Unsupported template account version');
  const state = reader.u8();
  if (state !== TEMPLATE_STATE_UPLOADING && state !== TEMPLATE_STATE_FINALIZED) {
    throw new TypeError('Invalid template account state');
  }
  const bump = reader.u8();
  const creator = reader.bytes(32);
  const templateId = reader.u16();
  if (reader.u16() !== 0) throw new TypeError('Invalid template account reserved bytes');
  const payloadLength = reader.u32();
  const writtenLength = reader.u32();
  const payloadHash = reader.bytes(32);
  const payload = reader.bytes(reader.remaining);
  if (payloadLength === 0 || payloadLength > MAX_TEMPLATE_PAYLOAD_LENGTH || payload.length !== payloadLength) {
    throw new RangeError('Invalid template payload length');
  }
  if (writtenLength > payloadLength || (state === TEMPLATE_STATE_FINALIZED && writtenLength !== payloadLength)) {
    throw new RangeError('Invalid template written length');
  }
  return {
    version: 2,
    state,
    bump,
    creator,
    templateId,
    payloadLength,
    writtenLength,
    payloadHash,
    payload,
  };
}

export function inspectTemplate(bytes: Uint8Array): CompileStats {
  if (bytes.length < 24) throw new RangeError('Template program is truncated');
  const reader = new Reader(bytes);
  if (!equalBytes(reader.bytes(4), Uint8Array.of(0x42, 0x56, 0x4d, 0x32))) {
    throw new TypeError('Invalid template program magic');
  }
  if (reader.u8() !== TEMPLATE_PROGRAM_VERSION) throw new TypeError('Unsupported template program version');
  const fixedAccounts = reader.u8();
  const batchStride = reader.u8();
  const batchMaxIterations = reader.u8();
  const inputs = reader.u8();
  const registers = reader.u8();
  const instructions = reader.u8();
  const cpis = reader.u8();
  const cpiAccounts = reader.u16();
  const dataSegments = reader.u16();
  const pubkeys = reader.u8();
  const flags = reader.u8();
  if ((flags & ~PROGRAM_FLAG_EMIT_EVENT) !== 0) throw new TypeError('Invalid template flags');
  const blobLength = reader.u16();
  const batchMinIterations = reader.u8();
  if (!equalBytes(reader.bytes(3), new Uint8Array(3))) throw new TypeError('Invalid reserved bytes');
  const expectedLength =
    24 +
    (fixedAccounts + batchStride) * 8 +
    inputs * 4 +
    instructions * 16 +
    cpis * 12 +
    cpiAccounts * 2 +
    dataSegments * 8 +
    pubkeys * 32 +
    blobLength;
  if (bytes.length !== expectedLength) throw new RangeError('Template section lengths do not match');
  const instructionStart = 24 + (fixedAccounts + batchStride) * 8 + inputs * 4;
  const opcodes = Array.from({ length: instructions }, (_, index) => {
    const offset = instructionStart + index * 16;
    return { opcode: bytes[offset]!, a: bytes[offset + 2]! };
  });
  let maxExpandedCpis = 0;
  for (let programCounter = 0; programCounter < opcodes.length; programCounter += 1) {
    const instruction = opcodes[programCounter]!;
    if (instruction.opcode === 42) {
      const body = opcodes.slice(programCounter + 1, programCounter + 1 + instruction.a);
      maxExpandedCpis += body.filter((record) => record.opcode === 41).length * batchMaxIterations;
      programCounter += instruction.a;
    } else if (instruction.opcode === 41) {
      maxExpandedCpis += 1;
    }
  }
  const cpiStart = instructionStart + instructions * 16;
  let maxCpiDataLength = 0;
  for (let index = 0; index < cpis; index += 1) {
    const offset = cpiStart + index * 12 + 8;
    maxCpiDataLength = Math.max(maxCpiDataLength, bytes[offset]! | (bytes[offset + 1]! << 8));
  }
  return {
    payloadBytes: bytes.length,
    fixedAccounts,
    batchStride,
    batchMaxIterations,
    batchMinIterations,
    inputs,
    registers,
    instructions,
    cpis,
    maxExpandedCpis,
    maxCpiDataLength,
    emitEvent: (flags & PROGRAM_FLAG_EMIT_EVENT) !== 0,
  };
}

function bindingDescriptor(
  binding: AccountBinding,
  constraint: AccountConstraint,
  name: string,
): InstructionAccountDescriptor {
  assertAddress(binding.address, `account ${name}`);
  if (constraint.address && !equalBytes(binding.address, constraint.address)) {
    throw new TypeError(`Account ${name} does not match its fixed address`);
  }
  return {
    address: binding.address.slice(),
    signer: constraint.signer,
    writable: constraint.writable,
  };
}

function encodeInput(writer: Writer, definition: InputDefinition, value: RunInputValue): void {
  if (definition.type === 'bool') {
    if (typeof value !== 'boolean') throw new TypeError('Expected bool input');
    writer.u8(value ? 1 : 0);
    return;
  }
  if (definition.type === 'pubkey') {
    if (!(value instanceof Uint8Array) || value.length !== 32) throw new TypeError('Expected pubkey input');
    writer.raw(value);
    return;
  }
  if (definition.type === 'bytes') {
    if (!(value instanceof Uint8Array) || value.length > definition.maxLength) {
      throw new TypeError(`Expected at most ${definition.maxLength} input bytes`);
    }
    writer.u16(value.length);
    writer.raw(value);
    return;
  }
  if (typeof value !== 'bigint' && typeof value !== 'number') throw new TypeError(`Expected ${definition.type} input`);
  const bigint = BigInt(value);
  const ranges = {
    u64: [0n, (1n << 64n) - 1n],
    i64: [-(1n << 63n), (1n << 63n) - 1n],
    u128: [0n, (1n << 128n) - 1n],
  } as const;
  const [minimum, maximum] = ranges[definition.type];
  if (bigint < minimum || bigint > maximum) throw new RangeError(`Input is outside ${definition.type} range`);
  writer.bigint(bigint, definition.type === 'u128' ? 16 : 8);
}

function validateTemplateId(templateId: number): void {
  if (!Number.isInteger(templateId) || templateId < 0 || templateId > 0xffff) {
    throw new RangeError('Template ID must be an unsigned 16-bit integer');
  }
}

function assertAddress(value: Uint8Array, label: string): void {
  if (!(value instanceof Uint8Array) || value.length !== 32) throw new TypeError(`${label} must contain 32 bytes`);
}

function equalBytes(left: Uint8Array, right: Uint8Array): boolean {
  return left.length === right.length && left.every((byte, index) => byte === right[index]);
}

class Writer {
  readonly output: number[] = [];
  u8(value: number): void {
    if (!Number.isInteger(value) || value < 0 || value > 0xff) throw new RangeError('Expected u8');
    this.output.push(value);
  }
  u16(value: number): void {
    if (!Number.isInteger(value) || value < 0 || value > 0xffff) throw new RangeError('Expected u16');
    this.output.push(value & 0xff, (value >>> 8) & 0xff);
  }
  u32(value: number): void {
    if (!Number.isInteger(value) || value < 0 || value > 0xffff_ffff) throw new RangeError('Expected u32');
    this.output.push(value & 0xff, (value >>> 8) & 0xff, (value >>> 16) & 0xff, (value >>> 24) & 0xff);
  }
  bigint(value: bigint, byteLength: number): void {
    const encoded = BigInt.asUintN(byteLength * 8, value);
    for (let index = 0; index < byteLength; index += 1) {
      this.u8(Number((encoded >> BigInt(index * 8)) & 0xffn));
    }
  }
  raw(bytes: Uint8Array): void {
    this.output.push(...bytes);
  }
  finish(): Uint8Array {
    return Uint8Array.from(this.output);
  }
}

class Reader {
  offset = 0;
  constructor(readonly input: Uint8Array) {}
  get remaining(): number {
    return this.input.length - this.offset;
  }
  u8(): number {
    return this.bytes(1)[0]!;
  }
  u16(): number {
    const bytes = this.bytes(2);
    return bytes[0]! | (bytes[1]! << 8);
  }
  u32(): number {
    const bytes = this.bytes(4);
    return (bytes[0]! | (bytes[1]! << 8) | (bytes[2]! << 16) | (bytes[3]! << 24)) >>> 0;
  }
  bytes(length: number): Uint8Array {
    const end = this.offset + length;
    if (end > this.input.length) throw new RangeError('Input is truncated');
    const value = this.input.slice(this.offset, end);
    this.offset = end;
    return value;
  }
}
