<script setup lang="ts">
// Replaces VitePress's local search box (see the alias in ../config.mts) and reads the same index.
// Over the default it adds: synonyms (CU, PDA, CPI, …), a strict-then-loose query, results grouped
// by page with a snippet of where the words matched, section filters, and recent searches.
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
};

const tokenize = (text: string) => text.toLowerCase().split(/[^\p{L}\p{N}_]+/u).filter(Boolean);

function run(terms: string[], combineWith: 'AND' | 'OR') {
  if (!index.value || !terms.length) return [];
  return index.value.search(terms.join(' '), {
    combineWith,
    prefix: (term, position, all) => position === all.length - 1 || term.length > 3,
    fuzzy: (term) => (term.length > 4 ? 0.2 : false),
    boost: { title: 5, titles: 2.5, text: 1 },
    tokenize,
  }) as Hit[];
}

const results = computed<Hit[]>(() => {
  const terms = tokenize(query.value);
  if (!terms.length || !index.value) return [];
  const started = performance.now();
  // Every word first; if that finds little, any word, synonyms included, ranked below.
  const strict = run(terms, 'AND');
  let hits = strict;
  if (strict.length < 6) {
    const seen = new Set(strict.map((hit) => hit.id));
    const expanded = [...new Set(terms.flatMap((term) => [term, ...(SYNONYMS[term] ?? [])]))];
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
const suggestions = ['sweep a balance', 'PDA bump', 'compute units', 'loops', 'errors', 'account groups', 'limits'];
const startHere = [
  { title: 'Getting started', path: '/guide/getting-started' },
  { title: 'Mental model', path: '/guide/mental-model' },
  { title: 'All examples', path: '/examples/' },
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
function onKey(event: KeyboardEvent) {
  if (event.key === 'ArrowDown') {
    event.preventDefault();
    move(1);
  } else if (event.key === 'ArrowUp') {
    event.preventDefault();
    move(-1);
  } else if (event.key === 'Enter') {
    const hit = flat.value[selected.value];
    if (hit) {
      event.preventDefault();
      open(String(hit.id), event.metaKey || event.ctrlKey);
    }
  } else if (event.key === 'Escape') {
    event.preventDefault();
    emit('close');
  } else if (event.key === 'Tab' && !event.shiftKey && query.value) {
    event.preventDefault();
    const order = ['all', 'guide', 'examples', 'reference'] as const;
    filter.value = order[(order.indexOf(filter.value) + 1) % order.length];
  }
}
watch([query, filter], () => (selected.value = 0));

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
            aria-controls="search-results"
          />
          <kbd @click="emit('close')">esc</kbd>
        </label>

        <div class="search-filters" role="tablist" aria-label="Section">
          <button
            v-for="option in (['all', 'guide', 'examples', 'reference'] as const)"
            :key="option"
            type="button"
            role="tab"
            :aria-selected="filter === option"
            @click="filter = option; input?.focus()"
          >{{ option === 'all' ? 'Everything' : option[0].toUpperCase() + option.slice(1) }}</button>
          <span v-if="query" class="search-stats">{{ counts.all }} match{{ counts.all === 1 ? '' : 'es' }} · {{ counts.pages }} page{{ counts.pages === 1 ? '' : 's' }} · {{ took }} ms</span>
        </div>

        <div id="search-results" ref="list" class="search-results" role="listbox">
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
          <p v-else-if="!groups.length" class="search-empty">
            Nothing for “{{ query }}”. Try fewer words, or <button type="button" @click="filter = 'all'">search everything</button>.
          </p>

          <section v-for="group in groups" :key="group.page" class="search-group">
            <h3>
              <span>{{ group.title }}</span>
              <span class="search-section">{{ sectionOf(group.page) }}</span>
            </h3>
            <a
              v-for="hit in group.hits"
              :key="hit.id"
              class="search-hit"
              role="option"
              :href="String(hit.id)"
              :aria-selected="flat[selected]?.id === hit.id"
              @mouseenter="selected = flat.indexOf(hit)"
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
          <span><kbd>tab</kbd> section</span>
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
  color: var(--faint);
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
  color: var(--faint);
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
  color: var(--faint);
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
}
</style>
