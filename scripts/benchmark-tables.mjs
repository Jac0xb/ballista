/**
 * Writes the measured cost table under each example in `docs/examples/`.
 *
 * Inputs are `fixtures/benchmarks.json` (written by `pnpm benchmarks`) and
 * `fixtures/benchmark-results.json` (written by the Mollusk benchmark in `tests/ballista`).
 * Each table is delimited by HTML comments, so re-running replaces it in place.
 *
 * Usage: node scripts/benchmark-tables.mjs
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = new URL('../', import.meta.url);
const benchmarks = JSON.parse(readFileSync(fileURLToPath(new URL('fixtures/benchmarks.json', root)), 'utf8'));
const results = JSON.parse(readFileSync(fileURLToPath(new URL('fixtures/benchmark-results.json', root)), 'utf8'));

const group = (value) => value.toLocaleString('en-US');
const signed = (value) => (value >= 0 ? `+${group(value)}` : `−${group(Math.abs(value))}`);

const baselineLabel = {
  equivalent: 'Plain instructions',
  weaker: 'Plain instructions, weaker',
  impossible: 'Plain instructions, not equivalent',
};

const verdictSentence = {
  equivalent: (note) => `${note} Ballista buys one instruction and a stored, verified shape, not a capability you lack.`,
  weaker: (note) => `${note} Enforcing that on chain any other way means deploying your own program.`,
  impossible: (note) => note,
};

function table(name) {
  const item = benchmarks[name];
  const measured = results[name];
  if (!item || !measured) throw new Error(`No benchmark for ${name}`);
  const ballistaUnits = measured.ballistaComputeUnits;
  const baselineUnits = measured.baselineComputeUnits;
  const rows = [
    '| Approach | Compute units | Transaction bytes | Stored on chain |',
    '| --- | ---: | ---: | --- |',
    `| Ballista | ${group(ballistaUnits)} | ${group(item.ballistaTransactionBytes)} | ${group(item.payloadBytes)}-byte template, once |`,
    `| ${baselineLabel[item.baseline.verdict]} | ${group(baselineUnits)} | ${group(item.baseline.transactionBytes)} | none |`,
    `| Difference | ${signed(ballistaUnits - baselineUnits)} | ${signed(item.ballistaTransactionBytes - item.baseline.transactionBytes)} | — |`,
  ];
  const instructionCount = item.baseline.instructionCount;
  const plural = instructionCount === 1 ? 'instruction' : 'instructions';
  const scope = item.rowCount > 0 ? ` covering ${item.rowCount} rows` : '';
  const standIn = item.standIn
    ? ' The protocol call is stood in by a System transfer, so neither row includes the protocol\'s own work.'
    : '';
  return [
    `<!-- benchmark:${name} -->`,
    '',
    ...rows,
    '',
    `One Ballista instruction${scope} against ${instructionCount} plain ${plural}, measured with Mollusk.${standIn} ${verdictSentence[item.baseline.verdict](item.baseline.note)}`,
    '',
    '<!-- /benchmark -->',
  ].join('\n');
}

const pages = new Map();
for (const [name, item] of Object.entries(benchmarks)) {
  if (!pages.has(item.page)) pages.set(item.page, []);
  pages.get(item.page).push({ name, anchor: item.anchor });
}

/** Heading text by anchor, so the summary reads the way the cookbook does. */
const titles = new Map();
for (const page of pages.keys()) {
  const text = readFileSync(fileURLToPath(new URL(`docs/examples/${page}.md`, root)), 'utf8');
  for (const line of text.split('\n')) {
    if (line.startsWith('## ')) titles.set(slug(line.slice(3)), line.slice(3).trim());
  }
}

for (const [page, items] of pages) {
  const path = fileURLToPath(new URL(`docs/examples/${page}.md`, root));
  let text = readFileSync(path, 'utf8');
  for (const { name, anchor } of items) {
    const block = table(name);
    const existing = new RegExp(`<!-- benchmark:${name} -->[\\s\\S]*?<!-- /benchmark -->`);
    if (existing.test(text)) {
      text = text.replace(existing, block);
      continue;
    }
    // Insert before the next top-level heading after this example's own heading.
    const lines = text.split('\n');
    const headingIndex = lines.findIndex(
      (line) => line.startsWith('## ') && slug(line.slice(3)) === anchor,
    );
    if (headingIndex === -1) throw new Error(`No heading for ${anchor} in ${page}.md`);
    let end = lines.length;
    for (let index = headingIndex + 1; index < lines.length; index += 1) {
      if (lines[index].startsWith('## ')) {
        end = index;
        break;
      }
    }
    while (end > headingIndex && lines[end - 1].trim() === '') end -= 1;
    lines.splice(end, 0, '', block);
    text = lines.join('\n');
  }
  writeFileSync(path, text.endsWith('\n') ? text : `${text}\n`);
  console.log(`${page}.md: ${items.length} tables`);
}

const summary = [
  '| Pattern | Ballista CU | Plain CU | Ballista bytes | Plain bytes | Without a program? |',
  '| --- | ---: | ---: | ---: | ---: | --- |',
  ...Object.entries(benchmarks).map(([name, item]) => {
    const measured = results[name];
    const verdict = {
      equivalent: 'Yes, same guarantees',
      weaker: 'Yes, weaker guarantees',
      impossible: 'No, needs a program',
    }[item.baseline.verdict];
    const title = titles.get(item.anchor) ?? name;
    return `| [${title}](/examples/${item.page}#${item.anchor}) | ${group(measured.ballistaComputeUnits)} | ${group(measured.baselineComputeUnits)} | ${group(item.ballistaTransactionBytes)} | ${group(item.baseline.transactionBytes)} | ${verdict} |`;
  }),
].join('\n');

const summaryPath = fileURLToPath(new URL('docs/benchmarks.md', root));
let summaryText = readFileSync(summaryPath, 'utf8');
const summaryBlock = `<!-- benchmark:summary -->\n\n${summary}\n\n<!-- /benchmark -->`;
if (/<!-- benchmark:summary -->[\s\S]*?<!-- \/benchmark -->/.test(summaryText)) {
  summaryText = summaryText.replace(/<!-- benchmark:summary -->[\s\S]*?<!-- \/benchmark -->/, summaryBlock);
  writeFileSync(summaryPath, summaryText);
  console.log('benchmarks.md: summary table');
} else {
  console.log('benchmarks.md: no summary marker, skipped');
}

function slug(heading) {
  return heading
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9 -]/g, '')
    .replace(/\s+/g, '-');
}
