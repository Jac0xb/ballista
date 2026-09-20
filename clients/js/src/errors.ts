import type { CompiledTemplate, SourceMapEntry } from './compiler.js';

/** First runtime error code; kinds follow `RUNTIME_ERROR_NAMES` in order. */
export const RUNTIME_ERROR_BASE = 6000;
/** First verifier error code; kinds follow `VERIFIER_ERROR_NAMES` in order. */
export const VERIFIER_ERROR_BASE = 6100;

/** Runtime failures raised by the program, in code order. Shared with Rust via `fixtures/`. */
export const RUNTIME_ERROR_NAMES = [
  'InvalidInstructionData',
  'InvalidTemplateAccount',
  'InvalidTemplateProgram',
  'TemplateNotUploading',
  'TemplateNotFinalized',
  'InvalidCreator',
  'InvalidChunkOffset',
  'HashMismatch',
  'InvalidRunInputs',
  'InvalidRuntimeAccount',
  'InvalidAccountRange',
  'InvalidRegister',
  'TypeMismatch',
  'ArithmeticOverflow',
  'DivisionByZero',
  'RequirementFailed',
  'CpiDataTooLarge',
  'InvalidPdaDerivation',
  'MissingReturnData',
  'ReturnDataMismatch',
  'AccountConstraintFailed',
  'CpiAccountLimitExceeded',
] as const;

/** Verifier failures raised at create and finalize, in code order. */
export const VERIFIER_ERROR_NAMES = [
  'Truncated',
  'PayloadTooLarge',
  'InvalidMagic',
  'UnsupportedVersion',
  'InvalidReservedBytes',
  'SectionLengthMismatch',
  'CountOverflow',
  'TooManyAccounts',
  'TooManyInputs',
  'TooManyRegisters',
  'TooManyInstructions',
  'InvalidBatch',
  'InvalidAccountConstraint',
  'InvalidInput',
  'InvalidInstruction',
  'InvalidCpi',
  'InvalidDataSegment',
  'InvalidRegister',
  'RegisterNotInitialized',
  'TypeMismatch',
  'InvalidBlobRange',
  'ExcessiveCpiExpansion',
  'InvalidFlags',
  'InvalidCarry',
  'ReadOutOfBounds',
  'TooManyCpiAccounts',
  'InvalidReturnData',
  'InvalidMinIterations',
  'TooManyAccountGroups',
] as const;

export type RuntimeErrorName = (typeof RUNTIME_ERROR_NAMES)[number];
export type VerifierErrorName = (typeof VERIFIER_ERROR_NAMES)[number];

export interface DecodedBallistaError {
  /** The raw custom error code. */
  code: number;
  /** The low 16 bits: the error kind. */
  kind: number;
  name: RuntimeErrorName | VerifierErrorName;
  /** The high 16 bits: program counter, account index, input index, count, or verifier context. */
  context: number;
  source: 'runtime' | 'verifier';
}

/**
 * Splits a Ballista custom error code into kind and context. Returns `undefined` for codes that
 * belong to another program, which pass through Ballista unchanged.
 */
export function decodeBallistaError(code: number): DecodedBallistaError | undefined {
  if (!Number.isInteger(code) || code < 0 || code > 0xffff_ffff) return undefined;
  const kind = code & 0xffff;
  const context = code >>> 16;
  if (kind >= RUNTIME_ERROR_BASE && kind < RUNTIME_ERROR_BASE + RUNTIME_ERROR_NAMES.length) {
    return { code, kind, name: RUNTIME_ERROR_NAMES[kind - RUNTIME_ERROR_BASE]!, context, source: 'runtime' };
  }
  if (kind >= VERIFIER_ERROR_BASE && kind < VERIFIER_ERROR_BASE + VERIFIER_ERROR_NAMES.length) {
    return { code, kind, name: VERIFIER_ERROR_NAMES[kind - VERIFIER_ERROR_BASE]!, context, source: 'verifier' };
  }
  return undefined;
}

export interface RunErrorExplanation {
  error: DecodedBallistaError;
  /** The authoring step that emitted the failing instruction, when the context is a program counter. */
  step?: SourceMapEntry;
  /** The runtime account that failed validation, when the context is an account index. */
  account?: { index: number; name: string; row?: number };
  /** The input that failed to decode, when the context is an input index. */
  input?: string;
  message: string;
}

/**
 * Maps a failed run's custom error code back to the template that produced it: the step for VM
 * failures, the account for constraint failures, or the input for decoding failures.
 */
export function explainRunError(code: number, compiled: CompiledTemplate): RunErrorExplanation | undefined {
  const error = decodeBallistaError(code);
  if (!error) return undefined;
  if (error.source === 'verifier') {
    return { error, message: `${error.name} (verifier context ${error.context})` };
  }
  switch (error.name) {
    case 'InvalidRunInputs': {
      const input = compiled.inputOrder[error.context];
      const message = input ? `${error.name}: input ${input} could not be decoded` : `${error.name}: unexpected trailing input bytes`;
      return { error, ...(input ? { input } : {}), message };
    }
    case 'InvalidAccountRange':
      return { error, message: `${error.name}: ${error.context} runtime accounts or iterations were supplied` };
    case 'AccountConstraintFailed': {
      const account = runtimeAccountName(compiled, error.context);
      const where = account.row === undefined ? account.name : `${account.name} in row ${account.row}`;
      return { error, account, message: `${error.name}: account ${where} does not satisfy its constraint` };
    }
    default: {
      const step = compiled.sourceMap.find((entry) => entry.pc === error.context);
      const where = step ? ` at ${step.path}${step.label ? ` (${step.label})` : ''}` : ` at instruction ${error.context}`;
      return { error, ...(step ? { step } : {}), message: `${error.name}${where}` };
    }
  }
}

function runtimeAccountName(compiled: CompiledTemplate, index: number): { index: number; name: string; row?: number } {
  const fixed = compiled.fixedAccountOrder;
  if (index < fixed.length) return { index, name: fixed[index]! };
  const stride = compiled.batchAccountOrder.length;
  if (stride === 0) return { index, name: `account[${index}]` };
  const offset = index - fixed.length;
  return { index, name: compiled.batchAccountOrder[offset % stride]!, row: Math.floor(offset / stride) };
}
