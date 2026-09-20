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
const sol = (lamports) => (lamports / 1e9).toFixed(5).replace(/0+$/, '').replace(/\.$/, '');
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
  const upload = item.upload;
  const uploadPlural = upload.transactionCount === 1 ? 'transaction' : 'transactions';
  const rows = [
    `| Cost | Ballista | ${baselineLabel[item.baseline.verdict]} | Difference |`,
    '| --- | ---: | ---: | ---: |',
    `| Compute units, every run | ${group(ballistaUnits)} | ${group(baselineUnits)} | ${signed(ballistaUnits - baselineUnits)} |`,
    `| Transaction bytes, every run | ${group(item.ballistaTransactionBytes)} | ${group(item.baseline.transactionBytes)} | ${signed(item.ballistaTransactionBytes - item.baseline.transactionBytes)} |`,
    `| Compute units, upload once | ${group(measured.uploadComputeUnits)} | none | — |`,
    `| Transaction bytes, upload once | ${group(upload.transactionBytes)} in ${upload.transactionCount} ${uploadPlural} | none | — |`,
    `| Rent locked in the template account | ${sol(upload.rentLamports)} SOL for ${group(upload.accountBytes)} bytes | none | — |`,
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
  '| Pattern | CU per run | Plain CU | Bytes per run | Plain bytes | Template rent | Without a program? |',
  '| --- | ---: | ---: | ---: | ---: | ---: | --- |',
  ...Object.entries(benchmarks).map(([name, item]) => {
    const measured = results[name];
    const verdict = {
      equivalent: 'Yes, same guarantees',
      weaker: 'Yes, weaker guarantees',
      impossible: 'No, needs a program',
    }[item.baseline.verdict];
    const title = titles.get(item.anchor) ?? name;
    return `| [${title}](/examples/${item.page}#${item.anchor}) | ${group(measured.ballistaComputeUnits)} | ${group(measured.baselineComputeUnits)} | ${group(item.ballistaTransactionBytes)} | ${group(item.baseline.transactionBytes)} | ${sol(item.upload.rentLamports)} SOL | ${verdict} |`;
  }),
].join('\n');

/** A line chart of bytes saved against the row count, as inline SVG so the page needs no plugin. */
function savingsChart(sweeps) {
  const series = Object.values(sweeps);
  const maxRows = Math.max(...series.flatMap((item) => item.points.map((point) => point.rows)));
  const values = series.flatMap((item) => item.points.map((point) => point.plain - point.ballista));
  const maxSaved = Math.max(...values);
  const minSaved = Math.min(...values);
  const width = 720;
  const height = 320;
  const left = 56;
  const right = 16;
  const top = 16;
  const bottom = 44;
  const plotWidth = width - left - right;
  const plotHeight = height - top - bottom;
  const x = (rows) => left + ((rows - 1) / (maxRows - 1)) * plotWidth;
  const y = (saved) => top + ((maxSaved - saved) / (maxSaved - minSaved)) * plotHeight;
  const colors = ['#2f6f4f', '#8a5a2b'];

  const ticks = [];
  for (let saved = Math.ceil(minSaved / 100) * 100; saved <= maxSaved; saved += 100) {
    ticks.push(saved);
  }
  const gridlines = ticks
    .map(
      (saved) =>
        `<line x1="${left}" y1="${y(saved).toFixed(1)}" x2="${width - right}" y2="${y(saved).toFixed(1)}" stroke="currentColor" stroke-opacity="${saved === 0 ? 0.45 : 0.12}" />` +
        `<text x="${left - 8}" y="${(y(saved) + 4).toFixed(1)}" text-anchor="end" font-size="12" fill="currentColor" fill-opacity="0.7">${saved}</text>`,
    )
    .join('\n    ');

  const rowTicks = [1, 5, 10, 15, 20, 25, 30].filter((rows) => rows <= maxRows);
  const rowLabels = rowTicks
    .map(
      (rows) =>
        `<text x="${x(rows).toFixed(1)}" y="${height - bottom + 20}" text-anchor="middle" font-size="12" fill="currentColor" fill-opacity="0.7">${rows}</text>`,
    )
    .join('\n    ');

  const lines = series
    .map((item, index) => {
      const path = item.points
        .map((point, position) => `${position === 0 ? 'M' : 'L'} ${x(point.rows).toFixed(1)} ${y(point.plain - point.ballista).toFixed(1)}`)
        .join(' ');
      return `<path d="${path}" fill="none" stroke="${colors[index % colors.length]}" stroke-width="2.5" />`;
    })
    .join('\n    ');

  const legend = series
    .map((item, index) => {
      const offsetY = top + 12 + index * 20;
      return (
        `<line x1="${left + 12}" y1="${offsetY}" x2="${left + 36}" y2="${offsetY}" stroke="${colors[index % colors.length]}" stroke-width="2.5" />` +
        `<text x="${left + 44}" y="${offsetY + 4}" font-size="13" fill="currentColor">${item.label}</text>`
      );
    })
    .join('\n    ');

  return [
    `<svg viewBox="0 0 ${width} ${height}" role="img" aria-label="Transaction bytes saved against the number of batch rows" style="width:100%;height:auto;max-width:720px">`,
    `    ${gridlines}`,
    `    <line x1="${left}" y1="${top}" x2="${left}" y2="${height - bottom}" stroke="currentColor" stroke-opacity="0.35" />`,
    `    ${lines}`,
    `    ${rowLabels}`,
    `    <text x="${left + plotWidth / 2}" y="${height - 6}" text-anchor="middle" font-size="13" fill="currentColor" fill-opacity="0.8">Rows in the batch</text>`,
    `    <text x="14" y="${top + plotHeight / 2}" text-anchor="middle" font-size="13" fill="currentColor" fill-opacity="0.8" transform="rotate(-90 14 ${top + plotHeight / 2})">Transaction bytes saved</text>`,
    `    ${legend}`,
    '</svg>',
  ].join('\n');
}

const sweepPath = fileURLToPath(new URL('fixtures/benchmark-sweeps.json', root));
const sweeps = JSON.parse(readFileSync(sweepPath, 'utf8'));
/** Per-row slope and break-even, derived so the prose cannot drift from the chart. */
function savingsCaption(sweeps) {
  return Object.values(sweeps)
    .map((item) => {
      const first = item.points[0];
      const last = item.points[item.points.length - 1];
      const span = last.rows - first.rows;
      const ballistaPerRow = (last.ballista - first.ballista) / span;
      const plainPerRow = (last.plain - first.plain) / span;
      const crossing = item.points.find((point) => point.plain - point.ballista >= 0);
      const evens = crossing ? `${crossing.rows} rows` : `more than ${last.rows} rows`;
      return `a ${item.captionLabel} costs ${ballistaPerRow.toFixed(0)} bytes per row through Ballista against ${plainPerRow.toFixed(0)} plain, breaking even at ${evens}`;
    })
    .join('; ');
}

const chartBlock = `<!-- benchmark:chart -->\n\n${savingsChart(sweeps)}\n\nMeasured: ${savingsCaption(sweeps)}.\n\n<!-- /benchmark -->`;
for (const page of ['docs/benchmarks.md']) {
  const chartPagePath = fileURLToPath(new URL(page, root));
  const chartText = readFileSync(chartPagePath, 'utf8');
  const marker = /<!-- benchmark:chart -->[\s\S]*?<!-- \/benchmark -->/;
  if (!marker.test(chartText)) {
    console.log(`${page}: no chart marker, skipped`);
    continue;
  }
  writeFileSync(chartPagePath, chartText.replace(marker, chartBlock));
  console.log(`${page}: savings chart`);
}

const summaryBlock = `<!-- benchmark:summary -->\n\n${summary}\n\n<!-- /benchmark -->`;
for (const page of ['docs/benchmarks.md', 'docs/examples/index.md']) {
  const summaryPath = fileURLToPath(new URL(page, root));
  const summaryText = readFileSync(summaryPath, 'utf8');
  const marker = /<!-- benchmark:summary -->[\s\S]*?<!-- \/benchmark -->/;
  if (!marker.test(summaryText)) {
    console.log(`${page}: no summary marker, skipped`);
    continue;
  }
  writeFileSync(summaryPath, summaryText.replace(marker, summaryBlock));
  console.log(`${page}: summary table`);
}

// The compute profile page: one table, grouped, most expensive first within each group.
const profile = JSON.parse(readFileSync(fileURLToPath(new URL('fixtures/cu-profile.json', root)), 'utf8'));
const profileOrder = ['Run', 'Instructions', 'Invocations', 'Batches', 'Accounts', 'Inputs'];
const profileRows = ['| Measurement | Compute units | What it covers |', '| --- | ---: | --- |'];
for (const groupName of profileOrder) {
  const entries = Object.entries(profile)
    .filter(([, entry]) => entry.group === groupName)
    .sort((left, right) => right[1].computeUnits - left[1].computeUnits);
  if (entries.length === 0) continue;
  profileRows.push(`| **${groupName}** | | |`);
  for (const [name, entry] of entries) {
    const units =
      entry.computeUnits === 0
        ? 'under 0.01'
        : entry.computeUnits < 10
          ? entry.computeUnits.toFixed(2)
          : group(entry.computeUnits);
    profileRows.push(`| ${name} | ${units} | ${entry.note} |`);
  }
}
const profilePath = fileURLToPath(new URL('docs/cu-profile.md', root));
const profileText = readFileSync(profilePath, 'utf8');
const profileMarker = /<!-- profile:table -->[\s\S]*?<!-- \/profile -->/;
if (profileMarker.test(profileText)) {
  writeFileSync(
    profilePath,
    profileText.replace(profileMarker, `<!-- profile:table -->\n\n${profileRows.join('\n')}\n\n<!-- /profile -->`),
  );
  console.log('cu-profile.md: profile table');
} else {
  console.log('cu-profile.md: no profile marker, skipped');
}

function slug(heading) {
  return heading
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9 -]/g, '')
    .replace(/\s+/g, '-');
}
