/**
 * The compiler's opcode table is a copy of the constants in `common/src/template/wire.rs`, and so
 * are the two that shape an emit's tag. This pairs every Rust opcode with its TypeScript name and
 * checks the values agree, so a change on either side that the other does not follow fails here
 * rather than as a verifier rejection.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { expect, test } from 'vitest';

import { MIN_EMIT_TAG_LENGTH, opcode, RUN_EVENT_TAG_FAMILY } from './compiler.js';

const wire = readFileSync(
  fileURLToPath(new URL('../../../common/src/template/wire.rs', import.meta.url)),
  'utf8',
);
const rust = new Map(
  [...wire.matchAll(/pub const (OP_[A-Z0-9_]+): u8 = (\d+);/g)].map(([, name, value]) => [
    name!,
    Number(value),
  ]),
);

const rustName: Record<keyof typeof opcode, string> = {
  loadInput: 'OP_LOAD_INPUT',
  constBool: 'OP_CONST_BOOL',
  constU64: 'OP_CONST_U64',
  constI64: 'OP_CONST_I64',
  constU128: 'OP_CONST_U128',
  constPubkey: 'OP_CONST_PUBKEY',
  constBytes: 'OP_CONST_BYTES',
  accountKey: 'OP_ACCOUNT_KEY',
  accountOwner: 'OP_ACCOUNT_OWNER',
  accountLamports: 'OP_ACCOUNT_LAMPORTS',
  accountDataLength: 'OP_ACCOUNT_DATA_LEN',
  accountIsEmpty: 'OP_ACCOUNT_IS_EMPTY',
  readU64: 'OP_READ_U64',
  readI64: 'OP_READ_I64',
  readU128: 'OP_READ_U128',
  readPubkey: 'OP_READ_PUBKEY',
  clockSlot: 'OP_CLOCK_SLOT',
  clockTimestamp: 'OP_CLOCK_TIMESTAMP',
  add: 'OP_ADD',
  subtract: 'OP_SUB',
  multiply: 'OP_MUL',
  divide: 'OP_DIV',
  equal: 'OP_EQ',
  notEqual: 'OP_NE',
  lessThan: 'OP_LT',
  lessThanOrEqual: 'OP_LTE',
  greaterThan: 'OP_GT',
  greaterThanOrEqual: 'OP_GTE',
  and: 'OP_AND',
  or: 'OP_OR',
  not: 'OP_NOT',
  min: 'OP_MIN',
  max: 'OP_MAX',
  select: 'OP_SELECT',
  castU64: 'OP_CAST_U64',
  castI64: 'OP_CAST_I64',
  castU128: 'OP_CAST_U128',
  loopIndex: 'OP_LOOP_INDEX',
  require: 'OP_REQUIRE',
  invoke: 'OP_INVOKE',
  forEach: 'OP_FOREACH',
  readU8: 'OP_READ_U8',
  readU16: 'OP_READ_U16',
  readU32: 'OP_READ_U32',
  readBool: 'OP_READ_BOOL',
  derivePda: 'OP_DERIVE_PDA',
  returnData: 'OP_RETURN_DATA',
  move: 'OP_MOVE',
  createPda: 'OP_CREATE_PDA',
  mulDiv: 'OP_MUL_DIV',
  mulDivCeil: 'OP_MUL_DIV_CEIL',
  remainder: 'OP_REM',
  shiftLeft: 'OP_SHL',
  shiftRight: 'OP_SHR',
  bitAnd: 'OP_BIT_AND',
  bitOr: 'OP_BIT_OR',
  bitXor: 'OP_BIT_XOR',
  powerOfTen: 'OP_POW10',
  readI32: 'OP_READ_I32',
  repeat: 'OP_REPEAT',
  emit: 'OP_EMIT',
  setReturnData: 'OP_SET_RETURN_DATA',
  instructionCount: 'OP_INSTRUCTION_COUNT',
  instructionIndex: 'OP_INSTRUCTION_INDEX',
  instructionProgram: 'OP_INSTRUCTION_PROGRAM',
  instructionAccountCount: 'OP_INSTRUCTION_ACCOUNT_COUNT',
  instructionAccount: 'OP_INSTRUCTION_ACCOUNT',
  instructionAccountFlags: 'OP_INSTRUCTION_ACCOUNT_FLAGS',
  instructionDataLength: 'OP_INSTRUCTION_DATA_LEN',
  readInstructionData: 'OP_READ_INSTRUCTION_DATA',
  readInstructionBytes: 'OP_READ_INSTRUCTION_BYTES',
  readAccountBytes: 'OP_READ_ACCOUNT_BYTES',
  bytesLength: 'OP_BYTES_LEN',
};

test('every opcode has the same number in Rust and TypeScript', () => {
  for (const [key, name] of Object.entries(rustName) as [keyof typeof opcode, string][]) {
    expect(rust.get(name), `${key} ↔ ${name}`).toBe(opcode[key]);
  }
  expect([...rust.keys()].sort()).toEqual(Object.values(rustName).sort());
});

test("an emit's tag rule is the same in Rust and TypeScript", () => {
  const family = /pub const RUN_EVENT_TAG_FAMILY: \[u8; \d+\] = \*b"([^"]*)";/.exec(wire)?.[1];
  const minimum = /pub const MIN_EMIT_TAG_LEN: usize = (\d+);/.exec(wire)?.[1];
  expect(family, 'RUN_EVENT_TAG_FAMILY in wire.rs').toBeDefined();
  expect(minimum, 'MIN_EMIT_TAG_LEN in wire.rs').toBeDefined();
  expect(new TextEncoder().encode(family)).toEqual(RUN_EVENT_TAG_FAMILY);
  expect(Number(minimum)).toBe(MIN_EMIT_TAG_LENGTH);
});
