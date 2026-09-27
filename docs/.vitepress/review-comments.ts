import { randomUUID } from 'node:crypto';
import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Plugin } from 'vite';

/**
 * Review comments for the docs site: highlight text on any page of the dev server, write a comment,
 * and it is appended to one JSON object per line in `.local/review-comments.jsonl`, which Git
 * ignores. Only `vitepress dev` serves the endpoints; production builds never include them.
 *
 * An edit is the same kind of record with `kind: 'edit'`: text typed straight into the page, saved
 * with the element's text before and after. Once the source says the same, whoever changed it sets
 * `appliedAt` on the record.
 *
 * Variations answer a comment with three candidate changes, previewed in place. Whoever answers a
 * comment writes `.local/review-variations.json` as `{ "sets": [VariationSet, ...] }`. A picker
 * hovers beside each set's `anchor` element and switches the page between the current version and
 * each variant. Keeping one records `chosen` in the file, and the change is then made in the
 * source and the set removed. The browser only ever writes `chosen`.
 */
export const REVIEW_FILE = fileURLToPath(new URL('../../.local/review-comments.jsonl', import.meta.url));
export const REVIEW_ENDPOINT = '/__review/comments';
export const VARIATIONS_FILE = fileURLToPath(new URL('../../.local/review-variations.json', import.meta.url));
export const VARIATIONS_ENDPOINT = '/__review/variations';
/** The event the dev server sends when either review file changes. */
export const REVIEW_CHANGED_EVENT = 'review:changed';

export interface ReviewComment {
  id: string;
  createdAt: string;
  /** A comment asks for a change; an edit is the change itself, typed into the page. */
  kind: 'comment' | 'edit';
  /** The page's route, such as `/ballista/guide/getting-started`. */
  page: string;
  /** The markdown file the page is built from, relative to docs/. */
  file: string;
  title: string;
  /** The heading the selected text sits under. */
  heading: string;
  /** Exactly the text that was highlighted. */
  selection: string;
  /** A CSS selector matching the one element the selection sits in. */
  selector: string;
  /** Where the selection starts and ends in that element's textContent. */
  offsets: [number, number];
  /** Text just before and after the selection, to find it again after edits. */
  before: string;
  after: string;
  comment: string;
  /** For an edit: the element's text before and after, line breaks kept. */
  original?: string;
  edited?: string;
  /** For an edit: the element's markup before, so inline code, links and emphasis can be kept. */
  originalHtml?: string;
  /** Set by whoever makes the change in the source. */
  appliedAt?: string;
}

/**
 * One previewable change. While a variant is showing, the picker injects its `css`, replaces the
 * text or children of each matched element, and sets `data-rv-<set id>` on `<html>` to the
 * variant's number (0 for the current page), so source code can also branch on it.
 */
export interface Variant {
  label: string;
  note?: string;
  css?: string;
  text?: { selector: string; text: string }[];
  html?: { selector: string; html: string }[];
}

export interface VariationSet {
  /** Lowercase letters, digits and dashes; also names the `data-rv-*` attribute. */
  id: string;
  /** The review comment this set answers. */
  commentId?: string;
  /** The route it applies to, such as `/ballista/`. */
  page: string;
  /** A CSS selector for the element the picker hovers beside. */
  anchor: string;
  /** What the comment asked for, shown in the picker. */
  request: string;
  variants: Variant[];
  /** Written by the picker: the kept variant's number, or 0 to keep the page as it is. */
  chosen?: number | null;
  chosenAt?: string;
}

const LIMITS = {
  page: 300,
  file: 300,
  selector: 1_000,
  title: 300,
  heading: 300,
  selection: 4_000,
  context: 300,
  comment: 4_000,
  edit: 8_000,
  html: 16_000,
};
const MAX_BODY = 100_000;

/** Two non-negative integers, start before end, or [0, 0] when the client sent anything else. */
function offsets(value: unknown): [number, number] {
  if (!Array.isArray(value) || value.length !== 2) return [0, 0];
  const [start, end] = value;
  return Number.isSafeInteger(start) && Number.isSafeInteger(end) && start >= 0 && end >= start ? [start, end] : [0, 0];
}

function readComments(): ReviewComment[] {
  if (!existsSync(REVIEW_FILE)) return [];
  return readFileSync(REVIEW_FILE, 'utf8')
    .split('\n')
    .filter((line) => line.trim() !== '')
    .flatMap((line) => {
      try {
        return [JSON.parse(line) as ReviewComment];
      } catch {
        return [];
      }
    });
}

function readVariations(): VariationSet[] {
  if (!existsSync(VARIATIONS_FILE)) return [];
  try {
    const parsed = JSON.parse(readFileSync(VARIATIONS_FILE, 'utf8')) as { sets?: VariationSet[] };
    return Array.isArray(parsed.sets) ? parsed.sets : [];
  } catch {
    // A half-written file reads as empty; the next change event brings the full one.
    return [];
  }
}

export function reviewComments(): Plugin {
  return {
    name: 'ballista-review-comments',
    apply: 'serve',
    configureServer(server) {
      // Tell open pages when either file changes, so new variations and choices show without a reload.
      server.watcher.add([REVIEW_FILE, VARIATIONS_FILE]);
      server.watcher.on('all', (_event, path) => {
        if (path === REVIEW_FILE || path === VARIATIONS_FILE) {
          server.ws.send({ type: 'custom', event: REVIEW_CHANGED_EVENT, data: { file: path === REVIEW_FILE ? 'comments' : 'variations' } });
        }
      });

      server.middlewares.use(VARIATIONS_ENDPOINT, (req, res) => {
        const send = (status: number, body: unknown) => {
          res.statusCode = status;
          res.setHeader('content-type', 'application/json');
          res.end(JSON.stringify(body));
        };
        const url = new URL(req.url ?? '/', 'http://localhost');

        if (req.method === 'GET') {
          const page = url.searchParams.get('page');
          send(200, readVariations().filter((set) => !page || set.page === page));
          return;
        }
        if (req.method !== 'POST' || url.pathname !== '/choose') {
          send(405, { error: 'GET the sets, or POST /choose with { id, variant }.' });
          return;
        }

        let body = '';
        req.setEncoding('utf8');
        req.on('data', (chunk: string) => {
          body += chunk;
          if (body.length > 1_000) req.destroy();
        });
        req.on('end', () => {
          let input: { id?: unknown; variant?: unknown };
          try {
            input = JSON.parse(body);
          } catch {
            send(400, { error: 'The body must be JSON.' });
            return;
          }
          const sets = readVariations();
          const set = sets.find((candidate) => candidate.id === input.id);
          const variant = input.variant;
          if (!set) {
            send(404, { error: 'No variation set has that id.' });
            return;
          }
          if (!Number.isSafeInteger(variant) || (variant as number) < 0 || (variant as number) > set.variants.length) {
            send(400, { error: `variant must be 0 to ${set.variants.length}.` });
            return;
          }
          set.chosen = variant as number;
          set.chosenAt = new Date().toISOString();
          writeFileSync(VARIATIONS_FILE, `${JSON.stringify({ sets }, null, 2)}\n`);
          send(200, set);
        });
      });

      server.middlewares.use(REVIEW_ENDPOINT, (req, res) => {
        const send = (status: number, body: unknown) => {
          res.statusCode = status;
          res.setHeader('content-type', 'application/json');
          res.end(JSON.stringify(body));
        };
        const query = new URL(req.url ?? '/', 'http://localhost').searchParams;

        if (req.method === 'GET') {
          const page = query.get('page');
          send(200, readComments().filter((comment) => !page || comment.page === page));
          return;
        }

        if (req.method === 'DELETE') {
          const id = query.get('id');
          const comments = readComments();
          const kept = comments.filter((comment) => comment.id !== id);
          if (kept.length === comments.length) {
            send(404, { error: 'No comment has that id.' });
            return;
          }
          writeFileSync(REVIEW_FILE, kept.map((comment) => JSON.stringify(comment) + '\n').join(''));
          send(200, { deleted: id });
          return;
        }

        if (req.method !== 'POST') {
          send(405, { error: 'Use GET, POST, or DELETE.' });
          return;
        }

        let body = '';
        req.setEncoding('utf8');
        req.on('data', (chunk: string) => {
          body += chunk;
          if (body.length > MAX_BODY) req.destroy();
        });
        req.on('end', () => {
          let input: Record<string, unknown>;
          try {
            input = JSON.parse(body);
          } catch {
            send(400, { error: 'The body must be JSON.' });
            return;
          }
          const text = (key: keyof typeof LIMITS, from: string = key) =>
            typeof input[from] === 'string' ? (input[from] as string).slice(0, LIMITS[key]) : '';
          const kind = input.kind === 'edit' ? 'edit' : 'comment';
          const record: ReviewComment = {
            id: randomUUID(),
            createdAt: new Date().toISOString(),
            kind,
            page: text('page'),
            file: text('file'),
            title: text('title'),
            heading: text('heading'),
            selection: text('selection').trim(),
            selector: text('selector'),
            offsets: offsets(input.offsets),
            before: text('context', 'before'),
            after: text('context', 'after'),
            comment: text('comment').trim(),
          };
          if (kind === 'edit') {
            record.original = text('edit', 'original');
            record.edited = text('edit', 'edited');
            record.originalHtml = text('html', 'originalHtml');
            if (!record.original || record.original === record.edited) {
              send(400, { error: 'Change some text before saving the edit.' });
              return;
            }
          } else if (!record.selection || !record.comment) {
            send(400, { error: 'Highlight some text and write a comment.' });
            return;
          }
          mkdirSync(dirname(REVIEW_FILE), { recursive: true });
          appendFileSync(REVIEW_FILE, JSON.stringify(record) + '\n');
          send(201, record);
        });
      });
    },
  };
}
