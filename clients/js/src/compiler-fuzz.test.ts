/**
 * Compiler fuzzing. A seeded generator writes template documents that `defineTemplate` accepts and
 * that cover the language: inputs of every type; fixed, row and registry accounts; account groups;
 * arithmetic, comparisons, casts, selects, account reads at fixed and run-time offsets, the clock,
 * PDAs, introspection and return data; and `let`, `require`, `invoke` with `when`, `assign` in
 * loops, `forEach`, `repeat`, `emit`, `setReturnData` and `setRegistry`. Sizes run from a few
 * values to past 64 registers, so `reuseRegisters` runs on its own as well as when forced.
 *
 * Each document is checked here against these properties:
 *
 * 1. **Compiles or rejects cleanly.** `compileTemplate` returns bytes, or throws a Zod error or
 *    one of its own messages: never an internal error such as a TypeError from undefined access,
 *    a writer's bare `Expected u8`, a stack overflow, or the reuse replay's "compiler bug".
 * 2. **Deterministic.** Compiling twice, compiling the parsed template again, and compiling a
 *    copy whose names are all renamed (some to names such as `constructor`) give the same bytes.
 * 3. **Register reuse preserves meaning.** A renumbered program (one past 64 values, or any with
 *    `registerReuse: 'always'`) is the one-register-per-value program (`registerReuse: 'never'`
 *    past 64) with its registers renamed, and on every path through its loops each register read
 *    sees the value the plain program's read sees. The replay here decodes the bytes itself and
 *    takes its register operands from the verifier (`common/src/template/verify.rs`), not from the
 *    compiler's own table, and models the runtime's per-pass snapshot (`execute.rs`).
 *
 * The rest runs in Rust, from the corpus this file writes: `common/tests/compiler_corpus.rs`
 * requires the verifier to accept every payload, and `tests/ballista` (Mollusk) runs each case's
 * plain and forced-reuse programs, plus a copy whose `let` aliases are materialized, on the same
 * accounts and inputs, and requires the same outcome.
 *
 * Commands (from the repository root):
 *
 * - More seeds: `FUZZ_SEEDS=5000 FUZZ_SEED_START=1 pnpm --dir clients/js exec vitest run src/compiler-fuzz.test.ts`
 * - Write a corpus for the Rust harnesses: add `FUZZ_CORPUS=/absolute/path/corpus.json`, then run
 *   `COMPILER_FUZZ_CORPUS=/absolute/path/corpus.json cargo test --manifest-path tests/ballista/Cargo.toml compiler_fuzz -- --nocapture`
 * - Rewrite the small committed corpus (`fixtures/compiler-fuzz-corpus.json`):
 *   `UPDATE_COMPILER_FUZZ_CORPUS=1 pnpm --dir clients/js exec vitest run src/compiler-fuzz.test.ts`
 */
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import { describe, expect, test } from 'vitest';
import { ZodError } from 'zod';

import {
  account,
  addressBytes,
  compileTemplate,
  data,
  encodeRunInputs,
  expression,
  INSTRUCTIONS_SYSVAR_ADDRESS_BYTES,
  inspectTemplate,
  opcode,
  step,
  SYSTEM_PROGRAM_ADDRESS_BYTES,
  TOKEN_PROGRAM_ADDRESS_BYTES,
  type AccountConstraintInput,
  type AccountReference,
  type CompiledTemplate,
  type CompileOptions,
  type DataPart,
  type Expression,
  type InputDefinition,
  type ReadType,
  type RegistryFieldType,
  type RunInputValue,
  type Step,
  type TemplateInput,
} from './index.js';

// ---------------------------------------------------------------------------------------------
// Seeded randomness
// ---------------------------------------------------------------------------------------------

/** mulberry32: a small seeded generator, so a seed reproduces its document on any machine. */
export class Rng {
  private state: number;

  constructor(seed: number) {
    this.state = (seed ^ 0x9e37_79b9) >>> 0;
  }

  u32(): number {
    this.state = (this.state + 0x6d2b_79f5) >>> 0;
    let t = this.state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return (t ^ (t >>> 14)) >>> 0;
  }

  /** An integer in `[0, n)`. */
  below(n: number): number {
    return n <= 0 ? 0 : Math.floor((this.u32() / 0x1_0000_0000) * n);
  }

  /** An integer in `[low, high]`. */
  range(low: number, high: number): number {
    return low + this.below(high - low + 1);
  }

  chance(probability: number): boolean {
    return this.u32() / 0x1_0000_0000 < probability;
  }

  pick<T>(items: readonly T[]): T {
    if (items.length === 0) throw new Error('pick from an empty list');
    return items[this.below(items.length)]!;
  }

  bytes(length: number): Uint8Array<ArrayBuffer> {
    return Uint8Array.from({ length }, () => this.below(256));
  }

  bits(width: number): bigint {
    let value = 0n;
    for (let filled = 0; filled < width; filled += 32) value = (value << 32n) | BigInt(this.u32());
    return BigInt.asUintN(width, value);
  }

  weighted<T>(entries: readonly (readonly [number, T])[]): T {
    const total = entries.reduce((sum, [weight]) => sum + weight, 0);
    let roll = this.u32() / 0x1_0000_0000 * total;
    for (const [weight, value] of entries) {
      if (roll < weight) return value;
      roll -= weight;
    }
    return entries[entries.length - 1]![1];
  }

  shuffle<T>(items: T[]): T[] {
    for (let index = items.length - 1; index > 0; index -= 1) {
      const other = this.below(index + 1);
      [items[index], items[other]] = [items[other]!, items[index]!];
    }
    return items;
  }
}

// ---------------------------------------------------------------------------------------------
// The world a generated template runs in
// ---------------------------------------------------------------------------------------------

const MEMO_PROGRAM_ADDRESS_BYTES = addressBytes('MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr');
const PROGRAM_ADDRESSES = {
  system: SYSTEM_PROGRAM_ADDRESS_BYTES,
  memo: MEMO_PROGRAM_ADDRESS_BYTES,
  token: TOKEN_PROGRAM_ADDRESS_BYTES,
} as const;
type ProgramName = keyof typeof PROGRAM_ADDRESSES;

/** The memo instruction before every run, so introspection has a neighbour to read. */
export const MEMO_BEFORE = new TextEncoder().encode('compiler-fuzz');
/** A Token program mint is 82 bytes: authority option, supply, decimals, flag, freeze option. */
const MINT_LENGTH = 82;
/** What `GetAccountDataSize` (Token instruction 21) returns for a mint: a token account's size. */
const TOKEN_ACCOUNT_SIZE_RETURN = 8;

/** How the Rust harness finds or builds one runtime account. */
export type AccountSource =
  | { kind: 'program'; program: ProgramName }
  | { kind: 'sysvar' }
  | { kind: 'registry'; index: number; key: Uint8Array }
  | { kind: 'stored'; address: Uint8Array<ArrayBuffer>; lamports: bigint; owner: Uint8Array; data: Uint8Array };

export interface WorldAccount {
  role: string;
  signer: boolean;
  writable: boolean;
  source: AccountSource;
}

type ValueKind = 'bool' | 'u64' | 'i64' | 'u128' | 'pubkey' | 'bytes';

/** A generated expression and, for `bytes`, the most bytes it can hold. */
interface Gen {
  e: Expression;
  type: ValueKind;
  max: number;
}

/** What a numeric binding is known to hold, so arithmetic can read it directly and stay in range. */
type Range = 'small' | 'nonzero' | 'shift';

interface Binding {
  type: ValueKind;
  max: number;
  range?: Range;
}

interface Scope {
  loop: 'rows' | 'count' | undefined;
  vars: Map<string, Binding>;
  /** The carried variables the body may assign. */
  carried: Map<string, Binding>;
}

type AccountKind = 'program' | 'sysvar' | 'wallet' | 'data' | 'mint' | 'entry';

interface AccountDecl {
  name: string;
  kind: AccountKind;
  program?: ProgramName;
  constraint: AccountConstraintInput;
  /** The data the world account holds; fixed-offset reads stay inside it. */
  dataLength: number;
  /** May be read as data: pinned by owner or address, or `unsafeUnpinned`, and not an entry. */
  readable: boolean;
  writable: boolean;
  signer: boolean;
  registry?: { name: string; index: number; fields: [string, RegistryFieldType][] };
  /** A data account's owner in the world, and whether its constraint pins its address instead. */
  owner?: Uint8Array<ArrayBuffer>;
  pinAddress?: boolean;
  /** Filled in when the world is built. */
  address?: Uint8Array<ArrayBuffer>;
  world?: WorldAccount;
  /** A registry entry's key as the world knows it, once the fixed accounts have addresses. */
  resolveKey?: () => Uint8Array;
  /** How the entry's key is chosen; two entries of one registry with the same choice collide. */
  keyChoice?: string;
}

/** One generated case: the document, how to run it, and how it was sized. */
export interface FuzzCase {
  seed: number;
  size: 'small' | 'medium' | 'large';
  template: TemplateInput;
  accounts: WorldAccount[];
  inputs: Record<string, RunInputValue>;
  rows: Record<string, RunInputValue>[];
  groupLengths: number[];
}

// ---------------------------------------------------------------------------------------------
// Expression cost, to keep documents under the instruction limit
// ---------------------------------------------------------------------------------------------

function literalKeyOf(expressionNode: Extract<Expression, { kind: 'literal' }>): string {
  const inner: unknown = expressionNode.value.value;
  if (inner instanceof Uint8Array) return `${expressionNode.value.type}:${[...inner].join(',')}`;
  return `${expressionNode.value.type}:${String(inner)}`;
}

/** The instructions an expression compiles to, given the literals and inputs already loaded. */
function expressionCost(node: Expression, literals: Set<string>, inputs: Set<string>): number {
  switch (node.kind) {
    case 'variable':
      return 0;
    case 'literal': {
      const key = literalKeyOf(node);
      if (literals.has(key)) return 0;
      literals.add(key);
      return 1;
    }
    case 'input':
      if (inputs.has(node.name)) return 0;
      inputs.add(node.name);
      return 1;
    case 'accountData':
      return 1 + (typeof node.offset === 'number' ? 0 : expressionCost(node.offset, literals, inputs));
    case 'pda':
      return (
        1 +
        node.seeds.reduce((sum, seed) => sum + expressionCost(seed, literals, inputs), 0) +
        (node.bump ? expressionCost(node.bump, literals, inputs) : 0)
      );
    case 'binary':
      return 1 + expressionCost(node.left, literals, inputs) + expressionCost(node.right, literals, inputs);
    case 'multiplyDivide':
      return (
        1 +
        expressionCost(node.left, literals, inputs) +
        expressionCost(node.right, literals, inputs) +
        expressionCost(node.divisor, literals, inputs)
      );
    case 'powerOfTen':
      return 1 + expressionCost(node.exponent, literals, inputs);
    case 'not':
    case 'cast':
    case 'bytesLength':
      return 1 + expressionCost(node.value, literals, inputs);
    case 'select':
      return (
        1 +
        expressionCost(node.condition, literals, inputs) +
        expressionCost(node.ifTrue, literals, inputs) +
        expressionCost(node.ifFalse, literals, inputs)
      );
    case 'instruction':
      return 1 + expressionCost(node.index, literals, inputs);
    case 'instructionAccount':
      return 1 + expressionCost(node.index, literals, inputs) + expressionCost(node.position, literals, inputs);
    case 'instructionData':
    case 'instructionDataBytes':
      return 1 + expressionCost(node.index, literals, inputs) + expressionCost(node.offset, literals, inputs);
    case 'accountDataBytes':
      return 1 + expressionCost(node.offset, literals, inputs);
    default:
      return 1;
  }
}

function partsCost(parts: DataPart[], literals: Set<string>, inputs: Set<string>): number {
  return parts.reduce((sum, part) => sum + (part.kind === 'encoded' ? expressionCost(part.value, literals, inputs) : 0), 0);
}

/** The instructions one step compiles to, loop bodies included; carried copies are counted as one each. */
function stepCost(current: Step, literals: Set<string>, inputs: Set<string>): number {
  switch (current.kind) {
    case 'let':
      return expressionCost(current.value, literals, inputs);
    case 'assign':
      return 1 + expressionCost(current.value, literals, inputs);
    case 'require':
      return 1 + expressionCost(current.condition, literals, inputs);
    case 'invoke':
      return 1 + partsCost(current.data, literals, inputs) + (current.when ? expressionCost(current.when, literals, inputs) : 0);
    case 'emit':
    case 'setReturnData':
      return 1 + partsCost(current.parts, literals, inputs);
    case 'setRegistry':
      return 1 + expressionCost(current.value, literals, inputs);
    case 'forEach':
      return 1 + (current.carry?.length ?? 0) + current.steps.reduce((sum, inner) => sum + stepCost(inner, literals, inputs), 0);
    case 'repeat':
      return (
        1 +
        (current.carry?.length ?? 0) +
        expressionCost(current.count, literals, inputs) +
        current.steps.reduce((sum, inner) => sum + stepCost(inner, literals, inputs), 0)
      );
  }
}

// ---------------------------------------------------------------------------------------------
// The generator
// ---------------------------------------------------------------------------------------------

const ALL_KINDS: readonly ValueKind[] = ['bool', 'u64', 'i64', 'u128', 'pubkey', 'bytes'];
const REGISTRY_TYPES: readonly RegistryFieldType[] = ['bool', 'u64', 'i64', 'u128', 'pubkey'];
const READ_WIDTH: Record<ReadType, number> = { bool: 1, u8: 1, u16: 2, u32: 4, i32: 4, u64: 8, i64: 8, u128: 16, pubkey: 32 };
const READS_BY_KIND: Record<Exclude<ValueKind, 'bytes'>, readonly ReadType[]> = {
  bool: ['bool'],
  u64: ['u8', 'u16', 'u32', 'u64'],
  i64: ['i64', 'i32'],
  u128: ['u128'],
  pubkey: ['pubkey'],
};

const u64 = (value: bigint | number) => expression.u64(value);
const i64 = (value: bigint | number) => expression.i64(value);
const u128 = (value: bigint | number) => expression.u128(value);
const fixed = (name: string) => account.fixed(name);
const row = (name: string) => account.iteration(name);

/** Generates one document and the world it runs in. Every choice comes from the seed. */
export class Generator {
  readonly rng: Rng;
  readonly seed: number;
  readonly size: FuzzCase['size'];
  /** The instruction budget the steps aim for. */
  readonly limit: number;
  instructions = 0;
  cpis = 0;
  readonly literals = new Set<string>();
  readonly usedInputs = new Set<string>();
  names = 0;
  balances = 0;

  inputs: Record<string, InputDefinition> = {};
  inputValues: Record<string, RunInputValue> = {};
  rowInputs: Record<string, InputDefinition> = {};
  rowValues: Record<string, RunInputValue>[] = [];
  fixedDecls: AccountDecl[] = [];
  rowDecls: AccountDecl[] = [];
  registries: Record<string, Record<string, RegistryFieldType>> = {};
  groups: string[] = [];
  groupLengths: number[] = [];
  batch: { maxIterations: number; minIterations: number; iterations: number } | undefined;
  emitEvent = false;
  /** Addresses of the fixed accounts the world stores, for keys and literals. */
  readonly knownAddresses: Uint8Array[] = [];
  /** The run instruction's account count, the template's included, and its data length. */
  runAccounts = 0;
  runDataLength = 0;

  constructor(seed: number) {
    this.seed = seed;
    this.rng = new Rng(seed);
    this.size = this.rng.weighted([
      [3, 'small'],
      [4, 'medium'],
      [3, 'large'],
    ] as const);
    this.limit =
      this.size === 'small' ? this.rng.range(6, 40) : this.size === 'medium' ? this.rng.range(40, 92) : this.rng.range(92, 124);
  }

  name(prefix: string): string {
    const name = `${prefix}${this.names}`;
    this.names += 1;
    return name;
  }

  // -- values ----------------------------------------------------------------------------------

  u64Value(): bigint {
    return this.rng.weighted([
      [5, BigInt(this.rng.below(20))],
      [4, BigInt(this.rng.below(100_000))],
      [2, this.rng.bits(32)],
      [1, this.rng.bits(64)],
      [1, this.rng.pick([0n, 1n, (1n << 63n) - 1n, 1n << 63n, (1n << 64n) - 1n])],
    ]);
  }

  i64Value(): bigint {
    return this.rng.weighted([
      [5, BigInt(this.rng.range(-20, 20))],
      [4, BigInt(this.rng.range(-100_000, 100_000))],
      [1, BigInt.asIntN(64, this.rng.bits(64))],
      [1, this.rng.pick([0n, -1n, -(1n << 63n), (1n << 63n) - 1n])],
    ]);
  }

  u128Value(): bigint {
    return this.rng.weighted([
      [5, BigInt(this.rng.below(1000))],
      [3, this.rng.bits(64)],
      [1, this.rng.bits(128)],
      [1, this.rng.pick([0n, 1n, (1n << 128n) - 1n, 1n << 64n])],
    ]);
  }

  pubkeyValue(): Uint8Array {
    if (this.knownAddresses.length > 0 && this.rng.chance(0.4)) return this.rng.pick(this.knownAddresses).slice();
    if (this.rng.chance(0.1)) return new Uint8Array(32);
    return this.rng.bytes(32);
  }

  inputValue(definition: InputDefinition): RunInputValue {
    switch (definition.type) {
      case 'bool':
        return this.rng.chance(0.5);
      case 'u64':
        return this.u64Value();
      case 'i64':
        return this.i64Value();
      case 'u128':
        return this.u128Value();
      case 'pubkey':
        return this.pubkeyValue();
      case 'bytes':
        return this.rng.bytes(this.rng.range(0, definition.maxLength));
    }
  }

  inputDefinition(): InputDefinition {
    const type = this.rng.pick(ALL_KINDS);
    return type === 'bytes' ? { type, maxLength: this.rng.range(1, 40) } : { type };
  }

  // -- declarations ----------------------------------------------------------------------------

  declare(): void {
    const rng = this.rng;
    for (let index = 0, count = rng.weighted([[2, 0], [4, rng.range(1, 3)], [3, rng.range(3, 7)]]); index < count; index += 1) {
      this.inputs[this.name('in')] = this.inputDefinition();
    }
    if (rng.chance(0.4)) {
      const maxIterations = rng.range(1, 4);
      const minIterations = rng.range(0, maxIterations);
      this.batch = { maxIterations, minIterations, iterations: rng.range(minIterations, maxIterations) };
      for (let index = 0, count = rng.weighted([[3, 0], [3, 1], [2, rng.range(2, 3)]]); index < count; index += 1) {
        this.rowInputs[this.name('rin')] = this.inputDefinition();
      }
    }

    // Fixed accounts. A payer always exists: it signs transfers and pays registry rent.
    const decls: AccountDecl[] = [];
    decls.push(this.walletDecl('payer', true, true));
    if (rng.chance(0.85)) decls.push(this.programDecl('system', true));
    if (rng.chance(0.4)) decls.push(this.programDecl('memo', rng.chance(0.85)));
    if (rng.chance(0.35)) {
      decls.push(this.programDecl('token', true));
      decls.push(this.mintDecl());
    }
    if (rng.chance(0.4)) {
      decls.push({
        name: this.name('sysvar'),
        kind: 'sysvar',
        constraint: { address: INSTRUCTIONS_SYSVAR_ADDRESS_BYTES },
        dataLength: 0,
        readable: false,
        writable: false,
        signer: false,
      });
    }
    for (let index = 0, count = rng.range(0, 3); index < count; index += 1) {
      decls.push(this.walletDecl('wallet', rng.chance(0.7), rng.chance(0.1)));
    }
    for (let index = 0, count = rng.weighted([[2, 0], [3, 1], [2, rng.range(2, 3)]]); index < count; index += 1) {
      decls.push(this.dataDecl('data'));
    }
    if (rng.chance(0.1)) decls.push(this.programDecl(rng.pick(['system', 'memo', 'token'] as const), false));
    rng.shuffle(decls);

    // Registries and their entries, which keep their relative order: a key may read an earlier one.
    if (rng.chance(0.3)) {
      if (!decls.some((decl) => decl.kind === 'program' && decl.program === 'system' && decl.constraint.address)) {
        decls.splice(rng.below(decls.length + 1), 0, this.programDecl('system', true));
      }
      const registryNames: string[] = [];
      for (let index = 0, count = rng.range(1, 2); index < count; index += 1) {
        const name = this.name('reg');
        const fields: Record<string, RegistryFieldType> = {};
        for (let field = 0, fieldCount = rng.range(1, 4); field < fieldCount; field += 1) {
          fields[this.name('f')] = rng.pick(REGISTRY_TYPES);
        }
        this.registries[name] = fields;
        registryNames.push(name);
      }
      const payer = decls.find((decl) => decl.kind === 'wallet' && decl.signer && decl.writable)!;
      const entries: AccountDecl[] = [];
      for (let index = 0, count = rng.range(1, 3); index < count; index += 1) {
        const registryName = rng.pick(registryNames);
        // Two entries of one registry with equal keys are one account, and the second open fails.
        // Keep that rare, so most runs get past the opens.
        for (let attempt = 0; attempt < 8; attempt += 1) {
          const entry = this.entryDecl(registryName, registryNames.indexOf(registryName), payer, entries);
          const clash = entries.some((other) => other.registry!.name === registryName && other.keyChoice === entry.keyChoice && entry.keyChoice !== 'literal');
          if (!clash || attempt === 7 || rng.chance(0.05)) {
            entries.push(entry);
            break;
          }
        }
      }
      let position = 0;
      for (const entry of entries) {
        position = rng.range(position, decls.length);
        decls.splice(position, 0, entry);
        position += 1;
      }
    }
    this.fixedDecls = decls;

    if (this.batch) {
      for (let index = 0, count = rng.range(1, 3); index < count; index += 1) {
        this.rowDecls.push(
          rng.weighted<AccountDecl>([
            [4, this.walletDecl('rw', true, false)],
            [3, this.dataDecl('rd', true)],
            [1, this.walletDecl('rr', false, false)],
            [1, this.programDecl(rng.pick(['system', 'memo', 'token'] as const), rng.chance(0.5))],
          ]),
        );
      }
    }
    if (rng.chance(0.2)) {
      for (let index = 0, count = rng.range(1, 2); index < count; index += 1) {
        this.groups.push(this.name('group'));
        this.groupLengths.push(rng.range(0, 3));
      }
    }
    this.emitEvent = rng.chance(0.2);
    for (const [name, definition] of Object.entries(this.inputs)) this.inputValues[name] = this.inputValue(definition);
    for (let iteration = 0; iteration < (this.batch?.iterations ?? 0); iteration += 1) {
      const values: Record<string, RunInputValue> = {};
      for (const [name, definition] of Object.entries(this.rowInputs)) values[name] = this.inputValue(definition);
      this.rowValues.push(values);
    }
    const groupTotal = this.groupLengths.reduce((sum, length) => sum + length, 0);
    this.runAccounts = 1 + this.fixedDecls.length + this.rowDecls.length * (this.batch?.iterations ?? 0) + groupTotal;
    const inputLength = (definition: InputDefinition, value: RunInputValue) =>
      definition.type === 'bytes' ? 2 + (value as Uint8Array).length : { bool: 1, u64: 8, i64: 8, u128: 16, pubkey: 32 }[definition.type];
    this.runDataLength =
      1 +
      this.groups.length +
      Object.entries(this.inputs).reduce((sum, [name, definition]) => sum + inputLength(definition, this.inputValues[name]!), 0) +
      this.rowValues.reduce(
        (sum, values) => sum + Object.entries(this.rowInputs).reduce((inner, [name, definition]) => inner + inputLength(definition, values[name]!), 0),
        0,
      );
  }

  walletDecl(prefix: string, writable: boolean, signer: boolean): AccountDecl {
    return {
      name: prefix === 'payer' ? this.name('payer') : this.name(prefix),
      kind: 'wallet',
      constraint: { writable, signer },
      dataLength: 0,
      readable: false,
      writable,
      signer,
    };
  }

  programDecl(program: ProgramName, pinned: boolean): AccountDecl {
    return {
      name: this.name(program),
      kind: 'program',
      program,
      constraint: pinned ? { executable: true, address: PROGRAM_ADDRESSES[program] } : { executable: true, unsafeUnpinned: true },
      dataLength: 0,
      readable: false,
      writable: false,
      signer: false,
    };
  }

  mintDecl(): AccountDecl {
    return {
      name: this.name('mint'),
      kind: 'mint',
      constraint: { owner: TOKEN_PROGRAM_ADDRESS_BYTES, minDataLength: this.rng.chance(0.5) ? MINT_LENGTH : 0 },
      dataLength: MINT_LENGTH,
      readable: true,
      writable: false,
      signer: false,
    };
  }

  /** A data account; a row's is pinned by owner or not at all, since an address pin is one account. */
  dataDecl(prefix: string, rowAccount = false): AccountDecl {
    const rng = this.rng;
    const writable = rng.chance(0.25);
    const dataLength = rng.range(8, 96);
    const pin = rng.weighted([
      [6, 'owner'],
      [rowAccount ? 0 : 2, 'address'],
      [2, 'unpinned'],
    ] as const);
    const owner = rng.bytes(32);
    const constraint: AccountConstraintInput = { writable, minDataLength: rng.chance(0.3) ? rng.range(0, dataLength) : 0 };
    if (pin === 'owner') constraint.owner = owner;
    if (pin === 'unpinned') constraint.unsafeUnpinned = true;
    // An address pin is filled in when the world account exists.
    return {
      name: this.name(prefix),
      kind: 'data',
      constraint,
      dataLength,
      readable: true,
      writable,
      signer: false,
      owner,
      pinAddress: pin === 'address',
    };
  }

  entryDecl(registryName: string, index: number, payer: AccountDecl, earlier: AccountDecl[]): AccountDecl {
    const rng = this.rng;
    const fields = Object.entries(this.registries[registryName]!);
    const name = this.name('entry');
    const pubkeyInputs = Object.entries(this.inputs).filter(([, definition]) => definition.type === 'pubkey');
    const earlierPubkeyFields = earlier.flatMap((entry) =>
      entry.registry!.fields.filter(([, type]) => type === 'pubkey').map(([field]) => [entry.name, field] as const),
    );
    let key: Expression | undefined;
    // The entry's key as the world knows it: inputs and account keys are known, a field of an
    // entry this run creates starts at zero, and no key is the zero key.
    let resolved: Uint8Array | (() => Uint8Array) = new Uint8Array(32);
    const choice = rng.weighted([
      [3, 'none'],
      [3, 'payer'],
      [pubkeyInputs.length > 0 ? 2 : 0, 'input'],
      [2, 'literal'],
      [earlierPubkeyFields.length > 0 ? 1 : 0, 'field'],
    ] as const);
    if (choice === 'payer') {
      key = expression.accountKey(payer.name);
      resolved = () => payer.address!;
    } else if (choice === 'input') {
      const [inputName] = rng.pick(pubkeyInputs);
      key = expression.input(inputName);
      resolved = () => this.inputValues[inputName] as Uint8Array;
    } else if (choice === 'literal') {
      const value = rng.bytes(32);
      key = expression.pubkey(value);
      resolved = value;
    } else if (choice === 'field') {
      const [entryName, field] = rng.pick(earlierPubkeyFields);
      key = expression.registry(entryName, field);
    }
    return {
      name,
      kind: 'entry',
      constraint: account.registry(registryName, { ...(key ? { key } : {}), payer: payer.name }),
      dataLength: 0,
      readable: false,
      writable: true,
      signer: false,
      registry: { name: registryName, index, fields },
      resolveKey: () => (typeof resolved === 'function' ? resolved() : resolved),
      keyChoice: choice === 'input' || choice === 'field' ? `${choice}:${key && 'name' in key ? String(key.name) : ''}${key && 'field' in key ? String(key.field) : ''}` : choice,
    };
  }

  // -- world -----------------------------------------------------------------------------------

  /** The world account for a declaration; stored accounts get a fresh address. */
  worldAccount(decl: AccountDecl, iteration?: number): WorldAccount {
    const rng = this.rng;
    const role = iteration === undefined ? decl.name : `${decl.name}[${iteration}]`;
    const base = { role, signer: decl.signer, writable: decl.writable };
    switch (decl.kind) {
      case 'program':
        return { ...base, source: { kind: 'program', program: decl.program! } };
      case 'sysvar':
        return { ...base, source: { kind: 'sysvar' } };
      case 'entry':
        return { ...base, source: { kind: 'registry', index: decl.registry!.index, key: decl.resolveKey!() } };
      case 'mint': {
        const mint = new Uint8Array(MINT_LENGTH);
        const view = new DataView(mint.buffer);
        view.setBigUint64(36, BigInt(rng.below(1_000_000_000)), true);
        mint[44] = rng.range(0, 9);
        mint[45] = 1;
        return {
          ...base,
          source: { kind: 'stored', address: rng.bytes(32), lamports: this.lamports(), owner: TOKEN_PROGRAM_ADDRESS_BYTES, data: mint },
        };
      }
      case 'data': {
        // Mostly zeros and ones, so a `bool` read usually succeeds and numbers stay small.
        // Some zeros and ones, so a `bool` read can succeed; otherwise varied, so a read of the
        // wrong offset or account gives a different value.
        const contents = Uint8Array.from({ length: decl.dataLength }, () => rng.weighted([[2, 0], [2, 1], [2, rng.below(16)], [5, rng.below(256)]]));
        return {
          ...base,
          source: { kind: 'stored', address: rng.bytes(32), lamports: this.lamports(), owner: decl.owner!, data: contents },
        };
      }
      case 'wallet':
        return {
          ...base,
          source: {
            kind: 'stored',
            address: rng.bytes(32),
            lamports: decl.signer && decl.writable ? 10_000_000_000n + this.lamports() : this.lamports(),
            owner: SYSTEM_PROGRAM_ADDRESS_BYTES,
            data: new Uint8Array(0),
          },
        };
    }
  }

  /** A balance no other account in the world has, so reading the wrong account's shows. */
  lamports(): bigint {
    this.balances += 1;
    return 1_000_000_000n + BigInt(this.balances) * 7_919n + BigInt(this.rng.below(7_919));
  }

  /** Builds the fixed accounts' world first, so keys, pins and literals can name their addresses. */
  buildFixedWorld(): WorldAccount[] {
    const world: WorldAccount[] = [];
    for (const decl of this.fixedDecls) {
      if (decl.kind === 'entry') continue;
      decl.world = this.worldAccount(decl);
      if (decl.world.source.kind === 'stored') {
        decl.address = decl.world.source.address;
        this.knownAddresses.push(decl.address);
        if (decl.pinAddress) decl.constraint.address = decl.address;
      }
    }
    // Entries last: their keys can name the payer's address.
    for (const decl of this.fixedDecls) {
      if (decl.kind === 'entry') decl.world = this.worldAccount(decl);
      world.push(decl.world!);
    }
    return world;
  }

  // -- expressions -----------------------------------------------------------------------------

  literal(type: ValueKind): Gen {
    const rng = this.rng;
    switch (type) {
      case 'bool':
        return { e: expression.bool(rng.chance(0.5)), type, max: 0 };
      case 'u64':
        return { e: u64(this.u64Value()), type, max: 0 };
      case 'i64':
        return { e: i64(this.i64Value()), type, max: 0 };
      case 'u128':
        return { e: u128(this.u128Value()), type, max: 0 };
      case 'pubkey':
        return { e: expression.pubkey(this.pubkeyValue()), type, max: 0 };
      case 'bytes': {
        const value = rng.bytes(rng.weighted([[1, 0], [6, rng.range(1, 12)], [1, rng.range(13, 40)]]));
        return { e: expression.bytes(value), type, max: value.length };
      }
    }
  }

  /** A small literal, for the wrappers that keep arithmetic in range. */
  small(type: 'u64' | 'i64' | 'u128', value: bigint): Expression {
    return type === 'u64' ? u64(value) : type === 'i64' ? i64(value) : u128(value);
  }

  accountsOf(scope: Scope, filter: (decl: AccountDecl) => boolean): { reference: AccountReference; decl: AccountDecl }[] {
    const found = this.fixedDecls.filter(filter).map((decl) => ({ reference: fixed(decl.name), decl }));
    if (scope.loop === 'rows') found.push(...this.rowDecls.filter(filter).map((decl) => ({ reference: row(decl.name), decl })));
    return found;
  }

  inputsOf(type: ValueKind, rowInputs: boolean): [string, InputDefinition][] {
    return Object.entries(rowInputs ? this.rowInputs : this.inputs).filter(([, definition]) => definition.type === type);
  }

  registryFieldsOf(type: ValueKind): [string, string][] {
    return this.fixedDecls
      .filter((decl) => decl.kind === 'entry')
      .flatMap((decl) => decl.registry!.fields.filter(([, fieldType]) => fieldType === type).map(([field]) => [decl.name, field] as [string, string]));
  }

  sysvar(): string | undefined {
    return this.fixedDecls.find((decl) => decl.kind === 'sysvar')?.name;
  }

  /** A value with no subexpression: a literal, an input, a variable, or a read that takes no operand. */
  leaf(scope: Scope, type: ValueKind): Gen {
    const rng = this.rng;
    const options: [number, () => Gen][] = [[3, () => this.literal(type)]];
    const inputs = this.inputsOf(type, false);
    if (inputs.length > 0) {
      options.push([3, () => {
        const [name, definition] = rng.pick(inputs);
        return { e: expression.input(name), type, max: definition.type === 'bytes' ? definition.maxLength : 0 };
      }]);
    }
    if (scope.loop === 'rows') {
      const rowInputs = this.inputsOf(type, true);
      if (rowInputs.length > 0) {
        options.push([3, () => {
          const [name, definition] = rng.pick(rowInputs);
          return { e: expression.rowInput(name), type, max: definition.type === 'bytes' ? definition.maxLength : 0 };
        }]);
      }
    }
    const variables = [...scope.vars].filter(([, binding]) => binding.type === type);
    if (variables.length > 0) {
      // Half the time one of the oldest: a value made early and read late keeps its register
      // longest, which is what register reuse must get right.
      options.push([6, () => {
        const pool = rng.chance(0.5) ? variables.slice(0, Math.max(1, Math.ceil(variables.length / 3))) : variables;
        const [name, binding] = rng.pick(pool);
        return { e: expression.variable(name), type, max: binding.max };
      }]);
    }
    const fields = this.registryFieldsOf(type);
    if (fields.length > 0) {
      options.push([2, () => {
        const [entry, field] = rng.pick(fields);
        return { e: expression.registry(entry, field), type, max: 0 };
      }]);
    }
    const anyAccount = this.accountsOf(scope, () => true);
    const sysvar = this.sysvar();
    if (type === 'u64') {
      options.push([2, () => ({ e: expression.accountField(rng.pick(anyAccount).reference, rng.pick(['lamports', 'dataLength'] as const)), type, max: 0 })]);
      options.push([1, () => ({ e: expression.clockSlot(), type, max: 0 })]);
      if (scope.loop) options.push([3, () => ({ e: expression.loopIndex(), type, max: 0 })]);
      if (sysvar) {
        options.push([1, () => ({
          e: rng.chance(0.5) ? expression.instructionCount(fixed(sysvar)) : expression.currentInstructionIndex(fixed(sysvar)),
          type,
          max: 0,
        })]);
      }
    } else if (type === 'i64') {
      options.push([1, () => ({ e: expression.clockUnixTimestamp(), type, max: 0 })]);
    } else if (type === 'bool') {
      options.push([2, () => ({ e: expression.accountField(rng.pick(anyAccount).reference, 'isEmpty'), type, max: 0 })]);
    } else if (type === 'pubkey') {
      options.push([3, () => ({ e: expression.accountField(rng.pick(anyAccount).reference, rng.pick(['key', 'owner'] as const)), type, max: 0 })]);
    }
    return rng.weighted(options)();
  }

  expr(scope: Scope, type: ValueKind, depth: number): Gen {
    if (depth <= 0 || this.rng.chance(0.3)) return this.leaf(scope, type);
    switch (type) {
      case 'u64':
      case 'i64':
      case 'u128':
        return this.numeric(scope, type, depth);
      case 'bool':
        return this.boolean(scope, depth);
      case 'pubkey':
        return this.pubkey(scope, depth);
      case 'bytes':
        return this.bytesValue(scope, depth);
    }
  }

  /**
   * An operand known to be in `range`: half the time a binding made to hold such a value, read
   * directly, otherwise a fresh expression clamped into it. `small` keeps a sum, product or shift
   * in range, `nonzero` a divisor, and `shift` an amount.
   */
  operand(scope: Scope, type: 'u64' | 'i64' | 'u128', range: Range, depth: number): Expression {
    const tagged = [...scope.vars].filter(([, binding]) => binding.type === type && binding.range === range);
    if (tagged.length > 0 && this.rng.chance(0.5)) {
      const pool = this.rng.chance(0.5) ? tagged.slice(0, Math.max(1, Math.ceil(tagged.length / 2))) : tagged;
      return expression.variable(this.rng.pick(pool)[0]);
    }
    return this.rangeExpression(scope, type, range, depth);
  }

  rangeExpression(scope: Scope, type: 'u64' | 'i64' | 'u128', range: Range, depth: number): Expression {
    const value = this.expr(scope, type, depth - 1).e;
    if (range === 'nonzero') return expression.max(value, this.small(type, 1n));
    if (range === 'shift') return this.clamp(type, value, 32n);
    return this.clamp(type, value, type === 'u128' ? 1n << 62n : 1n << 31n);
  }

  /** `value % modulus` in `type`, the usual way to keep a value small enough to stay in range. */
  clamp(type: 'u64' | 'i64' | 'u128', value: Expression, modulus: bigint): Expression {
    return expression.remainder(value, this.small(type, modulus));
  }

  numeric(scope: Scope, type: 'u64' | 'i64' | 'u128', depth: number): Gen {
    const rng = this.rng;
    const safe = rng.chance(0.85);
    const sub = (kind: ValueKind = type) => this.expr(scope, kind, depth - 1).e;
    const g = (e: Expression): Gen => ({ e, type, max: 0 });
    const halfWidth = type === 'u128' ? 1n << 62n : 1n << 31n;
    const options: [number, () => Gen][] = [
      [6, () => {
        const op = rng.pick(['add', 'subtract', 'multiply', 'divide', 'remainder', 'min', 'max'] as const);
        if (!safe) return g(expression[op](sub(), sub()));
        const small = () => this.operand(scope, type, 'small', depth);
        switch (op) {
          case 'add':
            return g(expression.add(small(), small()));
          case 'subtract': {
            if (type === 'i64') return g(expression.subtract(small(), small()));
            const [left, right] = [sub(), sub()];
            return g(expression.subtract(expression.max(left, right), expression.min(left, right)));
          }
          case 'multiply':
            return g(expression.multiply(small(), small()));
          case 'divide':
          case 'remainder':
            return g(expression[op](sub(), this.operand(scope, type, 'nonzero', depth)));
          default:
            return g(expression[op](sub(), sub()));
        }
      }],
      [2, () => g(expression.select(this.expr(scope, 'bool', depth - 1).e, sub(), sub()))],
      [2, () => {
        const from = rng.pick(['u64', 'i64', 'u128'] as const);
        let value = sub(from);
        if (safe) {
          if (from === 'i64' && type !== 'i64') value = expression.max(value, i64(0));
          if (from === 'u128' && type === 'u64') value = this.clamp('u128', value, 1n << 64n);
          if (from === 'u128' && type === 'i64') value = this.clamp('u128', value, 1n << 63n);
          if (from === 'u64' && type === 'i64') value = this.clamp('u64', value, 1n << 63n);
        }
        return g(expression.cast(type, value));
      }],
    ];
    if (type !== 'i64') {
      options.push([2, () => {
        const op = rng.pick(['bitAnd', 'bitOr', 'bitXor'] as const);
        return g(expression[op](sub(), sub()));
      }]);
      options.push([2, () => {
        const left = rng.chance(0.5);
        const amount = safe ? this.operand(scope, 'u64', 'shift', depth) : sub('u64');
        const value = safe && left ? this.operand(scope, type, 'small', depth) : sub();
        return g(left ? expression.shiftLeft(value, amount) : expression.shiftRight(value, amount));
      }]);
      options.push([2, () => {
        const divisor = safe ? this.operand(scope, type, 'nonzero', depth) : sub();
        const [left, right] = safe ? [this.operand(scope, type, 'small', depth), this.operand(scope, type, 'small', depth)] : [sub(), sub()];
        return g(expression.multiplyDivide(left, right, divisor, rng.pick(['down', 'up'] as const)));
      }]);
    }
    if (type === 'u128') {
      options.push([1, () => g(expression.powerOfTen(safe ? this.clamp('u64', sub('u64'), 39n) : sub('u64')))]);
    }
    if (type === 'u64') {
      options.push([1, () => g(expression.bytesLength(this.expr(scope, 'bytes', depth - 1).e))]);
    }
    const read = this.accountRead(scope, type, depth);
    if (read) options.push([3, read]);
    const introspection = this.introspection(scope, type, depth);
    if (introspection) options.push([2, introspection]);
    return rng.weighted(options)();
  }

  /** A typed read of a readable account, at a fixed or a run-time offset. */
  accountRead(scope: Scope, type: Exclude<ValueKind, 'bytes'>, depth: number): (() => Gen) | undefined {
    const readable = this.accountsOf(scope, (decl) => decl.readable);
    if (readable.length === 0) return undefined;
    // Mostly an account wide enough for the type: a fixed read past an account's data raises its
    // floor past what the world gives it, and the run fails before its first step.
    const narrowest = Math.min(...READS_BY_KIND[type].map((readType) => READ_WIDTH[readType]));
    const wide = readable.filter(({ decl }) => decl.dataLength >= narrowest);
    return () => {
      const rng = this.rng;
      const { reference, decl } = rng.pick(wide.length > 0 && rng.chance(0.97) ? wide : readable);
      const fitting = READS_BY_KIND[type].filter((candidate) => READ_WIDTH[candidate] <= decl.dataLength);
      const readType = rng.pick(fitting.length > 0 && rng.chance(0.97) ? fitting : READS_BY_KIND[type]);
      const width = READ_WIDTH[readType];
      const room = decl.dataLength - width;
      let offset: number | Expression;
      if (room < 0) {
        offset = rng.below(4);
      } else if (rng.chance(0.55)) {
        offset = rng.range(0, room);
      } else {
        const value = this.expr(scope, 'u64', depth - 1).e;
        offset = rng.chance(0.95) ? this.clamp('u64', value, BigInt(room + 1)) : value;
      }
      return { e: expression.accountData(reference, offset, readType), type, max: 0 };
    };
  }

  /** A read of the Instructions sysvar: the transaction is the memo before the run, then the run. */
  introspection(scope: Scope, type: Exclude<ValueKind, 'bytes'>, depth: number): (() => Gen) | undefined {
    const sysvar = this.sysvar();
    if (sysvar === undefined) return undefined;
    const rng = this.rng;
    const index = () => (rng.chance(0.85) ? this.clamp('u64', this.expr(scope, 'u64', depth - 1).e, 2n) : this.expr(scope, 'u64', depth - 1).e);
    const options: [number, () => Gen][] = [];
    if (type === 'u64') {
      options.push([1, () => ({ e: expression[rng.pick(['instructionAccountCount', 'instructionDataLength'] as const)](fixed(sysvar), index()), type, max: 0 })]);
      options.push([1, () => ({
        e: expression.instructionAccountFlags(fixed(sysvar), u64(1), this.clamp('u64', this.expr(scope, 'u64', depth - 1).e, BigInt(this.runAccounts))),
        type,
        max: 0,
      })]);
    }
    if (type === 'pubkey') {
      options.push([1, () => ({ e: expression.instructionProgram(fixed(sysvar), index()), type, max: 0 })]);
      options.push([1, () => ({
        e: expression.instructionAccount(fixed(sysvar), u64(1), this.clamp('u64', this.expr(scope, 'u64', depth - 1).e, BigInt(this.runAccounts))),
        type,
        max: 0,
      })]);
    }
    options.push([2, () => {
      const readType = rng.pick(READS_BY_KIND[type]);
      const room = Math.max(1, this.runDataLength - READ_WIDTH[readType] + 1);
      const offset = this.clamp('u64', this.expr(scope, 'u64', depth - 1).e, BigInt(room));
      return { e: expression.instructionData(fixed(sysvar), u64(1), offset, readType), type, max: 0 };
    }]);
    return () => rng.weighted(options)();
  }

  boolean(scope: Scope, depth: number): Gen {
    const rng = this.rng;
    const g = (e: Expression): Gen => ({ e, type: 'bool', max: 0 });
    const options: [number, () => Gen][] = [
      [6, () => {
        const kind = rng.pick(ALL_KINDS);
        const left = this.expr(scope, kind, depth - 1).e;
        const right = this.expr(scope, kind, depth - 1).e;
        if (kind === 'u64' || kind === 'i64' || kind === 'u128') {
          const op = rng.pick(['equal', 'notEqual', 'lessThan', 'lessThanOrEqual', 'greaterThan', 'greaterThanOrEqual'] as const);
          return g(expression[op](left, right));
        }
        return g(expression[rng.pick(['equal', 'notEqual'] as const)](left, right));
      }],
      [2, () => g(expression[rng.pick(['and', 'or'] as const)](this.expr(scope, 'bool', depth - 1).e, this.expr(scope, 'bool', depth - 1).e))],
      [2, () => g(expression.not(this.expr(scope, 'bool', depth - 1).e))],
      [1, () => g(expression.select(this.expr(scope, 'bool', depth - 1).e, this.expr(scope, 'bool', depth - 1).e, this.expr(scope, 'bool', depth - 1).e))],
    ];
    const read = this.accountRead(scope, 'bool', depth);
    if (read) options.push([1, read]);
    const introspection = this.introspection(scope, 'bool', depth);
    if (introspection) options.push([1, introspection]);
    return rng.weighted(options)();
  }

  pubkey(scope: Scope, depth: number): Gen {
    const rng = this.rng;
    const g = (e: Expression): Gen => ({ e, type: 'pubkey', max: 0 });
    const options: [number, () => Gen][] = [
      [2, () => g(expression.select(this.expr(scope, 'bool', depth - 1).e, this.expr(scope, 'pubkey', depth - 1).e, this.expr(scope, 'pubkey', depth - 1).e))],
    ];
    const programs = this.accountsOf(scope, (decl) => decl.kind === 'program');
    if (programs.length > 0) {
      options.push([4, () => {
        const seeds: Expression[] = [];
        for (let index = 0, count = rng.range(1, 3); index < count; index += 1) {
          const kind = rng.pick(['bool', 'u64', 'i64', 'u128', 'pubkey', 'bytes'] as const);
          let seed = this.expr(scope, kind, depth - 1);
          if (kind === 'bytes' && seed.max > 32) seed = this.literal('bytes');
          if (kind === 'bytes' && seed.max > 32) seed = { e: expression.bytes(rng.bytes(4)), type: 'bytes', max: 4 };
          seeds.push(seed.e);
        }
        const bump = rng.chance(0.1)
          ? rng.weighted<Expression>([
              [2, u64(rng.pick([255, 254, 253]))],
              [1, this.clamp('u64', this.expr(scope, 'u64', depth - 1).e, 256n)],
              [1, this.operand(scope, 'u64', 'shift', depth)],
            ])
          : undefined;
        return g(expression.pda(rng.pick(programs).reference, seeds, bump));
      }]);
    }
    const read = this.accountRead(scope, 'pubkey', depth);
    if (read) options.push([2, read]);
    const introspection = this.introspection(scope, 'pubkey', depth);
    if (introspection) options.push([1, introspection]);
    return rng.weighted(options)();
  }

  bytesValue(scope: Scope, depth: number): Gen {
    const rng = this.rng;
    const options: [number, () => Gen][] = [
      [3, () => {
        const left = this.expr(scope, 'bytes', depth - 1);
        const right = this.expr(scope, 'bytes', depth - 1);
        return { e: expression.select(this.expr(scope, 'bool', depth - 1).e, left.e, right.e), type: 'bytes', max: Math.max(left.max, right.max) };
      }],
    ];
    // Byte reads take accounts the template cannot write.
    const readable = this.accountsOf(scope, (decl) => decl.readable && !decl.writable);
    if (readable.length > 0) {
      options.push([2, () => {
        const { reference, decl } = rng.pick(readable);
        const length = rng.range(1, Math.max(1, Math.min(16, decl.dataLength)));
        const room = BigInt(Math.max(1, decl.dataLength - length + 1));
        const offset = rng.chance(0.95) ? this.clamp('u64', this.expr(scope, 'u64', depth - 1).e, room) : this.expr(scope, 'u64', depth - 1).e;
        return { e: expression.accountDataBytes(reference, offset, length), type: 'bytes', max: length };
      }]);
    }
    const sysvar = this.sysvar();
    if (sysvar) {
      options.push([1, () => {
        const length = rng.range(1, 8);
        const offset = this.clamp('u64', this.expr(scope, 'u64', depth - 1).e, BigInt(Math.max(1, this.runDataLength - length + 1)));
        return { e: expression.instructionDataBytes(fixed(sysvar), u64(1), offset, length), type: 'bytes', max: length };
      }]);
    }
    return rng.weighted(options)();
  }

  /** A condition that holds unless something it reads fails, so runs usually go on. */
  likelyTrue(scope: Scope, depth: number): Expression {
    const rng = this.rng;
    const kind = rng.pick(['u64', 'i64', 'u128', 'bool', 'pubkey', 'bytes'] as const);
    const value = this.expr(scope, kind, depth).e;
    if (kind === 'u64' || kind === 'i64' || kind === 'u128') {
      const other = this.expr(scope, kind, depth - 1).e;
      return rng.pick([
        () => expression.lessThanOrEqual(expression.min(value, other), expression.max(value, other)),
        () => expression.equal(value, value),
        () => expression.greaterThanOrEqual(expression.max(value, other), other),
      ])();
    }
    if (kind === 'bool') return expression.or(value, expression.not(value));
    return expression.equal(value, value);
  }

  part(scope: Scope, depth: number): DataPart {
    const rng = this.rng;
    if (rng.chance(0.12)) return data.literal(rng.bytes(rng.range(0, 8)));
    const kind = rng.pick(ALL_KINDS);
    const value = this.expr(scope, kind, depth);
    const safe = rng.chance(0.9);
    switch (kind) {
      case 'u64':
      case 'u128': {
        const encoding = rng.pick(kind === 'u64' ? (['u8', 'u16', 'u32', 'u64'] as const) : (['u8', 'u16', 'u32', 'u64', 'u128'] as const));
        if (encoding === 'u128') return data.encode('u128', value.e);
        const bound = { u8: 1n << 8n, u16: 1n << 16n, u32: 1n << 32n, u64: 1n << 64n }[encoding];
        const narrowed = safe && (encoding !== 'u64' || kind === 'u128') ? this.clamp(kind, value.e, bound) : value.e;
        return data.encode(encoding, narrowed);
      }
      case 'i64':
        return data.encode('i64', value.e);
      case 'bool':
        return data.encode('bool', value.e);
      case 'pubkey':
        return data.encode('pubkey', value.e);
      case 'bytes':
        return data.encode('bytes', value.e);
    }
  }

  emitTag(): Uint8Array {
    const rng = this.rng;
    if (rng.chance(0.02)) return rng.chance(0.5) ? rng.bytes(rng.range(0, 3)) : Uint8Array.of(0x42, 0x45, 0x56, ...rng.bytes(2));
    for (;;) {
      const tag = rng.bytes(rng.range(4, 8));
      if (!(tag[0] === 0x42 && tag[1] === 0x45 && tag[2] === 0x56)) return tag;
    }
  }

  // -- steps -----------------------------------------------------------------------------------

  /** Accepts `candidate` if it fits the budget, and charges it. */
  fits(candidate: Step, cpis = 0): boolean {
    const literals = new Set(this.literals);
    const inputs = new Set(this.usedInputs);
    const cost = stepCost(candidate, literals, inputs);
    if (this.instructions + cost > this.limit + 2 || this.cpis + cpis > 64) return false;
    this.instructions += cost;
    this.cpis += cpis;
    for (const key of literals) this.literals.add(key);
    for (const name of inputs) this.usedInputs.add(name);
    return true;
  }

  depth(): number {
    return this.rng.weighted([[2, 0], [4, 1], [3, 2], [1, 3]]);
  }

  /** A binding, which enters the scope only once the step is accepted. */
  letStep(scope: Scope): Step[] | undefined {
    const rng = this.rng;
    let name: string;
    let binding: Binding;
    let candidate: Step;
    // In a loop body, a binding of a carried value before the body assigns it: the before-and-
    // after pattern bindings exist for.
    if (scope.carried.size > 0 && rng.chance(0.25)) {
      const [carriedName, carriedBinding] = rng.pick([...scope.carried]);
      name = this.name('snap');
      binding = carriedBinding;
      candidate = step.snapshot(name, expression.variable(carriedName));
    } else {
      const type = rng.weighted<ValueKind>([[8, 'u64'], [3, 'bool'], [2, 'i64'], [2, 'u128'], [3, 'pubkey'], [2, 'bytes']]);
      const value = this.expr(scope, type, this.depth());
      name = this.name('v');
      binding = { type, max: value.max };
      candidate = step.let(name, value.e);
    }
    if (!this.fits(candidate)) return undefined;
    scope.vars.set(name, binding);
    return [candidate];
  }

  requireStep(scope: Scope): Step {
    const condition = this.rng.chance(0.85) ? this.likelyTrue(scope, this.depth()) : this.expr(scope, 'bool', this.depth()).e;
    return step.require(condition, this.rng.chance(0.3) ? this.name('check') : undefined);
  }

  /** A well-formed CPI most of the time: a transfer, a memo, or the Token program's size query. */
  invokeSteps(scope: Scope, passes: number): Step[] | undefined {
    const rng = this.rng;
    const payer = this.fixedDecls.find((decl) => decl.kind === 'wallet' && decl.signer && decl.writable)!;
    const systems = this.accountsOf(scope, (decl) => decl.kind === 'program' && decl.program === 'system');
    const memos = this.accountsOf(scope, (decl) => decl.kind === 'program' && decl.program === 'memo');
    const tokens = this.fixedDecls.filter((decl) => decl.kind === 'program' && decl.program === 'token');
    const mint = this.fixedDecls.find((decl) => decl.kind === 'mint');
    const recipients = this.accountsOf(scope, (decl) => decl.kind === 'wallet' && decl.writable);
    const kind = rng.weighted([
      [systems.length > 0 && recipients.length > 0 ? 5 : 0, 'transfer'],
      [memos.length > 0 ? 3 : 0, 'memo'],
      [tokens.length > 0 && mint ? 3 : 0, 'size'],
      [1, 'junk'],
    ] as const);
    // A junk call would fail the run, so it is usually guarded by a condition that is false.
    const when =
      kind === 'junk' && rng.chance(0.8)
        ? expression.not(this.likelyTrue(scope, this.depth()))
        : rng.chance(0.35)
          ? this.expr(scope, 'bool', this.depth()).e
          : undefined;
    const label = rng.chance(0.3) ? this.name('call') : undefined;
    let invoke: Step;
    if (kind === 'transfer') {
      const program = rng.pick(systems);
      const amount = this.expr(scope, 'u64', this.depth()).e;
      invoke = step.invoke({
        program: program.reference,
        accounts: [
          { account: fixed(payer.name), signer: true, writable: true },
          { account: rng.pick(recipients).reference, signer: false, writable: true },
        ],
        data: [data.literal(Uint8Array.of(2, 0, 0, 0)), data.encode('u64', rng.chance(0.9) ? this.clamp('u64', amount, 10_000n) : amount)],
        ...(when ? { when } : {}),
        ...(this.groups.length > 0 && rng.chance(0.4) ? { accountGroup: rng.pick(this.groups) } : {}),
        ...(program.decl.constraint.address && rng.chance(0.3) ? { programAddress: SYSTEM_PROGRAM_ADDRESS_BYTES } : {}),
        ...(label ? { label } : {}),
      });
    } else if (kind === 'memo') {
      const parts: DataPart[] = [data.literal(new TextEncoder().encode(`m${rng.below(10)}:`))];
      for (let index = 0, count = rng.range(0, 3); index < count; index += 1) {
        parts.push(
          rng.weighted<DataPart>([
            [3, data.encode('u8', this.clamp('u64', this.expr(scope, 'u64', this.depth()).e, 128n))],
            [1, data.encode('bool', this.expr(scope, 'bool', this.depth()).e)],
            [1, data.literal(new TextEncoder().encode(String(rng.below(1000))))],
            [0.15, this.part(scope, this.depth())],
          ]),
        );
      }
      invoke = step.invoke({
        program: rng.pick(memos).reference,
        accounts: rng.chance(0.3) ? [{ account: fixed(payer.name), signer: true, writable: false }] : [],
        data: parts,
        ...(when ? { when } : {}),
        ...(label ? { label } : {}),
      });
    } else if (kind === 'size') {
      invoke = step.invoke({
        program: fixed(rng.pick(tokens).name),
        accounts: [{ account: fixed(mint!.name), signer: false, writable: false }],
        data: [data.literal(Uint8Array.of(21))],
        ...(when ? { when } : {}),
        ...(label ? { label } : {}),
      });
    } else {
      const programs = this.accountsOf(scope, (decl) => decl.kind === 'program');
      if (programs.length === 0) return undefined;
      const candidates = this.accountsOf(scope, (decl) => decl.kind !== 'entry');
      const accounts = Array.from({ length: rng.range(0, 3) }, () => {
        const { reference, decl } = rng.pick(candidates);
        return { account: reference, signer: decl.signer && rng.chance(0.5), writable: decl.writable && rng.chance(0.5) };
      });
      invoke = step.invoke({
        program: rng.pick(programs).reference,
        accounts,
        data: Array.from({ length: rng.range(0, 3) }, () => this.part(scope, this.depth())),
        ...(when ? { when } : {}),
        ...(label ? { label } : {}),
      });
    }
    if (!this.fits(invoke, passes)) return undefined;
    const steps = [invoke];
    // Return data is read right after an unconditional invoke; the size query sets 8 bytes.
    if (!when && rng.chance(kind === 'size' ? 0.7 : 0.02)) {
      const readType = rng.pick(['bool', 'u8', 'u16', 'u32', 'i32', 'u64', 'i64', 'u128', 'pubkey'] as const);
      const room = TOKEN_ACCOUNT_SIZE_RETURN - READ_WIDTH[readType];
      const offset = room >= 0 && rng.chance(0.9) ? rng.range(0, room) : rng.range(0, 12);
      const name = this.name('ret');
      const read = step.let(name, expression.returnData(readType, offset));
      if (this.fits(read)) {
        scope.vars.set(name, { type: { bool: 'bool', u8: 'u64', u16: 'u64', u32: 'u64', i32: 'i64', u64: 'u64', i64: 'i64', u128: 'u128', pubkey: 'pubkey' }[readType] as ValueKind, max: 0 });
        steps.push(read);
      }
    }
    return steps;
  }

  emitStep(scope: Scope): Step {
    const parts: DataPart[] = [data.literal(this.emitTag())];
    for (let index = 0, count = this.rng.range(1, 5); index < count; index += 1) parts.push(this.part(scope, this.depth()));
    return step.emit(parts, this.rng.chance(0.2) ? this.name('log') : undefined);
  }

  setRegistryStep(scope: Scope): Step | undefined {
    const entries = this.fixedDecls.filter((decl) => decl.kind === 'entry');
    if (entries.length === 0) return undefined;
    const entry = this.rng.pick(entries);
    const [field, type] = this.rng.pick(entry.registry!.fields);
    return step.setRegistry(entry.name, field, this.expr(scope, type, this.depth()).e);
  }

  /** A new value for a carried variable, of its type and, for `bytes`, its maximum length. */
  assignStep(scope: Scope): Step | undefined {
    if (scope.carried.size === 0) return undefined;
    const [name, binding] = this.rng.pick([...scope.carried]);
    return this.assignFor(scope, name, binding);
  }

  assignFor(scope: Scope, name: string, binding: Binding): Step {
    const rng = this.rng;
    const current = expression.variable(name);
    const condition = () => this.expr(scope, 'bool', this.depth() - 1).e;
    let value: Expression;
    switch (binding.type) {
      case 'u64':
      case 'i64':
      case 'u128': {
        const type = binding.type;
        const other = this.expr(scope, type, this.depth()).e;
        value = rng.pick([
          () => expression.add(this.clamp(type, current, 1n << 40n), this.clamp(type, other, 1n << 20n)),
          () => expression.max(current, other),
          () => expression.min(current, other),
          () => expression.select(condition(), current, other),
          () => (type === 'i64' ? expression.subtract(this.clamp(type, current, 1n << 40n), this.clamp(type, other, 1n << 20n)) : expression.bitXor(current, other)),
        ])();
        break;
      }
      case 'bool':
        value = rng.pick([
          () => expression.not(current),
          () => expression.and(current, condition()),
          () => expression.or(current, condition()),
          () => expression.select(condition(), current, condition()),
        ])();
        break;
      case 'pubkey':
        value = rng.chance(0.5) ? expression.select(condition(), current, this.expr(scope, 'pubkey', this.depth()).e) : this.expr(scope, 'pubkey', this.depth()).e;
        break;
      case 'bytes': {
        // Same maximum: select between this value and one no longer than it.
        const shorter = [...scope.vars].filter(([, other]) => other.type === 'bytes' && other.max <= binding.max);
        const other = shorter.length > 0 && rng.chance(0.6)
          ? expression.variable(rng.pick(shorter)[0])
          : expression.bytes(rng.bytes(rng.range(0, binding.max)));
        value = expression.select(condition(), current, other);
        break;
      }
    }
    return step.assign(name, value);
  }

  /**
   * The before-and-after pattern bindings exist for, inside a loop body: bind a carried value,
   * assign the variable, then log both. The language says the binding keeps the value it had.
   */
  beforeAndAfterSteps(scope: Scope): Step[] | undefined {
    if (scope.carried.size === 0) return undefined;
    const [name, binding] = this.rng.pick([...scope.carried]);
    const before = this.name('before');
    const steps = [
      step.snapshot(before, expression.variable(name)),
      this.assignFor(scope, name, binding),
      step.emit([data.literal(this.emitTag()), data.encode(binding.type, expression.variable(before)), data.encode(binding.type, expression.variable(name))]),
    ];
    const literals = new Set(this.literals);
    const inputs = new Set(this.usedInputs);
    const cost = steps.reduce((sum, current) => sum + stepCost(current, literals, inputs), 0);
    if (this.instructions + cost > this.limit + 2) return undefined;
    this.instructions += cost;
    for (const key of literals) this.literals.add(key);
    for (const input of inputs) this.usedInputs.add(input);
    scope.vars.set(before, binding);
    return steps;
  }

  bodyStep(scope: Scope, passes: number): Step[] | undefined {
    const rng = this.rng;
    const kind = rng.weighted([
      [6, 'let'],
      [2, 'require'],
      [2, 'invoke'],
      [3, 'emit'],
      [this.fixedDecls.some((decl) => decl.kind === 'entry') ? 2 : 0, 'setRegistry'],
      [scope.carried.size > 0 ? 6 : 0, 'assign'],
      [scope.carried.size > 0 ? 2 : 0, 'beforeAndAfter'],
    ] as const);
    if (kind === 'invoke') return this.invokeSteps(scope, passes);
    if (kind === 'beforeAndAfter') return this.beforeAndAfterSteps(scope);
    if (kind === 'let') return this.letStep(scope);
    let candidate: Step | undefined;
    if (kind === 'require') candidate = this.requireStep(scope);
    else if (kind === 'emit') candidate = this.emitStep(scope);
    else if (kind === 'setRegistry') candidate = this.setRegistryStep(scope);
    else candidate = this.assignStep(scope);
    return candidate && this.fits(candidate) ? [candidate] : undefined;
  }

  loopStep(scope: Scope, kind: 'forEach' | 'repeat'): Step | undefined {
    const rng = this.rng;
    const candidates = [...scope.vars].filter(() => rng.chance(0.4)).slice(0, 3);
    const carried = new Map(candidates);
    const body: Scope = { loop: kind === 'forEach' ? 'rows' : 'count', vars: new Map(scope.vars), carried };
    const max = rng.range(1, 5);
    const passes = kind === 'forEach' ? this.batch!.maxIterations : max;
    let count: Expression | undefined;
    if (kind === 'repeat') {
      const value = this.expr(scope, 'u64', this.depth()).e;
      count = rng.chance(0.95) ? this.clamp('u64', value, BigInt(max + 1)) : value;
    }
    // The loop record, the count and the carried copies, before the body.
    const header: Step =
      kind === 'forEach'
        ? step.forEach([step.require(expression.bool(true))], { carry: [...carried.keys()] })
        : step.repeat(count!, [step.require(expression.bool(true))], { max, carry: [...carried.keys()] });
    const literals = new Set(this.literals);
    const inputs = new Set(this.usedInputs);
    const headerCost = stepCost({ ...header, steps: [] } as Step, literals, inputs);
    if (this.instructions + headerCost + 2 > this.limit + 2) return undefined;
    this.instructions += headerCost;
    for (const key of literals) this.literals.add(key);
    for (const name of inputs) this.usedInputs.add(name);
    const steps: Step[] = [];
    const bodyLimit = rng.range(1, 10);
    for (let attempt = 0; attempt < bodyLimit * 2 && steps.length < bodyLimit; attempt += 1) {
      const next = this.bodyStep(body, passes);
      if (next) steps.push(...next);
    }
    // A body must compile to an instruction: end with one that always does.
    const closing = rng.chance(0.5) ? this.emitStep(body) : step.require(this.likelyTrue(body, 1));
    if (this.fits(closing)) steps.push(closing);
    else steps.push(step.require(expression.bool(true)));
    if (kind === 'forEach') return step.forEach(steps, { carry: [...carried.keys()], ...(rng.chance(0.3) ? { label: this.name('rows') } : {}) });
    return step.repeat(count!, steps, { max, carry: [...carried.keys()], ...(rng.chance(0.3) ? { label: this.name('passes') } : {}) });
  }

  steps(): Step[] {
    const rng = this.rng;
    const scope: Scope = { loop: undefined, vars: new Map(), carried: new Map() };
    const steps: Step[] = [];
    // Registry keys compile before the steps; charge them first.
    for (const decl of this.fixedDecls) {
      if (decl.kind !== 'entry') continue;
      const key = decl.constraint.registry?.key;
      this.instructions += 1 + (key ? expressionCost(key as Expression, this.literals, this.usedInputs) : 0);
      this.cpis += 3;
    }
    const loops: ('forEach' | 'repeat')[] = [];
    if (this.batch) loops.push('forEach');
    for (let index = 0, count = rng.weighted([[4, 0], [3, 1], [1, 2]]); index < count; index += 1) {
      loops.push(this.batch && rng.chance(0.4) ? 'forEach' : 'repeat');
    }
    // A prelude of bindings known to be small, non-zero or a shift amount, which later arithmetic
    // reads directly: values made first and read last.
    for (let index = 0, count = rng.range(0, 6); index < count; index += 1) {
      const type = rng.weighted<'u64' | 'i64' | 'u128'>([[5, 'u64'], [1, 'i64'], [2, 'u128']]);
      const range = rng.pick(type === 'u64' ? (['small', 'nonzero', 'shift'] as const) : (['small', 'nonzero'] as const));
      const name = this.name(range === 'small' ? 'small' : range === 'nonzero' ? 'nonzero' : 'shift');
      const candidate = step.let(name, this.rangeExpression(scope, type, range, this.depth()));
      if (this.fits(candidate)) {
        steps.push(candidate);
        scope.vars.set(name, { type, max: 0, range });
      }
    }
    const triggers = loops.map(() => rng.range(0, Math.max(0, this.limit - 12))).sort((x, y) => x - y);
    let stalls = 0;
    while (this.instructions < this.limit && stalls < 12 && steps.length < 100) {
      if (loops.length > 0 && this.instructions >= triggers[0]!) {
        triggers.shift();
        const loop = this.loopStep(scope, loops.shift()!);
        if (loop) steps.push(loop);
        continue;
      }
      const kind = rng.weighted([
        [8, 'let'],
        [3, 'require'],
        [3, 'invoke'],
        [2, 'emit'],
        [this.fixedDecls.some((decl) => decl.kind === 'entry') ? 2 : 0, 'setRegistry'],
      ] as const);
      let next: Step[] | undefined;
      if (kind === 'invoke') next = this.invokeSteps(scope, 1);
      else if (kind === 'let') next = this.letStep(scope);
      else {
        const candidate = kind === 'require' ? this.requireStep(scope) : kind === 'emit' ? this.emitStep(scope) : this.setRegistryStep(scope);
        next = candidate && this.fits(candidate) ? [candidate] : undefined;
      }
      if (next) {
        steps.push(...next);
        stalls = 0;
      } else {
        stalls += 1;
      }
    }
    // A batch needs a forEach, even past the budget.
    if (this.batch && !steps.some((current) => current.kind === 'forEach')) {
      steps.push(step.forEach([step.require(expression.lessThanOrEqual(expression.loopIndex(), u64(60)))]));
    }
    if (steps.length === 0) steps.push(step.require(expression.bool(true)));
    // The run's return data: the values still in scope at the end.
    // Most runs return about half their root values: enough to see a swapped value, while the
    // values left out end their lives earlier, where reuse can get them wrong.
    if (rng.chance(0.85) && scope.vars.size > 0) {
      const parts: DataPart[] = [];
      let length = 0;
      for (const [name, binding] of [...scope.vars].filter(() => rng.chance(0.5))) {
        const width = binding.type === 'bytes' ? binding.max : { bool: 1, u64: 8, i64: 8, u128: 16, pubkey: 32 }[binding.type];
        if (length + width > 1_000 || parts.length >= 64) continue;
        length += width;
        parts.push(data.encode(binding.type, expression.variable(name)));
      }
      if (parts.length > 0) steps.push(step.setReturnData(parts));
    }
    return steps;
  }

  generate(): FuzzCase {
    this.declare();
    const fixedWorld = this.buildFixedWorld();
    const steps = this.steps();
    const accounts: Record<string, AccountConstraintInput> = {};
    for (const decl of this.fixedDecls) accounts[decl.name] = decl.constraint;
    const template: TemplateInput = {
      inputs: this.inputs,
      accounts,
      steps,
      ...(Object.keys(this.registries).length > 0 ? { registries: this.registries } : {}),
      ...(this.batch
        ? {
            batch: {
              maxIterations: this.batch.maxIterations,
              minIterations: this.batch.minIterations,
              row: Object.fromEntries(this.rowDecls.map((decl) => [decl.name, decl.constraint])),
              rowInputs: this.rowInputs,
            },
          }
        : {}),
      ...(this.groups.length > 0 ? { accountGroups: this.groups } : {}),
      ...(this.emitEvent ? { emitEvent: true } : {}),
    };
    const world = [...fixedWorld];
    for (let iteration = 0; iteration < (this.batch?.iterations ?? 0); iteration += 1) {
      for (const decl of this.rowDecls) world.push(this.worldAccount(decl, iteration));
    }
    this.groups.forEach((group, index) => {
      for (let member = 0; member < this.groupLengths[index]!; member += 1) {
        world.push({
          role: `${group}[${member}]`,
          signer: false,
          writable: this.rng.chance(0.5),
          source: { kind: 'stored', address: this.rng.bytes(32), lamports: this.lamports(), owner: SYSTEM_PROGRAM_ADDRESS_BYTES, data: new Uint8Array(0) },
        });
      }
    });
    return {
      seed: this.seed,
      size: this.size,
      template,
      accounts: world,
      inputs: this.inputValues,
      rows: this.rowValues,
      groupLengths: this.groupLengths,
    };
  }
}

export function generateCase(seed: number): FuzzCase {
  return new Generator(seed).generate();
}

// ---------------------------------------------------------------------------------------------
// Oracle 1: compiles or rejects cleanly
// ---------------------------------------------------------------------------------------------

/** Messages that mean the compiler failed inside, not that it refused the document. */
const INTERNAL_ERROR = [
  /Cannot read propert/i,
  /\bof undefined\b|undefined is not|is undefined\b/,
  /\bof null\b|null is not|is null\b/,
  /is not a function/,
  /is not iterable/,
  /^Expected u(8|16|32)$/,
  /Maximum call stack/i,
  /compiler bug/i,
  /Register reuse/,
  /NaN|Infinity/,
];

type Outcome = { ok: CompiledTemplate } | { error: unknown };

function compileOutcome(template: TemplateInput, options?: CompileOptions): Outcome {
  try {
    return { ok: compileTemplate(template, options) };
  } catch (error) {
    return { error };
  }
}

/** A refusal is clean when it is a Zod error or one of the compiler's own TypeError or RangeError messages. */
export function isCleanRejection(error: unknown): boolean {
  if (error instanceof ZodError) return true;
  if (!(error instanceof TypeError || error instanceof RangeError)) return false;
  return !INTERNAL_ERROR.some((pattern) => pattern.test(error.message));
}

function describeError(error: unknown): string {
  if (error instanceof ZodError) return `ZodError: ${error.issues.map((issue) => `${issue.path.join('.')}: ${issue.message}`).join('; ')}`;
  if (error instanceof Error) return `${error.constructor.name}: ${error.message}`;
  return String(error);
}

// ---------------------------------------------------------------------------------------------
// Oracle 3: an independent decoder and replay
// ---------------------------------------------------------------------------------------------

interface Record16 {
  opcode: number;
  dst: number;
  a: number;
  b: number;
  c: number;
  flags: number;
  immediate: bigint;
  reserved: number;
}

interface Segment8 {
  kind: number;
  register: number;
  rest: string;
}

interface DecodedProgram {
  header: Uint8Array;
  registerCount: number;
  instructions: Record16[];
  cpis: { segmentStart: number; segmentLength: number; bytes: string }[];
  segments: Segment8[];
  /** Every section that register renaming must leave alone, as hex. */
  fixedSections: string;
}

const NO_INDEX = 0xff;
const DATA_LITERAL = 0;
const DYNAMIC_OFFSET = 1;
const hex = (bytes: Uint8Array) => Buffer.from(bytes).toString('hex');

/** Decodes a payload by the wire format (`common/src/template/wire.rs`), not by the compiler's writer. */
export function decodeProgram(bytes: Uint8Array): DecodedProgram {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const header = bytes.slice(0, 24);
  const [fixedAccounts, stride, , inputs, registerCount, instructionCount, cpiCount] = [5, 6, 7, 8, 9, 10, 11].map((at) => bytes[at]!);
  const cpiAccounts = view.getUint16(12, true);
  const segmentCount = view.getUint16(14, true);
  const pubkeys = bytes[16]!;
  const blob = view.getUint16(18, true);
  const rowInputs = bytes[21]!;
  let offset = 24;
  const accountsAndInputs = bytes.slice(offset, offset + (fixedAccounts! + stride!) * 8 + (inputs! + rowInputs) * 4);
  offset += accountsAndInputs.length;
  const instructions: Record16[] = [];
  for (let pc = 0; pc < instructionCount!; pc += 1, offset += 16) {
    instructions.push({
      opcode: bytes[offset]!,
      dst: bytes[offset + 1]!,
      a: bytes[offset + 2]!,
      b: bytes[offset + 3]!,
      c: bytes[offset + 4]!,
      flags: bytes[offset + 5]!,
      immediate: view.getBigUint64(offset + 6, true),
      reserved: view.getUint16(offset + 14, true),
    });
  }
  const cpis = [];
  for (let index = 0; index < cpiCount!; index += 1, offset += 12) {
    cpis.push({ segmentLength: bytes[offset + 5]!, segmentStart: view.getUint16(offset + 6, true), bytes: hex(bytes.slice(offset, offset + 12)) });
  }
  const cpiAccountBytes = bytes.slice(offset, offset + cpiAccounts * 2);
  offset += cpiAccounts * 2;
  const segments: Segment8[] = [];
  for (let index = 0; index < segmentCount; index += 1, offset += 8) {
    segments.push({ kind: bytes[offset]!, register: bytes[offset + 1]!, rest: hex(bytes.slice(offset + 2, offset + 8)) });
  }
  const tail = bytes.slice(offset, offset + pubkeys * 32 + blob);
  return {
    header,
    registerCount: registerCount!,
    instructions,
    cpis,
    segments,
    fixedSections: [hex(accountsAndInputs), cpis.map((cpi) => cpi.bytes).join(''), hex(cpiAccountBytes), hex(tail)].join('|'),
  };
}

type Operand = 'a' | 'b' | 'c';

/**
 * The operands each opcode reads as registers, as `verify_instruction` in `verify.rs` reads them.
 * Written from the verifier, so a mistake in the compiler's own table (`REGISTER_OPERANDS`) shows.
 */
const VERIFIER_REGISTER_OPERANDS = new Map<number, readonly Operand[]>([
  [opcode.add, ['a', 'b']],
  [opcode.subtract, ['a', 'b']],
  [opcode.multiply, ['a', 'b']],
  [opcode.divide, ['a', 'b']],
  [opcode.remainder, ['a', 'b']],
  [opcode.min, ['a', 'b']],
  [opcode.max, ['a', 'b']],
  [opcode.bitAnd, ['a', 'b']],
  [opcode.bitOr, ['a', 'b']],
  [opcode.bitXor, ['a', 'b']],
  [opcode.shiftLeft, ['a', 'b']],
  [opcode.shiftRight, ['a', 'b']],
  [opcode.mulDiv, ['a', 'b', 'c']],
  [opcode.mulDivCeil, ['a', 'b', 'c']],
  [opcode.powerOfTen, ['a']],
  [opcode.instructionProgram, ['b']],
  [opcode.instructionAccountCount, ['b']],
  [opcode.instructionDataLength, ['b']],
  [opcode.instructionAccount, ['b', 'c']],
  [opcode.instructionAccountFlags, ['b', 'c']],
  [opcode.readInstructionData, ['b', 'c']],
  [opcode.readInstructionBytes, ['b', 'c']],
  [opcode.readAccountBytes, ['b']],
  [opcode.bytesLength, ['a']],
  [opcode.writeRegistry, ['a']],
  [opcode.equal, ['a', 'b']],
  [opcode.notEqual, ['a', 'b']],
  [opcode.lessThan, ['a', 'b']],
  [opcode.lessThanOrEqual, ['a', 'b']],
  [opcode.greaterThan, ['a', 'b']],
  [opcode.greaterThanOrEqual, ['a', 'b']],
  [opcode.and, ['a', 'b']],
  [opcode.or, ['a', 'b']],
  [opcode.not, ['a']],
  [opcode.select, ['a', 'b', 'c']],
  [opcode.castU64, ['a']],
  [opcode.castI64, ['a']],
  [opcode.castU128, ['a']],
  [opcode.move, ['a']],
  [opcode.createPda, ['b']],
  [opcode.require, ['a']],
]);
const VERIFIER_READ_OPCODES = new Set<number>([
  opcode.readU8,
  opcode.readU16,
  opcode.readU32,
  opcode.readU64,
  opcode.readI64,
  opcode.readU128,
  opcode.readPubkey,
  opcode.readBool,
  opcode.readI32,
]);

/** The register operands `record` reads: a dynamic read's offset, a guard, a count, a key, or the table's. */
function registerOperands(record: Record16): Operand[] {
  if (VERIFIER_READ_OPCODES.has(record.opcode)) return record.flags & DYNAMIC_OFFSET ? ['b'] : [];
  if (record.opcode === opcode.invoke || record.opcode === opcode.openRegistry) return record.b === NO_INDEX ? [] : ['b'];
  if (record.opcode === opcode.repeat) return ['b'];
  return [...(VERIFIER_REGISTER_OPERANDS.get(record.opcode) ?? [])];
}

/** The data segments `record` reads as it runs: seeds, output parts, or an invoke's data. */
function segmentsRead(program: DecodedProgram, record: Record16): Segment8[] {
  if (record.opcode === opcode.invoke) {
    const cpi = program.cpis[record.a];
    return cpi ? program.segments.slice(cpi.segmentStart, cpi.segmentStart + cpi.segmentLength) : [];
  }
  if ([opcode.derivePda, opcode.createPda, opcode.emit, opcode.setReturnData].includes(record.opcode as never)) {
    const start = Number(record.immediate & 0xffff_ffffn);
    return program.segments.slice(start, start + Number(record.immediate >> 32n));
  }
  return [];
}

/** Writes: every record whose destination is not `NO_INDEX`. */
function registersRead(program: DecodedProgram, record: Record16): number[] {
  const reads = registerOperands(record).map((operand) => record[operand]);
  for (const segment of segmentsRead(program, record)) if (segment.kind !== DATA_LITERAL) reads.push(segment.register);
  return reads;
}

/**
 * What each register read sees, in order, when loop `n` makes `passes[n]` passes, following the
 * runtime: a pass starts from the registers as the loop found them plus what it carried so far;
 * after the last pass the loop restores them, carried registers excepted. A value is named by the
 * program counter and pass that wrote it.
 */
export function replay(program: DecodedProgram, passes: readonly number[], inferCarried = false): string[] {
  const seen: string[] = [];
  let registers: string[] = new Array<string>(256).fill('unset');
  const run = (pc: number, pass: string) => {
    const record = program.instructions[pc]!;
    for (const register of registersRead(program, record)) {
      seen.push(`${pc}:${register >= program.registerCount ? 'out of range' : registers[register]}`);
    }
    if (record.dst !== NO_INDEX) {
      if (record.dst >= program.registerCount) seen.push(`${pc}: writes out of range`);
      registers[record.dst] = `${pc}${pass}`;
    }
  };
  let loop = 0;
  for (let pc = 0; pc < program.instructions.length; ) {
    const record = program.instructions[pc]!;
    if (record.opcode !== opcode.forEach && record.opcode !== opcode.repeat) {
      run(pc, '');
      pc += 1;
      continue;
    }
    if (record.opcode === opcode.repeat) seen.push(`${pc}: count ${registers[record.b]}`);
    const carried = [...Array(64).keys()].filter((register) => (record.immediate >> BigInt(register)) & 1n);
    // A one-register-per-value reference past 64 registers cannot name every carried register in
    // its 64-bit mask. A body's moves are its assignments, each into a carried register.
    if (inferCarried) {
      for (let body = pc + 1; body <= pc + record.a; body += 1) {
        const inner = program.instructions[body]!;
        if (inner.opcode === opcode.move && !carried.includes(inner.dst)) carried.push(inner.dst);
      }
    }
    const count = passes[loop] ?? 0;
    loop += 1;
    const snapshot = registers.slice();
    for (let pass = 0; pass < count; pass += 1) {
      registers = snapshot.slice();
      for (let body = pc + 1; body <= pc + record.a; body += 1) run(body, `#${pass}`);
      for (const register of carried) snapshot[register] = registers[register]!;
    }
    registers = snapshot.slice();
    pc += record.a + 1;
  }
  return seen;
}

/**
 * Differences between `plain` and `renamed` other than register numbers, as messages. Register
 * fields may differ only where the verifier reads or writes a register.
 */
export function structuralDifferences(plain: DecodedProgram, renamed: DecodedProgram): string[] {
  const differences: string[] = [];
  const header = (bytes: Uint8Array) => hex(Uint8Array.from(bytes).fill(0, 9, 10));
  if (header(plain.header) !== header(renamed.header)) differences.push('header differs beyond the register count');
  if (renamed.registerCount > plain.registerCount) differences.push(`renamed uses ${renamed.registerCount} registers, plain ${plain.registerCount}`);
  if (plain.fixedSections !== renamed.fixedSections) differences.push('accounts, inputs, CPIs, pubkeys or blob differ');
  if (plain.instructions.length !== renamed.instructions.length) return [...differences, 'instruction counts differ'];
  plain.instructions.forEach((left, pc) => {
    const right = renamed.instructions[pc]!;
    if (left.opcode !== right.opcode || left.flags !== right.flags || left.reserved !== right.reserved) {
      differences.push(`pc ${pc}: opcode, flags or reserved bytes differ`);
      return;
    }
    const registers = new Set(registerOperands(left));
    for (const operand of ['a', 'b', 'c'] as const) {
      if (!registers.has(operand) && left[operand] !== right[operand]) differences.push(`pc ${pc}: non-register operand ${operand} differs`);
    }
    if ((left.dst === NO_INDEX) !== (right.dst === NO_INDEX)) differences.push(`pc ${pc}: destination appears or vanishes`);
    const loop = left.opcode === opcode.forEach || left.opcode === opcode.repeat;
    if (!loop && left.immediate !== right.immediate) differences.push(`pc ${pc}: immediate differs`);
  });
  if (plain.segments.length !== renamed.segments.length) differences.push('segment counts differ');
  plain.segments.forEach((left, index) => {
    const right = renamed.segments[index];
    if (!right || left.kind !== right.kind || left.rest !== right.rest) differences.push(`segment ${index} differs`);
    else if (left.kind === DATA_LITERAL && left.register !== right.register) differences.push(`literal segment ${index} names a register`);
  });
  return differences;
}

/** Pass counts to replay: every loop at 0, 1, 2 and 3 passes, and mixtures. */
function passVectors(loops: number, rng: Rng): number[][] {
  const vectors = [0, 1, 2, 3].map((count) => new Array<number>(loops).fill(count));
  for (let index = 0; index < (loops > 1 ? 8 : 0); index += 1) vectors.push(Array.from({ length: loops }, () => rng.below(4)));
  return vectors;
}

function loopCount(program: DecodedProgram): number {
  let loops = 0;
  for (let pc = 0; pc < program.instructions.length; pc += 1) {
    const record = program.instructions[pc]!;
    if (record.opcode === opcode.forEach || record.opcode === opcode.repeat) {
      loops += 1;
      pc += record.a;
    }
  }
  return loops;
}

/**
 * Problems with `renamed` as a renaming of `plain`; empty when it reads every value `plain` does.
 * `plain` past 64 registers is a reference only: its carried registers are inferred.
 */
export function reuseProblems(plain: Uint8Array, renamed: Uint8Array, seed: number): string[] {
  const left = decodeProgram(plain);
  const right = decodeProgram(renamed);
  const problems = structuralDifferences(left, right);
  if (problems.length > 0) return problems;
  const rng = new Rng(seed ^ 0x5eed);
  for (const passes of passVectors(loopCount(left), rng)) {
    const before = replay(left, passes, left.registerCount > 64);
    const after = replay(right, passes);
    if (before.length !== after.length) return [`passes ${passes.join(',')}: ${before.length} reads before, ${after.length} after`];
    const index = before.findIndex((value, at) => value !== after[at]);
    if (index >= 0) return [`passes ${passes.join(',')}: read ${index} saw ${before[index]} before renaming and ${after[index]} after`];
  }
  return [];
}

// ---------------------------------------------------------------------------------------------
// Mutations: documents that break the rules on purpose, for oracle 1
// ---------------------------------------------------------------------------------------------

/** Names that mean something to a JavaScript object, as a document read from JSON can hold. */
const TRICKY_NAMES = ['constructor', 'toString', 'valueOf', 'hasOwnProperty', 'prototype', '__proto__', 'length', '_'];

type Mutable = Record<string, unknown>;

/** A place in a document a mutation can write: `parent[key]`. */
interface Slot {
  parent: Mutable | unknown[];
  key: string | number;
}

/** Every place in a document a mutation can reach. */
interface Sites {
  expressions: Slot[];
  /** References and declarations by name, with their kind: `input`, `account`, `variable`... */
  names: { slot: Slot; kind: string }[];
  stepLists: { list: unknown[]; inLoop: boolean }[];
  invokes: Mutable[];
  outputs: Mutable[];
  loops: Mutable[];
  constraints: Mutable[];
  /** Numeric fields: offsets, lengths, maxima. */
  numbers: Slot[];
}

const READ_TYPES: readonly ReadType[] = ['bool', 'u8', 'u16', 'u32', 'i32', 'u64', 'i64', 'u128', 'pubkey'];

function collectSites(template: Mutable): Sites {
  const sites: Sites = { expressions: [], names: [], stepLists: [], invokes: [], outputs: [], loops: [], constraints: [], numbers: [] };
  const name = (parent: Mutable, key: string, kind: string) => sites.names.push({ slot: { parent, key }, kind });
  const reference = (node: unknown) => {
    if (node && typeof node === 'object') {
      const record = node as Mutable;
      name(record, 'name', record.kind === 'iterationAccount' ? 'row' : 'account');
    }
  };
  const visitExpression = (parent: Mutable | unknown[], key: string | number): void => {
    const node = (parent as Record<string | number, unknown>)[key] as Mutable | undefined;
    if (!node || typeof node !== 'object') return;
    sites.expressions.push({ parent, key });
    const child = (field: string) => visitExpression(node, field);
    switch (node.kind) {
      case 'input':
      case 'rowInput':
      case 'variable':
        name(node, 'name', String(node.kind));
        break;
      case 'accountField':
        reference(node.account);
        break;
      case 'accountData':
        reference(node.account);
        if (typeof node.offset === 'number') sites.numbers.push({ parent: node, key: 'offset' });
        else child('offset');
        break;
      case 'returnData':
        sites.numbers.push({ parent: node, key: 'offset' });
        break;
      case 'pda':
        reference(node.program);
        (node.seeds as unknown[]).forEach((_, index) => visitExpression(node.seeds as unknown[], index));
        if (node.bump) child('bump');
        break;
      case 'binary':
        child('left');
        child('right');
        break;
      case 'multiplyDivide':
        child('left');
        child('right');
        child('divisor');
        break;
      case 'powerOfTen':
        child('exponent');
        break;
      case 'not':
      case 'cast':
      case 'bytesLength':
        child('value');
        break;
      case 'select':
        child('condition');
        child('ifTrue');
        child('ifFalse');
        break;
      case 'instructionCount':
      case 'currentInstructionIndex':
        reference(node.sysvar);
        break;
      case 'instruction':
        reference(node.sysvar);
        child('index');
        break;
      case 'instructionAccount':
        reference(node.sysvar);
        child('index');
        child('position');
        break;
      case 'instructionData':
        reference(node.sysvar);
        child('index');
        child('offset');
        break;
      case 'instructionDataBytes':
        reference(node.sysvar);
        child('index');
        child('offset');
        sites.numbers.push({ parent: node, key: 'length' });
        break;
      case 'accountDataBytes':
        reference(node.account);
        child('offset');
        sites.numbers.push({ parent: node, key: 'length' });
        break;
      case 'registry':
        name(node, 'account', 'account');
        name(node, 'field', 'field');
        break;
    }
  };
  const visitParts = (parts: unknown[]) =>
    parts.forEach((part) => {
      if ((part as Mutable).kind === 'encoded') visitExpression(part as Mutable, 'value');
    });
  const visitSteps = (list: unknown[], inLoop: boolean) => {
    sites.stepLists.push({ list, inLoop });
    for (const item of list) {
      const current = item as Mutable;
      switch (current.kind) {
        case 'require':
          visitExpression(current, 'condition');
          break;
        case 'let':
        case 'assign':
          name(current, 'name', 'variable');
          visitExpression(current, 'value');
          break;
        case 'invoke':
          sites.invokes.push(current);
          reference(current.program);
          for (const entry of current.accounts as Mutable[]) reference(entry.account);
          visitParts(current.data as unknown[]);
          if (current.when) visitExpression(current, 'when');
          if (current.accountGroup !== undefined) name(current, 'accountGroup', 'group');
          break;
        case 'emit':
        case 'setReturnData':
          sites.outputs.push(current);
          visitParts(current.parts as unknown[]);
          break;
        case 'setRegistry':
          name(current, 'account', 'account');
          name(current, 'field', 'field');
          visitExpression(current, 'value');
          break;
        case 'forEach':
        case 'repeat':
          sites.loops.push(current);
          if (current.kind === 'repeat') {
            visitExpression(current, 'count');
            sites.numbers.push({ parent: current, key: 'max' });
          }
          (current.carry as unknown[] | undefined)?.forEach((_, index) => sites.names.push({ slot: { parent: current.carry as unknown[], key: index }, kind: 'variable' }));
          visitSteps(current.steps as unknown[], true);
          break;
      }
    }
  };
  visitSteps(template.steps as unknown[], false);
  const declared = [template.accounts, (template.batch as Mutable | undefined)?.row].filter(Boolean) as Mutable[];
  for (const record of declared) {
    for (const constraint of Object.values(record) as Mutable[]) {
      sites.constraints.push(constraint);
      sites.numbers.push({ parent: constraint, key: 'minDataLength' });
      const registry = constraint.registry as Mutable | undefined;
      if (registry) {
        name(registry, 'name', 'registry');
        name(registry, 'payer', 'account');
        if (registry.key) visitExpression(registry, 'key');
      }
    }
  }
  return sites;
}

/** All names of one kind in a document, to point a reference at the wrong one. */
function namesOf(sites: Sites, kind: string): string[] {
  return [...new Set(sites.names.filter((entry) => entry.kind === kind).map((entry) => String((entry.slot.parent as Record<string | number, unknown>)[entry.slot.key])))];
}

/** An expression a document has no business holding where it is put: out of scope, mistyped, or extreme. */
function hostileExpression(rng: Rng, sites: Sites, existing: unknown): Expression {
  const any = (kind: string, fallback: string) => {
    const names = namesOf(sites, kind);
    return names.length > 0 && rng.chance(0.7) ? rng.pick(names) : rng.chance(0.5) ? fallback : rng.pick(TRICKY_NAMES);
  };
  const literal = (): Expression =>
    rng.pick([
      () => u64((1n << 64n) - 1n),
      () => i64(-(1n << 63n)),
      () => u128((1n << 128n) - 1n),
      () => expression.bool(rng.chance(0.5)),
      () => expression.pubkey(new Uint8Array(32)),
      () => expression.bytes(rng.bytes(rng.pick([0, 1, 32, 33, 1024]))),
    ])();
  return rng.pick<() => Expression>([
    () => expression.loopIndex(),
    () => expression.rowInput(any('rowInput', 'nope')),
    () => expression.returnData(rng.pick(READ_TYPES), rng.pick([0, 1, 1008, 1016, 1017, 0xffff_ffff])),
    () => expression.variable(any('variable', 'nope')),
    () => expression.input(any('input', 'nope')),
    () => expression.accountField(account.iteration(any('row', 'nope')), rng.pick(['key', 'lamports', 'isEmpty'] as const)),
    () => expression.accountData(fixed(any('account', 'nope')), rng.pick([0, 31, 1024, 0xffff_fff8, 0xffff_ffff]), rng.pick(READ_TYPES)),
    () => expression.accountData(fixed(any('account', 'nope')), literal(), rng.pick(READ_TYPES)),
    () => expression.accountDataBytes(fixed(any('account', 'nope')), u64(0), rng.pick([1, 1024])),
    () => expression.instructionCount(fixed(any('account', 'nope'))),
    () => expression.instructionDataBytes(fixed(any('account', 'nope')), u64(0), u64(0), rng.pick([1, 1024])),
    () => expression.pda(fixed(any('account', 'nope')), Array.from({ length: rng.pick([1, 2, 15, 16]) }, literal)),
    () => expression.pda(fixed(any('account', 'nope')), [literal()], literal()),
    () => expression.registry(any('account', 'nope'), any('field', 'nope')),
    () => literal(),
    () => expression.cast(rng.pick(['u64', 'i64', 'u128'] as const), literal()),
    () => expression.powerOfTen(u64(rng.pick([0, 38, 39, 1000]))),
    () => expression.select(expression.bool(true), expression.bytes(rng.bytes(5)), expression.bytes(rng.bytes(900))),
    () => expression.bytesLength(literal()),
    () => expression.multiplyDivide(literal(), literal(), literal(), rng.pick(['down', 'up'] as const)),
    () => expression[rng.pick(['shiftLeft', 'bitAnd', 'lessThan', 'and', 'equal', 'remainder'] as const)](literal(), literal()),
    () => {
      // The same expression, nested deep.
      let value = existing as Expression;
      for (let level = 0, depth = rng.pick([10, 130, 600]); level < depth; level += 1) value = expression.not(value);
      return value;
    },
  ])();
}

/** A step that breaks a placement or typing rule wherever it lands. */
function hostileStep(rng: Rng, sites: Sites): Step {
  const any = (kind: string, fallback: string) => {
    const names = namesOf(sites, kind);
    return names.length > 0 && rng.chance(0.7) ? rng.pick(names) : fallback;
  };
  const value = () => hostileExpression(rng, sites, expression.bool(true));
  return rng.pick<() => Step>([
    () => step.assign(any('variable', 'nope'), value()),
    () => step.setReturnData([data.encode('u64', u64(1))]),
    () => step.let(`fresh${rng.below(1000)}`, expression.returnData(rng.pick(READ_TYPES), 0)),
    () => step.let(any('variable', 'nope'), u64(1)),
    () => step.repeat(u64(1), [step.repeat(u64(1), [step.require(expression.bool(true))], { max: 1 })], { max: 1 }),
    () => step.repeat(value(), [step.require(expression.bool(true))], { max: rng.pick([1, 255]), carry: [any('variable', 'nope')] }),
    () => step.forEach([step.emit([data.literal(Uint8Array.of(1, 2, 3, 4)), data.encode('u64', expression.loopIndex())])]),
    () => step.emit([data.literal(rng.pick([Uint8Array.of(0x42, 0x45, 0x56, 0x31), Uint8Array.of(1, 2), new Uint8Array(0)]))]),
    () => step.emit([data.literal(Uint8Array.of(9, 9, 9, 9)), ...Array.from({ length: 33 }, () => data.encode('pubkey', expression.pubkey(new Uint8Array(32))))]),
    () => step.invoke({ program: fixed(any('account', 'nope')), accounts: [{ account: fixed(any('account', 'nope')), signer: true, writable: true }], data: [] }),
    () => step.invoke({ program: fixed(any('account', 'nope')), accounts: [], data: [data.literal(new Uint8Array(4097))] }),
    () => step.invoke({ program: fixed(any('account', 'nope')), accounts: [], data: [], accountGroup: any('group', 'nope') }),
    () => step.setRegistry(any('account', 'nope'), any('field', 'nope'), value()),
    () => step.require(value()),
  ])();
}

/** Breaks a valid document in one to three places; returns what it did. */
export function mutateDocument(original: TemplateInput, rng: Rng): { template: TemplateInput; mutations: string[] } {
  const template = structuredClone(original) as unknown as Mutable;
  const mutations: string[] = [];
  for (let count = rng.range(1, 3); mutations.length < count; ) {
    const sites = collectSites(template);
    const operator = rng.weighted<string>([
      [5, 'expression'],
      [4, 'name'],
      [3, 'insert-step'],
      [2, 'move-step'],
      [2, 'delete-step'],
      [1, 'duplicate-step'],
      [3, 'account'],
      [2, 'number'],
      [2, 'loop'],
      [1, 'declarations'],
    ]);
    switch (operator) {
      case 'expression': {
        if (sites.expressions.length === 0) continue;
        const { parent, key } = rng.pick(sites.expressions);
        const target = parent as Record<string | number, unknown>;
        target[key] = hostileExpression(rng, sites, target[key]);
        break;
      }
      case 'name': {
        if (sites.names.length === 0) continue;
        const { slot, kind } = rng.pick(sites.names);
        const others = sites.names.filter((entry) => entry.kind !== kind);
        const nameAt = ({ slot: other }: { slot: Slot }) => String((other.parent as Record<string | number, unknown>)[other.key]);
        const replacement = rng.weighted<() => string>([
          [3, () => (others.length > 0 ? nameAt(rng.pick(others)) : 'nope')],
          [2, () => rng.pick(TRICKY_NAMES)],
          [1, () => `unknown${rng.below(100)}`],
        ])();
        (slot.parent as Record<string | number, unknown>)[slot.key] = replacement;
        break;
      }
      case 'insert-step': {
        const { list } = rng.pick(sites.stepLists);
        list.splice(rng.below(list.length + 1), 0, hostileStep(rng, sites));
        break;
      }
      case 'move-step': {
        const from = rng.pick(sites.stepLists);
        const to = rng.pick(sites.stepLists);
        if (from.list.length === 0) continue;
        const [moved] = from.list.splice(rng.below(from.list.length), 1);
        to.list.splice(rng.below(to.list.length + 1), 0, moved);
        break;
      }
      case 'delete-step': {
        const { list } = rng.pick(sites.stepLists);
        if (list.length === 0) continue;
        list.splice(rng.below(list.length), 1);
        break;
      }
      case 'duplicate-step': {
        const { list } = rng.pick(sites.stepLists);
        if (list.length === 0) continue;
        const index = rng.below(list.length);
        const copies = rng.pick([2, 40, 130, 300]);
        list.splice(index, 0, ...Array.from({ length: copies }, () => structuredClone(list[index])));
        break;
      }
      case 'account': {
        if (sites.constraints.length === 0) continue;
        const constraint = rng.pick(sites.constraints);
        rng.pick<() => void>([
          () => (constraint.signer = !constraint.signer),
          () => (constraint.writable = !constraint.writable),
          () => (constraint.executable = !constraint.executable),
          () => delete constraint.address,
          () => delete constraint.owner,
          () => (constraint.unsafeUnpinned = !constraint.unsafeUnpinned),
          () => (constraint.address = rng.pick([new Uint8Array(32), INSTRUCTIONS_SYSVAR_ADDRESS_BYTES, new Uint8Array(31)])),
          () => (constraint.registry = { name: rng.pick([...namesOf(sites, 'registry'), 'nope']), payer: rng.pick([...namesOf(sites, 'account'), 'nope']) }),
        ])();
        break;
      }
      case 'number': {
        if (sites.numbers.length === 0) continue;
        const { parent, key } = rng.pick(sites.numbers);
        (parent as Record<string | number, unknown>)[key] = rng.pick([0, 1, 255, 256, 1024, 1025, 0xffff_ffff, 2 ** 32, -1, 1.5, Number.MAX_SAFE_INTEGER]);
        break;
      }
      case 'loop': {
        if (sites.loops.length === 0) continue;
        const loop = rng.pick(sites.loops);
        rng.pick<() => void>([
          () => (loop.carry = [...((loop.carry as string[] | undefined) ?? []), rng.pick([...namesOf(sites, 'variable'), 'nope'])]),
          () => (loop.carry = [...((loop.carry as string[] | undefined) ?? []), ...((loop.carry as string[] | undefined) ?? [])]),
          () => (loop.max = rng.pick([0, 1, 255, 256])),
          () => (loop.count = hostileExpression(rng, sites, loop.count)),
          () => (loop.steps = []),
        ])();
        break;
      }
      case 'declarations': {
        rng.pick<() => void>([
          () => ((template.inputs as Mutable)[`big${rng.below(100)}`] = { type: 'bytes', maxLength: rng.pick([0, 1024, 1025]) }),
          () => (template.registries = Object.fromEntries(Array.from({ length: 9 }, (_, index) => [`r${index}`, { f: 'u64' }]))),
          () => (template.emitEvent = !template.emitEvent),
          () => (template.accountGroups = ['g', 'g']),
          () => {
            const batch = template.batch as Mutable | undefined;
            if (batch) batch.maxIterations = rng.pick([0, 1, 60, 61]);
            else template.batch = { maxIterations: 2, row: { r: { writable: true } } };
          },
        ])();
        break;
      }
    }
    mutations.push(operator);
  }
  return { template: template as unknown as TemplateInput, mutations };
}

/**
 * The document with every declared name and reference passed through `rename`, each record keeping
 * its order. Keys are defined as own properties, as `JSON.parse` makes them, so a name such as
 * `__proto__` is a key like any other here.
 */
export function renameDocument(template: TemplateInput, rename: (name: string) => string): TemplateInput {
  const copy = structuredClone(template) as unknown as Mutable;
  for (const { slot } of collectSites(copy).names) {
    const target = slot.parent as Record<string | number, unknown>;
    target[slot.key] = rename(String(target[slot.key]));
  }
  const renameKeys = (record: unknown): Mutable => {
    const renamed: Mutable = {};
    for (const [key, value] of Object.entries((record ?? {}) as Mutable)) {
      Object.defineProperty(renamed, rename(key), { value, enumerable: true, writable: true, configurable: true });
    }
    return renamed;
  };
  copy.inputs = renameKeys(copy.inputs);
  copy.accounts = renameKeys(copy.accounts);
  if (copy.registries) {
    const registries: Mutable = {};
    for (const [name, fields] of Object.entries(copy.registries as Mutable)) {
      Object.defineProperty(registries, rename(name), { value: renameKeys(fields), enumerable: true, writable: true, configurable: true });
    }
    copy.registries = registries;
  }
  const batch = copy.batch as Mutable | undefined;
  if (batch) {
    batch.row = renameKeys(batch.row);
    if (batch.rowInputs) batch.rowInputs = renameKeys(batch.rowInputs);
  }
  if (copy.accountGroups) copy.accountGroups = (copy.accountGroups as string[]).map(rename);
  return copy as unknown as TemplateInput;
}

/** Every distinct name in a document. */
function declaredNames(template: TemplateInput): string[] {
  const copy = template as unknown as Mutable;
  const names = new Set<string>(collectSites(copy).names.map(({ slot }) => String((slot.parent as Record<string | number, unknown>)[slot.key])));
  for (const record of [copy.inputs, copy.accounts, copy.registries, (copy.batch as Mutable | undefined)?.row, (copy.batch as Mutable | undefined)?.rowInputs]) {
    for (const key of Object.keys((record ?? {}) as Mutable)) names.add(key);
  }
  for (const fields of Object.values((copy.registries ?? {}) as Mutable)) for (const key of Object.keys(fields as Mutable)) names.add(key);
  for (const group of (copy.accountGroups as string[] | undefined) ?? []) names.add(group);
  return [...names];
}

/** Oracle 2, for names: renamed, some to names an object already has, the document compiles the same. */
export function renamingProblems(template: TemplateInput, compiled: Outcome, rng: Rng): string[] {
  const names = declaredNames(template);
  const tricky = rng.shuffle([...TRICKY_NAMES]);
  const mapping = new Map(names.map((name, index) => [name, index < tricky.length && rng.chance(0.5) ? tricky[index]! : `renamed${index}`]));
  // A tricky name taken twice would merge two declarations; keep the mapping one-to-one.
  const used = new Set<string>();
  for (const [name, target] of mapping) {
    if (used.has(target)) mapping.set(name, `renamed_${name}`);
    used.add(mapping.get(name)!);
  }
  const renamed = renameDocument(template, (name) => mapping.get(name) ?? name);
  const outcome = compileOutcome(renamed);
  const proto = [...mapping.values()].includes('__proto__');
  if ('ok' in compiled && 'ok' in outcome) {
    return sameBytes(compiled.ok.bytes, outcome.ok.bytes) ? [] : [`${proto ? 'known proto-name: ' : ''}renamed names compile to different bytes`];
  }
  if ('ok' in compiled !== 'ok' in outcome) {
    const what = 'ok' in compiled ? `refused after renaming: ${describeError((outcome as { error: unknown }).error)}` : 'compiles only after renaming';
    return [`${proto ? 'known proto-name: ' : ''}${what}`];
  }
  return [];
}

function countInvokes(steps: Step[]): number {
  return steps.reduce((sum, current) => sum + (current.kind === 'invoke' ? 1 : current.kind === 'forEach' || current.kind === 'repeat' ? countInvokes(current.steps) : 0), 0);
}

/** The finding an internal error belongs to, when it is one already triaged. */
export function knownFinding(template: TemplateInput, error: unknown): string | undefined {
  const message = error instanceof Error ? error.message : '';
  if (message === 'Expected u8' && countInvokes(template.steps as Step[]) > 255) return 'invoke-index';
  if (message === 'Expected u32') {
    let reaches = false;
    const visit = (node: unknown): void => {
      if (node === null || typeof node !== 'object') return;
      if (Array.isArray(node)) return node.forEach(visit);
      const record = node as Mutable;
      if (record.kind === 'accountData' && typeof record.offset === 'number' && record.offset + READ_WIDTH[record.type as ReadType] > 0xffff_ffff) reaches = true;
      Object.values(record).forEach(visit);
    };
    visit(template);
    if (reaches) return 'offset-u32';
  }
  if (/Maximum call stack size exceeded/.test(message)) return 'deep-nesting';
  return undefined;
}

// ---------------------------------------------------------------------------------------------
// Metamorphic copies of a document
// ---------------------------------------------------------------------------------------------

function mapSteps(steps: Step[], visit: (current: Step) => Step): Step[] {
  return steps.map((current) => {
    const mapped = visit(current);
    if (mapped.kind === 'forEach' || mapped.kind === 'repeat') return { ...mapped, steps: mapSteps(mapped.steps, visit) };
    return mapped;
  });
}

/**
 * The document with every `let` that binds an existing register (a variable, a literal, or an
 * input) given a register of its own: `select(true, value, value)` computes the same value into a
 * fresh register. The language says a binding keeps the value it had when it was made, so the
 * copy must run exactly as the original.
 */
export function materializeAliases(template: TemplateInput): TemplateInput {
  const copy = structuredClone(template);
  copy.steps = mapSteps(copy.steps as Step[], (current) => {
    if (current.kind !== 'let' || !['variable', 'literal', 'input'].includes(current.value.kind)) return current;
    return { ...current, value: expression.select(expression.bool(true), current.value, current.value) };
  });
  return copy;
}

/**
 * Whether a loop body binds a carried variable with `let` (directly or through another such
 * binding) and then assigns the variable. The binding shares the variable's register, so after the
 * assignment it reads the new value: finding `carried-alias`, below.
 */
export function carriedAliasHazard(steps: Step[]): boolean {
  for (const loop of steps) {
    if (loop.kind !== 'forEach' && loop.kind !== 'repeat') continue;
    const carried = new Set(loop.carry ?? []);
    const aliasOf = new Map<string, string>();
    for (const inner of loop.steps) {
      if (inner.kind === 'let' && inner.value.kind === 'variable') {
        const target = aliasOf.get(inner.value.name) ?? (carried.has(inner.value.name) ? inner.value.name : undefined);
        if (target !== undefined) aliasOf.set(inner.name, target);
      }
      if (inner.kind === 'assign' && [...aliasOf.values()].includes(inner.name)) return true;
    }
  }
  return false;
}

// ---------------------------------------------------------------------------------------------
// The corpus the Rust harnesses read
// ---------------------------------------------------------------------------------------------

interface CorpusVariant {
  payload: string;
  /**
   * The source map, run-length: `[pc, path]` where the step path changes. A failure's program
   * counter maps to its step, to compare failures between programs whose counters differ.
   */
  steps: [number, string][];
}

export interface CorpusCase {
  seed: number;
  size: FuzzCase['size'];
  /** Register values before reuse: above 64, the natural compile reuses registers on its own. */
  values: number;
  /** `compileTemplate(document)`. */
  natural: CorpusVariant;
  /** Compiled with `registerReuse: 'always'`, when the natural compile does not reuse registers already. */
  forced?: CorpusVariant;
  /** `materializeAliases(document)`, compiled. */
  materialized?: CorpusVariant;
  /** Known findings this document can trigger, so a harness counts them rather than failing. */
  hazards: string[];
  accounts: unknown[];
  /** The run data after the instruction tag; absent for a mutated document its world no longer fits. */
  data?: string;
  /** For a mutated document, what was done to it. */
  mutations?: string[];
}

function variant(compiled: CompiledTemplate): CorpusVariant {
  const steps: [number, string][] = [];
  for (const entry of compiled.sourceMap) {
    const path = entry.label ? `${entry.path} (${entry.label})` : entry.path;
    if (steps.at(-1)?.[1] !== path) steps.push([entry.pc, path]);
  }
  return { payload: hex(compiled.bytes), steps };
}

function corpusAccount(world: WorldAccount): unknown {
  const source = world.source;
  const base = { role: world.role, signer: world.signer, writable: world.writable };
  switch (source.kind) {
    case 'program':
      return { ...base, program: source.program };
    case 'sysvar':
      return { ...base, sysvar: 'instructions' };
    case 'registry':
      return { ...base, registry: { index: source.index, key: hex(source.key) } };
    case 'stored':
      return { ...base, address: hex(source.address), lamports: source.lamports.toString(), owner: hex(source.owner), data: hex(source.data) };
  }
}

// ---------------------------------------------------------------------------------------------
// One case through every oracle
// ---------------------------------------------------------------------------------------------

/**
 * The register values a program writes, as the compiler counts them before reuse: one for every
 * destination, except an assignment's move into a register its loop carries, which updates a value.
 */
export function valuesBeforeReuse(program: DecodedProgram): number {
  let values = 0;
  for (let pc = 0; pc < program.instructions.length; pc += 1) {
    const record = program.instructions[pc]!;
    if (record.opcode !== opcode.forEach && record.opcode !== opcode.repeat) {
      if (record.dst !== NO_INDEX) values += 1;
      continue;
    }
    for (let body = pc + 1; body <= pc + record.a; body += 1) {
      const inner = program.instructions[body]!;
      const carried = inner.dst < 64 && ((record.immediate >> BigInt(inner.dst)) & 1n) === 1n;
      if (inner.dst !== NO_INDEX && !(inner.opcode === opcode.move && carried)) values += 1;
    }
    pc += record.a;
  }
  return values;
}

export interface CaseReport {
  seed: number;
  size: FuzzCase['size'];
  compiled: boolean;
  /** Internal errors, nondeterminism and reuse problems: each one a finding. */
  problems: string[];
  /** Findings already triaged, each with its own skipped test: counted, not failed. */
  known: string[];
  rejection?: string;
  reused: 'natural' | 'forced' | 'none';
  corpus?: CorpusCase;
  /** For a mutated document, what was done to it. */
  mutations?: string[];
}

const sameBytes = (left: Uint8Array, right: Uint8Array) => left.length === right.length && left.every((byte, index) => byte === right[index]);

export function checkCase(fuzzCase: FuzzCase, options: { corpus?: boolean; mutations?: string[] } = {}): CaseReport {
  const report: CaseReport = {
    seed: fuzzCase.seed,
    size: fuzzCase.size,
    compiled: false,
    problems: [],
    known: [],
    reused: 'none',
    ...(options.mutations ? { mutations: options.mutations } : {}),
  };
  const natural = compileOutcome(fuzzCase.template);
  // Oracle 2, for names: the same document under other names compiles the same.
  for (const problem of renamingProblems(fuzzCase.template, natural, new Rng(fuzzCase.seed ^ 0x0a11a5))) {
    if (problem.startsWith('known proto-name')) report.known.push('proto-name');
    else report.problems.push(problem);
  }
  if ('error' in natural) {
    report.rejection = describeError(natural.error);
    if (!isCleanRejection(natural.error)) {
      const known = knownFinding(fuzzCase.template, natural.error);
      if (known) report.known.push(known);
      else report.problems.push(`internal error: ${report.rejection}`);
    }
    // A refusal must be deterministic too.
    const again = compileOutcome(fuzzCase.template);
    if (!('error' in again) || describeError(again.error) !== report.rejection) report.problems.push('a second compile did not refuse the same way');
    return report;
  }
  report.compiled = true;
  const compiled = natural.ok;

  // Oracle 2: deterministic.
  const again = compileOutcome(fuzzCase.template);
  if ('error' in again || !sameBytes(again.ok.bytes, compiled.bytes)) report.problems.push('compiling twice gave different results');
  const reparsed = compileOutcome(compiled.template);
  if ('error' in reparsed || !sameBytes(reparsed.ok.bytes, compiled.bytes)) report.problems.push('compiling the parsed template gave different results');
  try {
    expect(inspectTemplate(compiled.bytes)).toEqual(compiled.stats);
  } catch {
    report.problems.push('inspectTemplate disagrees with the compiler stats');
  }

  // Oracle 3: register reuse. A natural compile above 64 values has reused registers already.
  const decoded = decodeProgram(compiled.bytes);
  const values = valuesBeforeReuse(decoded);
  if (values <= 64 && decoded.registerCount !== values) report.problems.push(`${values} values but ${decoded.registerCount} registers without reuse`);
  if (decoded.registerCount > 64) report.problems.push(`the header declares ${decoded.registerCount} registers`);
  let forced: CompiledTemplate | undefined;
  if (values > 64) {
    // The natural compile renumbered on its own; compare it with the numbering it came from.
    report.reused = 'natural';
    const plain = compileOutcome(fuzzCase.template, { registerReuse: 'never' });
    if ('error' in plain) report.problems.push(`the one-register-per-value compile failed: ${describeError(plain.error)}`);
    else report.problems.push(...reuseProblems(plain.ok.bytes, compiled.bytes, fuzzCase.seed).map((problem) => `natural reuse: ${problem}`));
  } else if (values > 0) {
    const outcome = compileOutcome(fuzzCase.template, { registerReuse: 'always' });
    if ('error' in outcome) {
      report.problems.push(`forced reuse refused a template that compiles: ${describeError(outcome.error)}`);
    } else {
      forced = outcome.ok;
      report.reused = 'forced';
      report.problems.push(...reuseProblems(compiled.bytes, forced.bytes, fuzzCase.seed).map((problem) => `forced reuse: ${problem}`));
      const forcedAgain = compileOutcome(fuzzCase.template, { registerReuse: 'always' });
      if ('error' in forcedAgain || !sameBytes(forcedAgain.ok.bytes, forced.bytes)) report.problems.push('forced reuse is not deterministic');
    }
  }

  let materialized: CompiledTemplate | undefined;
  const copy = compileOutcome(materializeAliases(fuzzCase.template));
  if ('error' in copy) {
    if (!isCleanRejection(copy.error)) report.problems.push(`materialized copy: internal error: ${describeError(copy.error)}`);
  } else {
    materialized = copy.ok;
  }

  if (options.corpus) {
    // A mutated document may no longer fit its world's inputs; it is then verified but not run.
    let runData: Uint8Array | undefined;
    try {
      runData = encodeRunInputs(compiled, fuzzCase.inputs, {
        ...(fuzzCase.rows.length > 0 && compiled.rowInputOrder.length > 0 ? { rows: fuzzCase.rows } : {}),
        groupLengths: fuzzCase.groupLengths,
      });
    } catch (error) {
      if (!options.mutations) {
        report.problems.push(`the generator's inputs do not encode: ${describeError(error)}`);
        return report;
      }
    }
    report.corpus = {
      seed: fuzzCase.seed,
      size: fuzzCase.size,
      values,
      natural: variant(compiled),
      ...(forced ? { forced: variant(forced) } : {}),
      ...(materialized ? { materialized: variant(materialized) } : {}),
      hazards: carriedAliasHazard(fuzzCase.template.steps as Step[]) ? ['carried-alias'] : [],
      accounts: fuzzCase.accounts.map(corpusAccount),
      ...(runData ? { data: hex(runData) } : {}),
      ...(options.mutations ? { mutations: options.mutations } : {}),
    };
  }
  return report;
}

// ---------------------------------------------------------------------------------------------
// Findings, minimized. Each has a skipped test below that fails today.
// ---------------------------------------------------------------------------------------------

/**
 * Finding `carried-alias` (medium): inside a loop body, `let` or `snapshot` of a carried variable
 * binds the variable's own register, since a `let` of a variable compiles to no instruction. A
 * later `assign` writes that register, so from then on the binding reads the new value. The
 * language says a binding evaluates once and keeps its value, and `snapshot` is named for
 * before-and-after checks. `sharesRegister` copies a carried variable that a binding shares when
 * the loop starts, but nothing does when the binding is made inside the body.
 *
 * Here each pass snapshots the total, adds one, and requires the total to have grown by one from
 * the snapshot. Every pass should pass; the compiled check compares the new total plus one with
 * itself and fails the first pass. A check of the form `total - before <= cap` passes always.
 */
export function carriedAliasDocument(): TemplateInput {
  return {
    inputs: { passes: { type: 'u64' } },
    accounts: {},
    steps: [
      step.let('total', u64(0)),
      step.repeat(
        expression.input('passes'),
        [
          step.snapshot('before', expression.variable('total')),
          step.assign('total', expression.add(expression.variable('total'), u64(1))),
          step.require(expression.equal(expression.add(expression.variable('before'), u64(1)), expression.variable('total')), 'grewByOne'),
        ],
        { max: 4, carry: ['total'] },
      ),
      step.setReturnData([data.encode('u64', expression.variable('total'))]),
    ],
  };
}

const invokeOf = (program: string): Step => step.invoke({ program: fixed(program), accounts: [], data: [] });

/**
 * Finding `invoke-index` (low): a template with more than 256 invoke steps fails with a bare
 * `RangeError: Expected u8`. An invoke names its CPI descriptor by a one-byte index, and the
 * compiler writes that byte as each invoke compiles, before its checks on the CPI count (`can
 * expand to N CPIs`) and the descriptor table (`table count exceeds the wire format`) run, after
 * every step. Such a template is invalid anyway; the error should say why.
 */
export function manyInvokesDocument(): TemplateInput {
  return {
    accounts: { program: { executable: true, address: new Uint8Array(32).fill(9) } },
    steps: [
      invokeOf('program'),
      ...Array.from({ length: 4 }, () => step.repeat(u64(1), Array.from({ length: 64 }, () => invokeOf('program')), { max: 1 })),
    ],
  };
}

/**
 * Finding `offset-u32` (low): a fixed read offset near the top of the `u32` range fails with a bare
 * `RangeError: Expected u32`. The compiler raises the account's minimum data length to the read's
 * end, `offset + width`, which can pass `u32` while the offset itself is a valid `u32`; the
 * account record's writer then throws. The schema accepts the offset, so the error should say the
 * read reaches past what an account's data length can be.
 */
export function largeOffsetDocument(): TemplateInput {
  return {
    accounts: { data: { owner: new Uint8Array(32).fill(1) } },
    steps: [step.require(expression.equal(expression.accountData(fixed('data'), 0xffff_fffc, 'u64'), u64(0)))],
  };
}

/**
 * Finding `deep-nesting` (low): an expression nested about 1,500 levels deep throws `RangeError:
 * Maximum call stack size exceeded` from Zod's recursive parse of `ExpressionSchema`, before any
 * limit applies. At 1,000 levels the compiler refuses it cleanly: more than 128 instructions.
 */
export function deepNestingDocument(depth = 2_000): TemplateInput {
  let value = expression.bool(true);
  for (let level = 0; level < depth; level += 1) value = expression.not(value);
  return { accounts: {}, steps: [step.require(value)] };
}

/**
 * Finding `proto-name` (low): `__proto__` matches the identifier pattern, but a record parsed by
 * Zod assigns it as `result[key] = value`, which sets the object's prototype instead of adding the
 * key. A document read with `JSON.parse` that declares an input, account, registry or field named
 * `__proto__` loses the declaration without an error: here the step that reads the input fails
 * with "Unknown input: __proto__". A registry whose only field is `__proto__` is refused as
 * holding no bytes, and an unused declaration vanishes silently, shifting what follows it.
 */
export function protoNameDocument(): TemplateInput {
  const document = JSON.parse('{"inputs": {"__proto__": {"type": "u64"}}, "accounts": {}}') as TemplateInput;
  document.steps = [step.require(expression.equal(expression.input('__proto__'), u64(1)))];
  return document;
}

// ---------------------------------------------------------------------------------------------
// Triage helpers
// ---------------------------------------------------------------------------------------------

/** A document and its world as JSON, with bigints and bytes spelled out. */
export function showDocument(fuzzCase: FuzzCase): string {
  const replacer = (_key: string, value: unknown) =>
    typeof value === 'bigint' ? `${value}n` : value instanceof Uint8Array ? `0x${hex(value)}` : value;
  return JSON.stringify(fuzzCase, replacer, 1);
}

const OPCODE_NAMES = new Map<number, string>(Object.entries(opcode).map(([name, code]) => [code, name]));

/** One line per instruction: pc, opcode, operands, immediate and the step it came from. */
export function disassemble(compiled: CompiledTemplate): string {
  const program = decodeProgram(compiled.bytes);
  const lines = [`${program.registerCount} registers`];
  program.instructions.forEach((record, pc) => {
    const source = compiled.sourceMap.find((entry) => entry.pc === pc);
    const operands = (['dst', 'a', 'b', 'c'] as const).map((field) => `${field}=${record[field] === NO_INDEX ? '-' : record[field]}`);
    lines.push(
      `${pc}\t${OPCODE_NAMES.get(record.opcode) ?? record.opcode}\t${operands.join(' ')}${record.flags ? ` flags=${record.flags}` : ''} imm=0x${record.immediate.toString(16)}\t${source?.path ?? ''}${source?.label ? ` (${source.label})` : ''}`,
    );
  });
  program.segments.forEach((segment, index) => lines.push(`segment ${index}: kind ${segment.kind} register ${segment.register === NO_INDEX ? '-' : segment.register}`));
  return lines.join('\n');
}

// ---------------------------------------------------------------------------------------------
// The runs
// ---------------------------------------------------------------------------------------------

const environment = (name: string) => process.env[name];
const SEED_START = Number(environment('FUZZ_SEED_START') ?? 1);
const SEEDS = Number(environment('FUZZ_SEEDS') ?? 400);
const CORPUS_PATH = environment('FUZZ_CORPUS');
/** Mutated copies checked per seed: documents that break the rules on purpose. */
const MUTANTS_PER_SEED = Number(environment('FUZZ_MUTANTS') ?? 2);
const COMMITTED_CORPUS = fileURLToPath(new URL('../../../fixtures/compiler-fuzz-corpus.json', import.meta.url));
/** The committed corpus: up to this many cases, picked from these seeds to cover the most features. */
const COMMITTED_CASES = 12;
const COMMITTED_CANDIDATES = 400;
/** The minimized findings' payloads and run data, for the Rust regression tests. */
const FINDINGS_FIXTURE = fileURLToPath(new URL('../../../fixtures/compiler-fuzz-findings.json', import.meta.url));

function writeFindings(): void {
  const passes = new Uint8Array(8);
  new DataView(passes.buffer).setBigUint64(0, 2n, true);
  const findings = {
    command: 'UPDATE_COMPILER_FUZZ_CORPUS=1 pnpm --dir clients/js exec vitest run src/compiler-fuzz.test.ts',
    'carried-alias': {
      document: 'carriedAliasDocument() in clients/js/src/compiler-fuzz.test.ts',
      payload: hex(compileTemplate(carriedAliasDocument()).bytes),
      // Two passes: by the language's rules the run succeeds and returns 2.
      data: hex(passes),
      returns: '0200000000000000',
    },
  };
  writeFileSync(FINDINGS_FIXTURE, `${JSON.stringify(findings, null, 1)}\n`);
}

/** The features a document uses: step and expression kinds, and how its registers are numbered. */
function features(fuzzCase: FuzzCase, corpusCase: CorpusCase): Set<string> {
  const found = new Set<string>([`size:${fuzzCase.size}`, corpusCase.values > 64 ? 'reuse:natural' : 'reuse:forced']);
  for (const hazard of corpusCase.hazards) found.add(`hazard:${hazard}`);
  const visit = (node: unknown): void => {
    if (node === null || typeof node !== 'object') return;
    if (Array.isArray(node)) return node.forEach(visit);
    const record = node as Record<string, unknown>;
    if (typeof record.kind === 'string') found.add(`${record.kind}${typeof record.op === 'string' ? `:${record.op}` : ''}`);
    Object.values(record).forEach(visit);
  };
  visit(fuzzCase.template.steps);
  visit(Object.values(fuzzCase.template.accounts));
  if (fuzzCase.template.batch) found.add('batch');
  if ((fuzzCase.template.accountGroups ?? []).length > 0) found.add('accountGroups');
  return found;
}

/** One case per line, so `common/tests/compiler_corpus.rs` can read it without a JSON parser. */
function writeCorpus(path: string, cases: CorpusCase[], command: string): void {
  const header = JSON.stringify({ version: 1, generator: 'clients/js/src/compiler-fuzz.test.ts', command });
  const lines = cases.map((corpusCase) => JSON.stringify(corpusCase));
  writeFileSync(path, `${header.slice(0, -1)},"cases":[\n${lines.join(',\n')}\n]}\n`);
}

describe('compiler fuzz', () => {
  test(`seeds ${SEED_START}..${SEED_START + SEEDS - 1}: compile or refuse cleanly, deterministically, and reuse registers faithfully`, { timeout: 3_600_000 }, () => {
    const reports: CaseReport[] = [];
    const corpus: CorpusCase[] = [];
    const mutants: CaseReport[] = [];
    for (let seed = SEED_START; seed < SEED_START + SEEDS; seed += 1) {
      let report: CaseReport;
      let fuzzCase: FuzzCase | undefined;
      try {
        fuzzCase = generateCase(seed);
        report = checkCase(fuzzCase, { corpus: CORPUS_PATH !== undefined });
      } catch (error) {
        // The generator or an oracle threw: a harness bug, reported with its seed.
        report = { seed, size: 'small', compiled: false, problems: [`harness: ${describeError(error)}`], known: [], reused: 'none' };
      }
      reports.push(report);
      if (report.corpus) corpus.push(report.corpus);
      for (let index = 0; fuzzCase && index < MUTANTS_PER_SEED; index += 1) {
        let mutant: CaseReport;
        try {
          const { template, mutations } = mutateDocument(fuzzCase.template, new Rng(seed * 7_919 + index));
          mutant = checkCase({ ...fuzzCase, template }, { corpus: CORPUS_PATH !== undefined, mutations });
        } catch (error) {
          mutant = { seed, size: fuzzCase.size, compiled: false, problems: [`harness (mutant ${index}): ${describeError(error)}`], known: [], reused: 'none' };
        }
        mutants.push(mutant);
        if (mutant.corpus) corpus.push(mutant.corpus);
      }
    }
    if (CORPUS_PATH) writeCorpus(CORPUS_PATH, corpus, `FUZZ_SEED_START=${SEED_START} FUZZ_SEEDS=${SEEDS}`);

    const compiled = reports.filter((report) => report.compiled);
    const histogram = (list: CaseReport[]) =>
      Object.entries(
        list
          .filter((report) => report.rejection)
          .reduce<Record<string, number>>((counts, report) => {
            const key = report.rejection!.replace(/\d+/g, 'N').slice(0, 90);
            counts[key] = (counts[key] ?? 0) + 1;
            return counts;
          }, {}),
      ).sort((x, y) => y[1] - x[1]);
    const known = [...reports, ...mutants].flatMap((report) => report.known).reduce<Record<string, number>>((counts, id) => {
      counts[id] = (counts[id] ?? 0) + 1;
      return counts;
    }, {});
    const summary = {
      seeds: `${SEED_START}..${SEED_START + SEEDS - 1}`,
      mutants: { checked: mutants.length, compiled: mutants.filter((report) => report.compiled).length, refusals: histogram(mutants).slice(0, 40) },
      knownFindings: known,
      compiled: compiled.length,
      refused: reports.length - compiled.length,
      forcedReuse: reports.filter((report) => report.reused === 'forced').length,
      naturalReuse: compiled.filter((report) => report.reused === 'natural').length,
      bySize: Object.fromEntries((['small', 'medium', 'large'] as const).map((size) => [size, reports.filter((report) => report.size === size).length])),
      refusals: Object.entries(
        reports
          .filter((report) => report.rejection)
          .reduce<Record<string, number>>((counts, report) => {
            const key = report.rejection!.replace(/\d+/g, 'N').slice(0, 90);
            counts[key] = (counts[key] ?? 0) + 1;
            return counts;
          }, {}),
      ).sort((x, y) => y[1] - x[1]),
    };
    console.log(JSON.stringify(summary, null, 1));
    const problems = [...reports, ...mutants]
      .filter((report) => report.problems.length > 0)
      .map((report) => `seed ${report.seed}${report.mutations ? ` mutant (${report.mutations.join(', ')})` : ''}: ${report.problems.join('; ')}`);
    expect(problems).toEqual([]);
    // The generator must mostly write documents the compiler accepts, or the oracles test little.
    expect(compiled.length).toBeGreaterThanOrEqual(reports.length * 0.6);
  });

  test.runIf(environment('UPDATE_COMPILER_FUZZ_CORPUS') === '1')('rewrite the committed corpus', () => {
    // Greedy: each pick adds the most features not yet covered, the smaller payload breaking ties.
    const candidates = Array.from({ length: COMMITTED_CANDIDATES }, (_, index) => {
      const fuzzCase = generateCase(index + 1);
      const corpusCase = checkCase(fuzzCase, { corpus: true }).corpus;
      return corpusCase ? { corpusCase, features: features(fuzzCase, corpusCase), size: JSON.stringify(corpusCase).length } : undefined;
    }).filter((candidate) => candidate !== undefined);
    const covered = new Set<string>();
    const picked: CorpusCase[] = [];
    while (picked.length < COMMITTED_CASES) {
      const score = (candidate: (typeof candidates)[number]) => [...candidate.features].filter((feature) => !covered.has(feature)).length;
      const best = candidates
        .filter((candidate) => !picked.includes(candidate.corpusCase) && score(candidate) > 0)
        .sort((x, y) => score(y) - score(x) || x.size - y.size)[0];
      if (!best) break;
      picked.push(best.corpusCase);
      for (const feature of best.features) covered.add(feature);
    }
    // Then the forced-reuse cases whose renumbering does the most: many values, few registers.
    const renumbering = candidates
      .filter((candidate) => candidate.corpusCase.forced && !picked.includes(candidate.corpusCase))
      .map((candidate) => ({ candidate, saved: candidate.corpusCase.values - decodeProgram(Buffer.from(candidate.corpusCase.forced!.payload, 'hex')).registerCount }))
      .sort((x, y) => y.saved - x.saved || x.candidate.size - y.candidate.size);
    for (const { candidate } of renumbering) {
      if (picked.length >= COMMITTED_CASES) break;
      picked.push(candidate.corpusCase);
    }
    picked.sort((x, y) => x.seed - y.seed);
    writeCorpus(COMMITTED_CORPUS, picked, 'UPDATE_COMPILER_FUZZ_CORPUS=1 pnpm --dir clients/js exec vitest run src/compiler-fuzz.test.ts');
    writeFindings();
  });

  // Triage: `FUZZ_SHOW=<seed>` prints a seed's document, world and disassembly.
  test.runIf(environment('FUZZ_SHOW') !== undefined)('show one case', () => {
    const fuzzCase = generateCase(Number(environment('FUZZ_SHOW')));
    console.log(showDocument(fuzzCase));
    const outcome = compileOutcome(fuzzCase.template);
    if ('error' in outcome) console.log(`refused: ${describeError(outcome.error)}`);
    else console.log(disassemble(outcome.ok));
  });

  test('the generator covers the language', () => {
    const seen = new Set<string>();
    const visitExpression = (node: unknown): void => {
      if (node === null || typeof node !== 'object') return;
      if (Array.isArray(node)) return node.forEach(visitExpression);
      const record = node as Record<string, unknown>;
      if (typeof record.kind === 'string') seen.add(`expression:${record.kind}${record.kind === 'binary' ? `:${String(record.op)}` : ''}`);
      Object.values(record).forEach(visitExpression);
    };
    const visitSteps = (steps: Step[]) => {
      for (const current of steps) {
        seen.add(`step:${current.kind}${current.kind === 'invoke' && current.when ? ':when' : ''}`);
        visitExpression(current);
        if (current.kind === 'forEach' || current.kind === 'repeat') visitSteps(current.steps);
      }
    };
    let reused = 0;
    for (let seed = 1; seed <= 300; seed += 1) {
      const fuzzCase = generateCase(seed);
      visitSteps(fuzzCase.template.steps as Step[]);
      for (const constraint of Object.values(fuzzCase.template.accounts)) if (constraint.registry) seen.add('account:registry');
      if (fuzzCase.template.batch) seen.add('batch');
      if (Object.keys(fuzzCase.template.batch?.rowInputs ?? {}).length > 0) seen.add('rowInputs');
      if ((fuzzCase.template.accountGroups ?? []).length > 0) seen.add('accountGroups');
      for (const definition of Object.values(fuzzCase.template.inputs ?? {})) seen.add(`input:${definition.type}`);
      const outcome = compileOutcome(fuzzCase.template);
      if ('ok' in outcome && valuesBeforeReuse(decodeProgram(outcome.ok.bytes)) > 64) reused += 1;
    }
    for (const required of [
      'input:bool', 'input:u64', 'input:i64', 'input:u128', 'input:pubkey', 'input:bytes',
      'batch', 'rowInputs', 'accountGroups', 'account:registry',
      'expression:input', 'expression:rowInput', 'expression:variable', 'expression:literal', 'expression:accountField',
      'expression:accountData', 'expression:returnData', 'expression:clock', 'expression:loopIndex', 'expression:pda',
      'expression:binary:add', 'expression:binary:subtract', 'expression:binary:multiply', 'expression:binary:divide',
      'expression:binary:remainder', 'expression:binary:shiftLeft', 'expression:binary:bitXor', 'expression:binary:lessThan',
      'expression:binary:equal', 'expression:binary:and', 'expression:multiplyDivide', 'expression:powerOfTen',
      'expression:not', 'expression:select', 'expression:cast', 'expression:instructionCount',
      'expression:currentInstructionIndex', 'expression:instruction', 'expression:instructionAccount',
      'expression:instructionData', 'expression:instructionDataBytes', 'expression:accountDataBytes',
      'expression:bytesLength', 'expression:registry',
      'step:let', 'step:require', 'step:invoke', 'step:invoke:when', 'step:assign', 'step:forEach', 'step:repeat',
      'step:emit', 'step:setReturnData', 'step:setRegistry',
    ]) {
      expect(seen, required).toContain(required);
    }
    // Some documents need more than 64 registers, so `reuseRegisters` runs without being forced.
    expect(reused).toBeGreaterThan(10);
  });
});

/**
 * The findings, minimized. Each test states what the compiler should do and fails today, so it is
 * skipped; remove the `.skip` with the fix. `tests/ballista` runs the carried-alias template too.
 */
describe('compiler fuzz findings', () => {
  // BUG carried-alias: a `let` of a carried variable in a loop body shares its register, so the
  // body's `assign` changes the binding. See `carriedAliasDocument`.
  test.skip('carried-alias: a snapshot of a carried value keeps its value after the assignment', () => {
    const program = decodeProgram(compileTemplate(carriedAliasDocument()).bytes);
    const assignment = program.instructions.findIndex((record) => record.opcode === opcode.move);
    // After the move, the first add is `before + 1`: it must not read the register the move wrote.
    const beforePlusOne = program.instructions.slice(assignment + 1).find((record) => record.opcode === opcode.add)!;
    expect(beforePlusOne.a).not.toBe(program.instructions[assignment]!.dst);
  });

  // BUG invoke-index: more than 256 invokes fail with a bare `Expected u8`. See `manyInvokesDocument`.
  test.skip('invoke-index: more than 256 invokes are refused with a clear error', () => {
    const outcome = compileOutcome(manyInvokesDocument());
    expect('error' in outcome && isCleanRejection(outcome.error)).toBe(true);
  });

  // BUG offset-u32: a fixed read whose end passes u32 fails with a bare `Expected u32`. See
  // `largeOffsetDocument`.
  test.skip('offset-u32: a fixed read past any data length is refused with a clear error', () => {
    const outcome = compileOutcome(largeOffsetDocument());
    expect('error' in outcome && isCleanRejection(outcome.error)).toBe(true);
  });

  // BUG deep-nesting: a deeply nested expression overflows the stack in Zod's parse. See
  // `deepNestingDocument`.
  test.skip('deep-nesting: a deeply nested expression is refused with a clear error', () => {
    const outcome = compileOutcome(deepNestingDocument());
    expect('error' in outcome && isCleanRejection(outcome.error)).toBe(true);
  });

  // BUG proto-name: a declaration named `__proto__` vanishes. See `protoNameDocument`.
  test.skip('proto-name: an input named __proto__ is declared, or refused by name', () => {
    const outcome = compileOutcome(protoNameDocument());
    if ('ok' in outcome) expect(outcome.ok.inputOrder).toContain('__proto__');
    else expect(describeError(outcome.error)).toMatch(/__proto__.*(reserved|not allowed)|identifier/);
  });

  // Today's behaviour, so a change to it is noticed: each finding reproduces.
  test('every finding reproduces today', () => {
    const program = decodeProgram(compileTemplate(carriedAliasDocument()).bytes);
    const assignment = program.instructions.findIndex((record) => record.opcode === opcode.move);
    const beforePlusOne = program.instructions.slice(assignment + 1).find((record) => record.opcode === opcode.add)!;
    expect(beforePlusOne.a).toBe(program.instructions[assignment]!.dst);
    expect(() => compileTemplate(manyInvokesDocument())).toThrow(/^Expected u8$/);
    expect(() => compileTemplate(largeOffsetDocument())).toThrow(/^Expected u32$/);
    expect(() => compileTemplate(deepNestingDocument())).toThrow(/Maximum call stack size exceeded/);
    expect(() => compileTemplate(protoNameDocument())).toThrow('Unknown input: __proto__');
  });
});
