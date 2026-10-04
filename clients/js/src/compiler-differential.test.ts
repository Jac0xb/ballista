/**
 * Writes the compiler fuzzer's documents, with what `compileTemplate` makes of each, as JSON for
 * the Rust compiler's differential test (`clients/rust/tests/template_differential.rs`), which
 * compiles each with `ballista_sdk::template` and compares the bytes.
 *
 *     COMPILER_DIFF_OUT=/tmp/corpus.json COMPILER_DIFF_SEEDS=2000 \
 *       pnpm --dir clients/js exec vitest run compiler-differential -t differential
 *     COMPILER_DIFF_CORPUS=/tmp/corpus.json cargo test -p ballista-sdk --test template_differential
 *
 * A document is JSON with two tagged values: `{ "$bigint": "12" }` and `{ "$bytes": "<hex>" }`.
 * Object keys keep their order, which is declaration order.
 *
 * Without `COMPILER_DIFF_OUT` this file does nothing: the fuzz suite is imported only to write,
 * because importing it registers its tests here too (`-t differential` skips them).
 */
import { writeFileSync } from 'node:fs';

import { describe, expect, test } from 'vitest';

import {
  compileTemplate,
  data,
  expression,
  step,
  type Expression,
  type GroupFilter,
  type InputDefinition,
  type Step,
  type TemplateInput,
} from './index.js';

const environment = (name: string) => process.env[name];
const OUT = environment('COMPILER_DIFF_OUT');
const SEED_START = Number(environment('COMPILER_DIFF_SEED_START') ?? 1);
const SEEDS = Number(environment('COMPILER_DIFF_SEEDS') ?? 2_000);
const MUTANTS = Number(environment('COMPILER_DIFF_MUTANTS') ?? 1);
const GROUP_DOCUMENTS = Number(environment('COMPILER_DIFF_GROUP_DOCUMENTS') ?? 1_000);

const fuzz = OUT ? await import('./compiler-fuzz.test.js') : undefined;

export interface DifferentialEntry {
  name: string;
  document: TemplateInput;
  /** `compileTemplate(document).bytes`, hex, when it compiles. */
  bytes?: string;
  /** Register values before reuse: above 64, the compile reused registers. */
  values?: number;
  /** The error's message, when it does not. */
  error?: string;
  /** Uses a group expression. */
  groups: boolean;
  /** Has the carried-alias pattern (`carriedAliasHazard`). */
  hazard: boolean;
}

const hex = (bytes: Uint8Array) => Buffer.from(bytes).toString('hex');

export function documentJson(value: unknown): string {
  return JSON.stringify(value, (_key, current: unknown) => {
    if (typeof current === 'bigint') return { $bigint: current.toString() };
    if (current instanceof Uint8Array) return { $bytes: hex(current) };
    return current;
  });
}

function entry(name: string, document: TemplateInput): DifferentialEntry {
  const text = documentJson(document);
  const groups = /"kind":"group(Length|Any|Count)"/.test(text);
  let hazard = false;
  try {
    hazard = fuzz!.carriedAliasHazard((document.steps ?? []) as Step[]);
  } catch {
    // A mutated document's steps need not be well formed.
  }
  try {
    const bytes = compileTemplate(document).bytes;
    const values = fuzz!.valuesBeforeReuse(fuzz!.decodeProgram(bytes));
    return { name, document, bytes: hex(bytes), values, groups, hazard };
  } catch (error) {
    return { name, document, error: error instanceof Error ? error.message : String(error), groups, hazard };
  }
}

type Rng = InstanceType<NonNullable<typeof fuzz>['Rng']>;
type MatchKind = 'u64' | 'i64' | 'u128' | 'pubkey' | 'bool' | 'bytes';

/**
 * A document built around group expressions, for what the fuzzer's generator leaves out: three
 * and four match segments and excepts, every match type, floors below and above the segments,
 * filter values live across many other values (so the natural compile reuses registers), filters
 * inside loops, and the carried-alias pattern next to a group count.
 */
function groupDocument(rng: Rng): TemplateInput {
  const inputs: Record<string, InputDefinition> = {
    n: { type: 'u64' },
    seed: { type: 'u64' },
  };
  const steps: Step[] = [];
  const valueOf = (kind: MatchKind): Expression => {
    const name = `v${Object.keys(inputs).length}`;
    switch (rng.below(3)) {
      case 0:
        if (kind === 'bytes') return expression.bytes(rng.bytes(rng.range(1, 8)));
        if (kind === 'pubkey') return expression.pubkey(rng.bytes(32));
        if (kind === 'bool') return expression.bool(rng.chance(0.5));
        return expression[kind](rng.bits(kind === 'u128' ? 128 : 63));
      case 1:
        inputs[name] = (kind === 'bytes' ? { type: 'bytes', maxLength: rng.range(1, 8) } : { type: kind }) as InputDefinition;
        return expression.input(name);
      default:
        // A value bound early and read again late, so it stays live across the group expression.
        inputs[name] = (kind === 'bytes' ? { type: 'bytes', maxLength: rng.range(1, 8) } : { type: kind }) as InputDefinition;
        steps.push(step.let(`${name}Bound`, expression.input(name)));
        return expression.variable(`${name}Bound`);
    }
  };
  const filter = (): GroupFilter => {
    const programs = [rng.bytes(32)];
    if (rng.chance(0.4)) programs.push(rng.bytes(32));
    const match = Array.from({ length: rng.weighted([[2, 1], [2, 2], [3, 3], [3, 4], [0.3, 5]] as const) }, () => ({
      offset: rng.chance(0.1) ? 0xffff - rng.below(40) : rng.below(300),
      equals: valueOf(rng.weighted([[4, 'u64'], [2, 'i64'], [2, 'u128'], [4, 'pubkey'], [2, 'bool'], [1, 'bytes']] as const)),
    }));
    const exceptKeys = Array.from({ length: rng.weighted([[2, 0], [1, 1], [1, 2], [1, 3], [2, 4], [0.2, 5]] as const) }, () =>
      rng.chance(0.5) ? expression.accountKey(rng.pick(['a', 'b'])) : valueOf('pubkey'),
    );
    return {
      programs,
      match,
      ...(exceptKeys.length > 0 ? { exceptKeys } : {}),
      ...(rng.chance(0.4) ? { minDataLength: rng.below(400) } : {}),
    };
  };
  const group = () => rng.pick(['g', 'h']);
  const fillers = rng.weighted([[2, 0], [1, 30], [2, 62], [2, 75]] as const);
  for (let index = 0; index < fillers; index += 1) {
    const previous = index === 0 ? expression.input('seed') : expression.variable(`f${index - 1}`);
    steps.push(step.let(`f${index}`, expression.add(previous, expression.input('seed'))));
  }
  steps.push(step.let('count', expression.groupCount(group(), filter())));
  steps.push(step.require(expression.lessThanOrEqual(expression.variable('count'), expression.groupLength(group()))));
  if (rng.chance(0.5)) steps.push(step.require(expression.or(expression.groupAny(group(), filter()), expression.bool(true))));
  if (rng.chance(0.5)) {
    const body: Step[] = [];
    if (rng.chance(0.5)) body.push(step.let('before', expression.variable('total')));
    body.push(step.assign('total', expression.add(expression.variable('total'), expression.groupCount(group(), filter()))));
    if (body.length === 2) body.push(step.require(expression.lessThanOrEqual(expression.variable('before'), expression.variable('total'))));
    steps.push(step.let('total', expression.u64(0)));
    steps.push(step.repeat(expression.input('n'), body, { max: 4, carry: ['total'] }));
  }
  // Every other filler stays live to the end: more than 64 values in all, fewer than 64 at once.
  const live = Array.from({ length: Math.ceil(fillers / 2) }, (_, index) => data.encode('u64', expression.variable(`f${index * 2}`)));
  steps.push(step.setReturnData([data.encode('u64', expression.variable('count')), ...live]));
  for (const name of Object.keys(inputs).filter((current) => /^v\d+$/.test(current))) {
    const bound = steps.some((current) => current.kind === 'let' && current.name === `${name}Bound`);
    steps.push(step.emit([data.literal(new TextEncoder().encode('grp!')), data.encode(inputs[name]!.type, bound ? expression.variable(`${name}Bound`) : expression.input(name))]));
  }
  return { inputs, accounts: { a: {}, b: {} }, accountGroups: ['g', 'h'], steps };
}

describe.runIf(OUT)('compiler differential', () => {
  test('writes the differential corpus', { timeout: 3_600_000 }, () => {
    const { generateCase, mutateDocument, materializeAliases, Rng } = fuzz!;
    const entries: DifferentialEntry[] = [
      entry('carriedAlias', fuzz!.carriedAliasDocument()),
      entry('perPassCap', fuzz!.perPassCapDocument()),
      entry('manyInvokes', fuzz!.manyInvokesDocument()),
      entry('largeOffset', fuzz!.largeOffsetDocument()),
      entry('protoName', fuzz!.protoNameDocument()),
      entry('deepNesting', fuzz!.deepNestingDocument(200)),
    ];
    for (let seed = SEED_START; seed < SEED_START + SEEDS; seed += 1) {
      const { template } = generateCase(seed);
      const natural = entry(`seed ${seed}`, template);
      entries.push(natural);
      if (natural.hazard) entries.push(entry(`seed ${seed} materialized`, materializeAliases(template)));
      const rng = new Rng(seed * 7_919 + 1);
      for (let index = 0; index < MUTANTS; index += 1) {
        const mutant = mutateDocument(template, rng);
        entries.push(entry(`seed ${seed} mutant ${index} (${mutant.mutations.join('; ')})`, mutant.template));
      }
    }
    const groupRng = new Rng(0x6703);
    for (let index = 0; index < GROUP_DOCUMENTS; index += 1) entries.push(entry(`group ${index}`, groupDocument(groupRng)));
    writeFileSync(OUT!, `[\n${entries.map((current) => documentJson(current)).join(',\n')}\n]\n`);
    const compiled = entries.filter((current) => current.bytes !== undefined).length;
    console.log(
      `${entries.length} documents: ${compiled} compile, ${entries.filter((current) => current.groups).length} use groups, ` +
        `${entries.filter((current) => current.hazard).length} carry an alias`,
    );
    expect(compiled).toBeGreaterThan(0);
  });
});
