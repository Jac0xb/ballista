// Claude's side of the docs review tools, one subcommand per step: `node scripts/review/claude.mjs …`.
//
//   answer <set-id> <variants.json> [anchor]   stream variants into a set, one at a time
//   snapshot <set-id> <file>...                 copy files before changing them
//   applied <set-id>                            mark the kept variant applied, with line counts
//   edit-applied <comment-id>                   mark a typed edit as in the source
//   close <set-id>                              remove a set (nothing to change)
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('../../', import.meta.url));
const VARIATIONS = join(ROOT, '.local/review-variations.json');
const COMMENTS = join(ROOT, '.local/review-comments.jsonl');
const SNAPSHOTS = join(ROOT, '.local/review-state/snapshots');
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const readSets = () => (existsSync(VARIATIONS) ? JSON.parse(readFileSync(VARIATIONS, 'utf8')).sets ?? [] : []);
const writeSets = (sets) => writeFileSync(VARIATIONS, JSON.stringify({ sets }, null, 2) + '\n');
const update = (id, change) => {
  const sets = readSets();
  const set = sets.find((candidate) => candidate.id === id);
  if (!set) throw new Error(`No set ${id}`);
  change(set);
  writeSets(sets);
  return set;
};

const [command, id, ...rest] = process.argv.slice(2);

if (command === 'answer') {
  const variants = JSON.parse(readFileSync(rest[0], 'utf8'));
  const anchor = rest[1];
  update(id, (set) => {
    if (anchor) set.anchor = anchor;
    set.status = 'drafting';
  });
  // One at a time, so the page shows each option arriving.
  for (let count = 1; count <= variants.length; count++) {
    update(id, (set) => {
      set.variants = variants.slice(0, count);
      set.status = count === variants.length ? 'ready' : 'drafting';
    });
    if (count < variants.length) await sleep(450);
  }
  console.log(`answered ${id} with ${variants.length} variants`);
} else if (command === 'snapshot') {
  const dir = join(SNAPSHOTS, id);
  mkdirSync(dir, { recursive: true });
  const files = rest.map((file) => resolve(ROOT, file));
  files.forEach((file, index) => copyFileSync(file, join(dir, `${index}-${basename(file)}`)));
  writeFileSync(join(dir, 'files.json'), JSON.stringify(files));
  console.log(`snapshot of ${files.length} file(s) for ${id}`);
} else if (command === 'applied') {
  const dir = join(SNAPSHOTS, id);
  const notes = [];
  if (existsSync(join(dir, 'files.json'))) {
    const files = JSON.parse(readFileSync(join(dir, 'files.json'), 'utf8'));
    files.forEach((file, index) => {
      let out = '';
      try {
        out = execFileSync('git', ['diff', '--no-index', '--numstat', join(dir, `${index}-${basename(file)}`), file], { encoding: 'utf8' });
      } catch (error) {
        out = error.stdout ?? ''; // git diff exits 1 when the files differ
      }
      const [added, removed] = out.trim().split(/\s+/);
      if (added !== undefined && out.trim()) notes.push(`${basename(file)} · +${added} −${removed}`);
    });
  }
  const set = update(id, (set) => {
    set.appliedAt = new Date().toISOString();
    if (notes.length) set.appliedNote = notes.join(', ');
  });
  console.log(`applied ${id}${set.appliedNote ? ` (${set.appliedNote})` : ''}`);
} else if (command === 'edit-applied') {
  const records = readFileSync(COMMENTS, 'utf8').split('\n').filter(Boolean).map((line) => JSON.parse(line));
  let hit = 0;
  for (const record of records) {
    if (record.id === id || record.id.startsWith(id)) {
      record.appliedAt = new Date().toISOString();
      hit++;
    }
  }
  writeFileSync(COMMENTS, records.map((record) => JSON.stringify(record)).join('\n') + '\n');
  console.log(`marked ${hit} edit(s) applied`);
} else if (command === 'close') {
  writeSets(readSets().filter((set) => set.id !== id));
  console.log(`closed ${id}`);
} else {
  console.log('usage: answer | snapshot | applied | edit-applied | close');
  process.exit(1);
}
