<script setup lang="ts">
// Replaces VitePress's local search box (see the alias in ../config.mts) and reads the same index.
// Over the default it adds: synonyms (CU, PDA, CPI, …), a strict-then-loose query, results grouped
// by page with a snippet of where the words matched, section filters, recent searches, typo
// tolerance with a "did you mean", and the combobox pattern (the input keeps focus and points at the
// selected result with aria-activedescendant).
import localSearchIndex from '@localSearchIndex';
import MiniSearch, { type SearchResult } from 'minisearch';
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue';
import { useData, useRouter, withBase } from 'vitepress';

const emit = defineEmits<{ (event: 'close'): void }>();

interface Stored {
  title: string;
  titles: string[];
  text?: string;
}
type Hit = SearchResult & Stored;
interface Group {
  page: string;
  title: string;
  hits: Hit[];
}

const router = useRouter();
const { localeIndex } = useData();
const input = ref<HTMLInputElement>();
const list = ref<HTMLElement>();
const query = ref('');
const filter = ref<'all' | 'guide' | 'examples' | 'reference'>('all');
const selected = ref(0);
const index = shallowRef<MiniSearch<Stored> | null>(null);
const took = ref(0);

// ---- The index

const indexData = shallowRef(localSearchIndex);
if (import.meta.hot) {
  import.meta.hot.accept('/@localSearchIndex', (module) => {
    if (module) indexData.value = module.default;
  });
}
async function loadIndex() {
  const loader = (indexData.value as Record<string, () => Promise<{ default: string }>>)[localeIndex.value];
  if (!loader) return;
  const json = (await loader()).default;
  index.value = MiniSearch.loadJSON<Stored>(json, { fields: ['title', 'titles', 'text'], storeFields: ['title', 'titles', 'text'] });
}
watch(indexData, loadIndex);

// ---- Querying

/** Words people type for things the docs spell out, and the reverse. */
const SYNONYMS: Record<string, string[]> = {
  cu: ['compute', 'units'],
  cus: ['compute', 'units'],
  compute: ['cu'],
  pda: ['program', 'derived', 'address'],
  pdas: ['program', 'derived', 'address'],
  ata: ['associated', 'token', 'account'],
  atas: ['associated', 'token', 'account'],
  cpi: ['invoke', 'call'],
  cpis: ['invoke', 'call'],
  invoke: ['cpi'],
  tx: ['transaction'],
  txn: ['transaction'],
  sol: ['lamports'],
  lamports: ['sol'],
  loop: ['batch', 'rows', 'foreach'],
  loops: ['batch', 'rows', 'foreach'],
  batch: ['loop', 'rows'],
  if: ['conditional', 'when'],
  condition: ['conditional', 'when'],
  cost: ['compute', 'units', 'bytes'],
  price: ['oracle', 'pyth'],
  swap: ['jupiter'],
  ts: ['typescript'],
  js: ['typescript'],
  rs: ['rust'],
  err: ['error'],
  fail: ['error', 'require'],
  check: ['require', 'assert'],
  upload: ['finalize', 'template'],
  audit: ['security', 'trust'],
  safe: ['security', 'guardrails'],
  safety: ['security', 'guardrails'],
  trust: ['security'],
  debug: ['inspect', 'failure', 'error'],
  decode: ['inspect'],
  inspect: ['decode'],
  why: ['failure'],
  term: ['glossary'],
  terms: ['glossary'],
  define: ['glossary'],
};

const tokenize = (text: string) => text.toLowerCase().split(/[^\p{L}\p{N}_]+/u).filter(Boolean);

function run(terms: string[], combineWith: 'AND' | 'OR') {
  if (!index.value || !terms.length) return [];
  return index.value.search(terms.join(' '), {
    combineWith,
    prefix: (term, position, all) => position === all.length - 1 || term.length > 3,
    // Long words tolerate two slips, so transposed letters ("comptue", "recieve") still match.
    // A word already corrected is searched as spelled.
    fuzzy: (term) => (corrected.has(term) ? false : term.length >= 6 ? 0.3 : term.length > 4 ? 0.2 : false),
    boost: { title: 5, titles: 2.5, text: 1 },
    tokenize,
  }) as Hit[];
}

/** Whether any indexed word starts with this one. */
function known(term: string) {
  return !!index.value && index.value.search(term, { prefix: true, fuzzy: false, tokenize }).length > 0;
}
const corrected = new Set<string>();
/** A word the index has never seen, with two neighbouring letters swapped into one it has ("comptue"). */
function correct(term: string) {
  if (term.length < 4 || /\d/.test(term) || known(term)) return term;
  for (let at = 0; at < term.length - 1; at++) {
    const swapped = term.slice(0, at) + term[at + 1] + term[at] + term.slice(at + 2);
    if (swapped !== term && known(swapped)) {
      corrected.add(swapped);
      return swapped;
    }
  }
  return term;
}
const terms = computed(() => {
  corrected.clear();
  return index.value ? tokenize(query.value).map(correct) : [];
});
/** Set when a typo was corrected, so the palette can say what it searched for. */
const correction = computed(() => {
  const typed = tokenize(query.value).join(' ');
  const used = terms.value.join(' ');
  return used !== typed ? used : '';
});

const results = computed<Hit[]>(() => {
  const words = terms.value;
  if (!words.length || !index.value) return [];
  const started = performance.now();
  // Every word first; if that finds little, any word, synonyms included, ranked below.
  const strict = run(words, 'AND');
  let hits = strict;
  if (strict.length < 6) {
    const seen = new Set(strict.map((hit) => hit.id));
    const expanded = [...new Set(words.flatMap((term) => [term, ...(SYNONYMS[term] ?? [])]))];
    const loose = run(expanded, 'OR').filter((hit) => !seen.has(hit.id)).map((hit) => ({ ...hit, score: hit.score * 0.4 }));
    hits = [...strict, ...loose];
  }
  const kept = hits.filter((hit) => filter.value === 'all' || String(hit.id).includes(`/${filter.value}/`));
  took.value = Math.max(1, Math.round(performance.now() - started));
  return kept.slice(0, 60);
});

/** Hits grouped by page, pages ordered by their best hit, at most four sections each. */
const groups = computed<Group[]>(() => {
  const byPage = new Map<string, Group>();
  for (const hit of results.value) {
    const page = String(hit.id).split('#')[0];
    let group = byPage.get(page);
    if (!group) {
      group = { page, title: hit.titles[0] ?? hit.title, hits: [] };
      byPage.set(page, group);
    }
    if (group.hits.length < 4) group.hits.push(hit);
  }
  return [...byPage.values()].slice(0, 12);
});
const flat = computed(() => groups.value.flatMap((group) => group.hits));
const hitId = (hit: Hit) => `search-hit-${flat.value.indexOf(hit)}`;

/** Edit distance counting a swap of neighbouring letters as one edit. */
function distance(a: string, b: string) {
  const d = Array.from({ length: a.length + 1 }, (_, i) => [i, ...Array(b.length).fill(0)]);
  for (let j = 1; j <= b.length; j++) d[0][j] = j;
  for (let i = 1; i <= a.length; i++) {
    for (let j = 1; j <= b.length; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      d[i][j] = Math.min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + cost);
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) d[i][j] = Math.min(d[i][j], d[i - 2][j - 2] + 1);
    }
  }
  return d[a.length][b.length];
}
/** When nothing matches, the query with each unknown word replaced by the closest word the docs use. */
const didYouMean = computed(() => {
  const typed = tokenize(query.value);
  if (!index.value || !typed.length || groups.value.length) return '';
  const fixed = typed.map((term) => {
    if (term.length < 3 || known(term)) return term;
    const candidates = new Set(index.value!.autoSuggest(term, { fuzzy: 0.6, prefix: false, tokenize }).flatMap((item) => item.terms));
    let best = term;
    let bestDistance = Math.max(1, Math.floor(term.length / 3)) + 1;
    for (const candidate of candidates) {
      const d = distance(term, candidate);
      if (d < bestDistance || (d === bestDistance && candidate.length < best.length)) [best, bestDistance] = [candidate, d];
    }
    return best;
  });
  const suggestion = fixed.join(' ');
  return suggestion !== typed.join(' ') ? suggestion : '';
});
const counts = computed(() => {
  const all = results.value.length;
  return { all, pages: groups.value.length };
});

// ---- Showing a hit

const escape = (text: string) => text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
function markTerms(text: string, terms: string[]) {
  const words = [...new Set(terms)].filter((term) => term.length > 1).sort((a, b) => b.length - a.length);
  if (!words.length) return escape(text);
  // Matches start at a word, so "pda" lights up "PDA" and "PDAs" but not the middle of "assertPda".
  const pattern = new RegExp(`(?<![\\p{L}\\p{N}])(${words.map((word) => word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|')})`, 'giu');
  return text
    .split(pattern)
    .map((part, position) => (position % 2 ? `<mark>${escape(part)}</mark>` : escape(part)))
    .join('');
}
/** About 150 characters of the section, around the first place a matched word occurs. */
function snippet(hit: Hit) {
  const text = (hit.text ?? '').replace(/\s+/g, ' ').trim();
  if (!text) return '';
  const lower = text.toLowerCase();
  const at = Math.min(...hit.terms.map((term) => lower.indexOf(term)).filter((position) => position >= 0), text.length);
  let start = at === text.length ? 0 : Math.max(0, at - 50);
  // Begin on a whole word.
  if (start > 0) start = Math.min(at, text.indexOf(' ', start) + 1 || start);
  const piece = text.slice(start, start + 160);
  return `${start > 0 ? '…' : ''}${markTerms(piece, hit.terms)}${start + 160 < text.length ? '…' : ''}`;
}
const breadcrumb = (hit: Hit) => [...hit.titles.slice(1), hit.title].join(' › ') || hit.title;
const sectionOf = (page: string) => (page.includes('/guide/') ? 'Guide' : page.includes('/examples/') ? 'Examples' : page.includes('/reference/') ? 'Reference' : 'Docs');

// ---- Recent searches and suggestions

const RECENT_KEY = 'ballista:recent-searches';
const recent = ref<string[]>([]);
function readRecent() {
  try {
    recent.value = JSON.parse(localStorage.getItem(RECENT_KEY) ?? '[]').slice(0, 6);
  } catch {
    recent.value = [];
  }
}
function remember(term: string) {
  const clean = term.trim();
  if (!clean) return;
  recent.value = [clean, ...recent.value.filter((item) => item !== clean)].slice(0, 6);
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(recent.value));
  } catch {
    // Recent searches are a convenience; the palette works without them.
  }
}
const suggestions = ['sweep a balance', 'PDA bump', 'compute units', 'loops', 'failure modes', 'inspect a template', 'security', 'limits'];
const startHere = [
  { title: 'Getting started', path: '/guide/getting-started' },
  { title: 'How it works', path: '/guide/mental-model' },
  { title: 'All examples', path: '/examples/' },
  { title: 'Glossary', path: '/reference/glossary' },
  { title: 'Limits', path: '/reference/limits' },
];

// ---- Navigation

function open(id: string, newTab = false) {
  remember(query.value);
  if (newTab) {
    window.open(id, '_blank', 'noopener');
    return;
  }
  emit('close');
  router.go(id);
}
function move(step: number) {
  if (!flat.value.length) return;
  selected.value = (selected.value + step + flat.value.length) % flat.value.length;
  nextTick(() => list.value?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' }));
}
const filters = ['all', 'guide', 'examples', 'reference'] as const;
/** Section filters are a tab row: click one, or focus it and use the arrow keys. */
function onFilterKey(event: KeyboardEvent) {
  if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
  event.preventDefault();
  const row = event.currentTarget as HTMLElement;
  const step = event.key === 'ArrowRight' ? 1 : -1;
  filter.value = filters[(filters.indexOf(filter.value) + step + filters.length) % filters.length];
  nextTick(() => row.querySelector<HTMLElement>('[aria-selected="true"]')?.focus());
}
function onKey(event: KeyboardEvent) {
  const inResults = !!list.value?.contains(document.activeElement);
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    // The selection moves; if focus is already in the results it moves with it.
    event.preventDefault();
    move(event.key === 'ArrowDown' ? 1 : -1);
    if (inResults) nextTick(() => document.getElementById(`search-hit-${selected.value}`)?.focus());
  } else if (event.key === 'Tab' && !event.shiftKey && event.target === input.value && flat.value.length) {
    // Tab steps from the query into the results, starting at the selected one.
    event.preventDefault();
    document.getElementById(`search-hit-${selected.value}`)?.focus();
  } else if (event.key === 'Enter' && (event.target === input.value || inResults)) {
    const hit = flat.value[selected.value];
    if (hit) {
      event.preventDefault();
      open(String(hit.id), event.metaKey || event.ctrlKey);
    }
  } else if (event.key === 'Escape') {
    event.preventDefault();
    emit('close');
  }
}
watch([query, filter], () => (selected.value = 0));

// Focus goes back where it was when the palette closes.
const previousFocus = typeof document === 'undefined' ? null : (document.activeElement as HTMLElement | null);
let previousOverflow = '';
onMounted(async () => {
  readRecent();
  previousOverflow = document.body.style.overflow;
  document.body.style.overflow = 'hidden';
  input.value?.focus();
  await loadIndex();
});
onBeforeUnmount(() => {
  document.body.style.overflow = previousOverflow;
  const target = previousFocus;
  if (target && target !== document.body && typeof target.focus === 'function') {
    requestAnimationFrame(() => {
      if (target.isConnected) target.focus({ preventScroll: true });
    });
  }
});
</script>

<template>
  <Teleport to="body">
    <div class="search-palette" role="dialog" aria-modal="true" aria-label="Search the docs" @keydown="onKey">
      <div class="search-backdrop" @click="emit('close')" />
      <div class="search-panel">
        <label class="search-field">
          <svg viewBox="0 0 20 20" aria-hidden="true"><circle cx="8.5" cy="8.5" r="5.5" /><path d="m13 13 4.5 4.5" /></svg>
          <input
            ref="input"
            v-model="query"
            type="search"
            placeholder="Search guides, examples and the API"
            aria-label="Search"
            autocomplete="off"
            spellcheck="false"
            role="combobox"
            aria-autocomplete="list"
            aria-controls="search-results"
            :aria-expanded="flat.length > 0"
            :aria-activedescendant="flat.length ? `search-hit-${selected}` : undefined"
          />
          <kbd @click="emit('close')">esc</kbd>
        </label>

        <div class="search-filters">
          <div class="search-filter-tabs" role="tablist" aria-label="Section" @keydown="onFilterKey">
          <button
            v-for="option in filters"
            :key="option"
            type="button"
            role="tab"
            :aria-selected="filter === option"
            :tabindex="filter === option ? 0 : -1"
            @click="filter = option; input?.focus()"
          >{{ option === 'all' ? 'Everything' : option[0].toUpperCase() + option.slice(1) }}</button>
          </div>
          <span v-if="query" class="search-stats">{{ counts.all }} match{{ counts.all === 1 ? '' : 'es' }} · {{ counts.pages }} page{{ counts.pages === 1 ? '' : 's' }} · {{ took }} ms</span>
        </div>

        <div id="search-results" ref="list" class="search-results" :role="flat.length ? 'listbox' : undefined" aria-label="Results">
          <template v-if="!query">
            <section v-if="recent.length" class="search-group">
              <h3>Recent</h3>
              <div class="search-chips">
                <button v-for="item in recent" :key="item" type="button" @click="query = item; input?.focus()">↺ {{ item }}</button>
              </div>
            </section>
            <section class="search-group">
              <h3>Try</h3>
              <div class="search-chips">
                <button v-for="item in suggestions" :key="item" type="button" @click="query = item; input?.focus()">{{ item }}</button>
              </div>
            </section>
            <section class="search-group">
              <h3>Start here</h3>
              <a v-for="item in startHere" :key="item.path" class="search-hit" :href="withBase(item.path)" @click.prevent="open(withBase(item.path))">
                <span class="search-hit-title">{{ item.title }}</span>
              </a>
            </section>
          </template>

          <p v-else-if="!index" class="search-empty">Loading the index…</p>
          <template v-else-if="!groups.length">
            <p class="search-empty">
              Nothing for “{{ query }}”.
              <template v-if="didYouMean">Did you mean <button type="button" class="search-suggest" @click="query = didYouMean; input?.focus()">{{ didYouMean }}</button>?</template>
              <template v-else-if="filter !== 'all'">Try <button type="button" @click="filter = 'all'; input?.focus()">searching everything</button>.</template>
              <template v-else>Try fewer or shorter words.</template>
            </p>
            <section class="search-group">
              <h3>Or start here</h3>
              <a v-for="item in startHere" :key="item.path" class="search-hit" :href="withBase(item.path)" @click.prevent="open(withBase(item.path))">
                <span class="search-hit-title">{{ item.title }}</span>
              </a>
            </section>
          </template>

          <p v-if="correction && groups.length" class="search-correction">
            Showing results for <strong>{{ correction }}</strong>
          </p>
          <section v-for="(group, groupIndex) in groups" :key="group.page" class="search-group" role="group" :aria-labelledby="`search-group-${groupIndex}`">
            <h3 :id="`search-group-${groupIndex}`">
              <span>{{ group.title }}</span>
              <span class="search-section">{{ sectionOf(group.page) }}</span>
            </h3>
            <a
              v-for="hit in group.hits"
              :key="hit.id"
              :id="hitId(hit)"
              class="search-hit"
              role="option"
              :href="String(hit.id)"
              :aria-selected="flat[selected]?.id === hit.id"
              @mouseenter="selected = flat.indexOf(hit)"
              @focus="selected = flat.indexOf(hit)"
              @click.prevent="open(String(hit.id), $event.metaKey || $event.ctrlKey)"
            >
              <span class="search-hit-title" v-html="markTerms(breadcrumb(hit), hit.terms)" />
              <span v-if="snippet(hit)" class="search-hit-text" v-html="snippet(hit)" />
            </a>
          </section>
        </div>

        <footer class="search-keys">
          <span><kbd>↑</kbd><kbd>↓</kbd> move</span>
          <span><kbd>↵</kbd> open</span>
          <span><kbd>⌘</kbd><kbd>↵</kbd> new tab</span>
          <span><kbd>tab</kbd> into results</span>
        </footer>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.search-palette {
  position: fixed;
  inset: 0;
  z-index: 300;
  display: flex;
  justify-content: center;
  /* The panel is as tall as its results, up to its max height. */
  align-items: flex-start;
  padding: 11vh 16px 16px;
}
.search-backdrop {
  position: absolute;
  inset: 0;
  background: color-mix(in srgb, var(--ink) 30%, transparent);
  backdrop-filter: blur(3px);
  animation: search-fade 0.16s ease-out;
}
.search-panel {
  position: relative;
  width: min(760px, 100%);
  max-height: 78vh;
  display: flex;
  flex-direction: column;
  background: var(--paper);
  border: 1px solid var(--ink);
  box-shadow: 8px 8px 0 var(--shadow);
  animation: search-rise 0.2s cubic-bezier(0.2, 0.8, 0.2, 1);
}
.search-field {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 16px 18px;
  border-bottom: 1px solid var(--rule);
}
.search-field svg {
  width: 20px;
  height: 20px;
  flex: none;
  fill: none;
  stroke: var(--signal);
  stroke-width: 1.8;
  stroke-linecap: round;
}
.search-field input {
  flex: 1;
  min-width: 0;
  font-family: var(--display-font);
  font-size: 24px;
  letter-spacing: -0.02em;
  color: var(--ink);
  background: transparent;
}
.search-field input::placeholder {
  color: var(--muted);
}
.search-field input::-webkit-search-cancel-button {
  display: none;
}
kbd {
  display: inline-block;
  min-width: 22px;
  padding: 2px 6px;
  border: 1px solid var(--rule);
  background: var(--surface);
  font-family: var(--vp-font-family-mono);
  font-size: 10px;
  line-height: 1.5;
  text-align: center;
  color: var(--muted);
}
.search-field kbd {
  cursor: pointer;
}
.search-filters {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 10px 18px;
  border-bottom: 1px solid var(--rule);
  font-family: var(--vp-font-family-mono);
  font-size: 11px;
}
.search-filter-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
.search-filters button {
  padding: 4px 10px;
  border: 1px solid transparent;
  color: var(--muted);
  cursor: pointer;
}
.search-filters button:hover {
  color: var(--ink);
}
.search-filters button[aria-selected='true'] {
  border-color: var(--ink);
  color: var(--ink);
  background: var(--surface);
}
.search-stats {
  margin-left: auto;
  color: var(--muted);
}
.search-results {
  flex: 0 1 auto;
  min-height: 0;
  overflow-y: auto;
  padding: 6px 0 12px;
  overscroll-behavior: contain;
}
.search-group {
  padding: 8px 10px 4px;
}
.search-group h3 {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  margin: 6px 8px 6px;
  font-family: var(--vp-font-family-mono);
  font-size: 10px;
  font-weight: 400;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--muted);
}
.search-section {
  color: var(--muted);
}
.search-hit {
  display: grid;
  gap: 3px;
  padding: 9px 12px 10px 14px;
  border-left: 2px solid transparent;
  color: var(--ink);
  text-decoration: none;
}
.search-hit[aria-selected='true'],
.search-hit:hover {
  background: var(--surface);
  border-left-color: var(--signal);
}
.search-hit-title {
  font-size: 15px;
  line-height: 1.35;
  letter-spacing: -0.01em;
}
.search-hit-text {
  font-size: 12.5px;
  line-height: 1.55;
  color: var(--muted);
}
.search-hit :deep(mark) {
  background: none;
  color: var(--signal);
  font-weight: 600;
  box-shadow: inset 0 -1px 0 currentColor;
}
.search-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding: 2px 8px 8px;
}
.search-chips button {
  padding: 6px 11px;
  border: 1px solid var(--rule);
  font-family: var(--vp-font-family-mono);
  font-size: 11.5px;
  color: var(--ink);
  cursor: pointer;
}
.search-chips button:hover {
  border-color: var(--signal);
  color: var(--signal);
}
.search-empty {
  margin: 28px 24px;
  font-size: 14px;
  color: var(--muted);
}
.search-correction {
  margin: 6px 24px 0;
  font-family: var(--vp-font-family-mono);
  font-size: 11px;
  color: var(--muted);
}
.search-correction strong {
  font-weight: 500;
  color: var(--signal);
}
.search-empty + .search-group {
  padding-top: 0;
}
.search-empty button {
  color: var(--signal);
  text-decoration: underline;
  cursor: pointer;
}
.search-keys {
  display: flex;
  gap: 18px;
  padding: 10px 18px;
  border-top: 1px solid var(--rule);
  font-family: var(--vp-font-family-mono);
  font-size: 10px;
  color: var(--muted);
}
.search-keys kbd {
  margin-right: 3px;
}
@keyframes search-fade {
  from {
    opacity: 0;
  }
}
@keyframes search-rise {
  from {
    opacity: 0;
    transform: translateY(10px) scale(0.99);
  }
}
@media (max-width: 767px) {
  .search-palette {
    padding: 0;
  }
  .search-panel {
    max-height: 100vh;
    height: 100%;
    border: 0;
    box-shadow: none;
  }
  .search-field input {
    font-size: 20px;
  }
  .search-keys {
    display: none;
  }
  /* The counts get their own row rather than squeezing the filters. */
  .search-filters {
    flex-wrap: wrap;
    row-gap: 6px;
    padding: 10px 12px;
  }
  .search-stats {
    flex-basis: 100%;
    margin-left: 10px;
  }
  .search-filters,
  .search-group h3,
  kbd {
    font-size: 11px;
  }
}
</style>
