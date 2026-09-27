// Claude's watch on the docs review tools (see docs/.vitepress/review-comments.ts). Run it as a
// long-lived monitor: `node scripts/review/watch.mjs`. It polls the review files and prints one line
// per new comment, edit, deletion or kept variant.
// Ids already reported are kept in a state file, so re-arming the watch never repeats them.
// A new comment also gets a placeholder set straight away, so the page shows "Claude is reading"
// within a second while the real options are written.
import { appendFileSync, existsSync, readFileSync, writeFileSync } from 'node:fs';

const LOCAL = new URL('../../.local/', import.meta.url);
const FILE = new URL('review-comments.jsonl', LOCAL);
const STATE = new URL('review-state/seen', LOCAL);
const seen = new Set(existsSync(STATE) ? readFileSync(STATE, 'utf8').split('\n').filter(Boolean) : []);
// With --once the watch exits after the first line it prints, for runners that notify on exit.
const once = process.argv.includes('--once');
const report = (line) => {
  console.log(line);
  if (once) process.exit(0);
};
const flat = (text, max) => {
  const one = String(text ?? '').replace(/\s+/g, ' ').trim();
  return one.length > max ? `${one.slice(0, max)}…` : one;
};

function tick() {
  let records = [];
  try {
    if (existsSync(FILE)) {
      records = readFileSync(FILE, 'utf8').split('\n').filter((line) => line.trim()).flatMap((line) => {
        try { return [JSON.parse(line)]; } catch { return []; }
      });
    }
  } catch (error) {
    console.log(`WATCH-ERROR ${error.message}`);
    return;
  }
  const present = new Set(records.map((record) => record.id));
  for (const record of records) {
    if (seen.has(record.id)) continue;
    seen.add(record.id);
    appendFileSync(STATE, `${record.id}\n`);
    if (record.kind === 'edit') {
      // Show only what changed, with a little context on each side.
      const was = String(record.original ?? ''), now = String(record.edited ?? '');
      let start = 0;
      while (start < was.length && start < now.length && was[start] === now[start]) start++;
      let end = 0;
      while (end < was.length - start && end < now.length - start && was[was.length - 1 - end] === now[now.length - 1 - end]) end++;
      const before = was.slice(Math.max(0, start - 30), start), after = was.slice(was.length - end, was.length - end + 30);
      report(`EDIT ${record.id} | ${record.file} | § ${flat(record.heading, 80)} | …${flat(before, 40)}[${flat(was.slice(start, was.length - end), 400)} → ${flat(now.slice(start, now.length - end), 400)}]${flat(after, 40)}…`);
    } else {
      const setId = placeholder(record);
      report(`NEW ${record.id} | SET ${setId} | ${record.file} | § ${flat(record.heading, 80)} | anchor ${record.selector} | "${flat(record.selection, 200)}" | COMMENT: ${flat(record.comment, 1000)}`);
    }
  }
  for (const id of seen) {
    if (!present.has(id) && existsSync(FILE)) {
      // Only report deletions of ids this watch saw in the file; state entries stay so they never repeat.
      if (!tick.reportedDeleted.has(id) && tick.everPresent.has(id)) {
        tick.reportedDeleted.add(id);
        report(`DELETED ${id}`);
      }
    }
  }
  for (const id of present) tick.everPresent.add(id);
}
tick.reportedDeleted = new Set();
tick.everPresent = new Set();

// Variations: report each choice once, keyed by set id and the time it was made.
const VARIATIONS = new URL('review-variations.json', LOCAL);
const readSets = () => {
  try {
    return existsSync(VARIATIONS) ? JSON.parse(readFileSync(VARIATIONS, 'utf8')).sets ?? [] : [];
  } catch {
    return null; // mid-write
  }
};
const writeSets = (sets) => writeFileSync(VARIATIONS, JSON.stringify({ sets }, null, 2) + '\n');

/** An empty, drafting set for a new comment: the picker appears at once and fills in as options are written. */
function placeholder(record) {
  const id = `c-${record.id.slice(0, 8)}`;
  const sets = readSets();
  if (!sets || sets.some((set) => set.commentId === record.id)) return id;
  sets.push({
    id,
    commentId: record.id,
    page: record.page,
    anchor: record.selector || 'main',
    request: record.comment,
    variants: [],
    status: 'drafting',
    createdAt: new Date().toISOString(),
  });
  writeSets(sets);
  return id;
}

/** Applied sets have done their job once the page has shown "Applied"; clear them after ten minutes. */
function prune() {
  const sets = readSets();
  if (!sets) return;
  const kept = sets.filter((set) => !set.appliedAt || Date.now() - Date.parse(set.appliedAt) < 10 * 60_000);
  if (kept.length !== sets.length) writeSets(kept);
}
function tickVariations() {
  if (!existsSync(VARIATIONS)) return;
  let sets = [];
  try {
    sets = JSON.parse(readFileSync(VARIATIONS, 'utf8')).sets ?? [];
  } catch {
    return; // mid-write
  }
  for (const set of sets) {
    if (typeof set.chosen !== 'number') continue;
    const key = `chosen:${set.id}:${set.chosenAt}`;
    if (seen.has(key)) continue;
    seen.add(key);
    appendFileSync(STATE, `${key}\n`);
    const label = set.chosen === 0 ? 'keep the page as it is' : `${'ABCDEFGH'[set.chosen - 1]} · ${set.variants[set.chosen - 1]?.label ?? '?'}`;
    report(`CHOSEN ${set.id} | ${set.page} | ${label} | request: ${flat(set.request, 200)}`);
  }
}

tick();
tickVariations();
setInterval(() => {
  tick();
  tickVariations();
}, 1000);
setInterval(prune, 60_000);
