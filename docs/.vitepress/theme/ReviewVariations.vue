<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue';
import { useRoute } from 'vitepress';

// Previews the variations written for a review comment (see ../review-comments.ts): a picker hovers
// beside each set's anchor and switches the page between the current version and each variant.
// A set can arrive while it is still being written (`status: 'drafting'`), its variants appearing
// one by one, and it says when the kept variant is in the source (`appliedAt`).

interface Variant {
  label: string;
  note?: string;
  css?: string;
  text?: { selector: string; text: string }[];
  html?: { selector: string; html: string }[];
}
interface VariationSet {
  id: string;
  commentId?: string;
  page: string;
  anchor: string;
  request: string;
  variants: Variant[];
  status?: 'drafting' | 'ready';
  /** When the comment was picked up; the picker counts the seconds while options are written. */
  createdAt?: string;
  chosen?: number | null;
  appliedAt?: string;
  /** What changed in the source, such as `FieldManual.vue · +12 −8`. */
  appliedNote?: string;
}
interface Placement {
  top: number;
  left: number;
  found: boolean;
  below: boolean;
}

const ENDPOINT = '/__review/variations';
const LETTERS = 'ABCDEFGH';
const APPLIED_LINGER_MS = 1800;
const route = useRoute();
const sets = ref<VariationSet[]>([]);
/** The variant each set is showing: 0 is the current page. */
const showing = reactive<Record<string, number>>({});
const placements = reactive<Record<string, Placement>>({});
const outline = ref<{ top: number; left: number; width: number; height: number } | null>(null);
const busy = ref('');
const now = ref(Date.now());
let clock: ReturnType<typeof setInterval> | undefined;
/** Sets the reader has switched by hand; the rest follow the newest proposal. */
const touched = new Set<string>();
/** Sets applied during this visit: their picker says so, then leaves; the preview stays up. */
const retiring = reactive<Record<string, boolean>>({});
const retired = new Set<string>();
/** Sets already in the source when the page loaded: nothing left to preview. */
const settled = new Set<string>();

const letter = (index: number) => (index === 0 ? 'Now' : LETTERS[index - 1] ?? String(index));
const storageKey = (id: string) => `ballista-review-variant:${id}`;
function remembered(id: string): number | null {
  try {
    const value = sessionStorage.getItem(storageKey(id));
    return value === null ? null : Number(value);
  } catch {
    return null;
  }
}
function remember(id: string, index: number) {
  try {
    sessionStorage.setItem(storageKey(id), String(index));
  } catch {
    // Previewing still works; it just won't survive a reload.
  }
}

// ---- Applying a variant to the live page

interface Applied {
  signature: string;
  style?: HTMLStyleElement;
  restores: (() => void)[];
}
const applied = new Map<string, Applied>();

function unapply(id: string) {
  const current = applied.get(id);
  if (current) {
    current.style?.remove();
    // Put back the original nodes, not copies, so the page's own components keep working.
    for (const restore of current.restores.reverse()) restore();
    applied.delete(id);
  }
  document.documentElement.removeAttribute(`data-rv-${id}`);
}

function apply(set: VariationSet, index: number) {
  const variant = index > 0 ? set.variants[index - 1] : undefined;
  const signature = `${index}:${JSON.stringify(variant ?? null)}`;
  if (applied.get(set.id)?.signature === signature) return;
  unapply(set.id);
  document.documentElement.setAttribute(`data-rv-${set.id}`, String(index));
  const next: Applied = { signature, restores: [] };
  applied.set(set.id, next);
  if (!variant) return;
  if (variant.css) {
    next.style = document.createElement('style');
    next.style.dataset.reviewVariant = set.id;
    next.style.textContent = variant.css;
    document.head.append(next.style);
  }
  const replace = (selector: string, fill: (element: Element) => void) => {
    let elements: Element[] = [];
    try {
      elements = Array.from(document.querySelectorAll(selector));
    } catch {
      return;
    }
    for (const element of elements) {
      const original = Array.from(element.childNodes);
      fill(element);
      next.restores.push(() => element.replaceChildren(...original));
    }
  };
  for (const patch of variant.text ?? []) replace(patch.selector, (element) => (element.textContent = patch.text));
  for (const patch of variant.html ?? []) replace(patch.selector, (element) => (element.innerHTML = patch.html));
}

/** Swaps the page with a short crossfade where the browser supports it, so nothing jumps. */
function crossfade(change: () => void): Promise<void> {
  const motion = !window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  type Transition = { finished: Promise<void>; updateCallbackDone: Promise<void> };
  const start = (document as Document & { startViewTransition?: (update: () => void) => Transition }).startViewTransition;
  if (!motion || !start) {
    change();
    return Promise.resolve();
  }
  // The class scopes the crossfade styles to this transition, apart from the theme toggle's own.
  const root = document.documentElement;
  root.classList.add('rv-switching');
  const transition = start.call(document, change);
  transition.finished.finally(() => root.classList.remove('rv-switching'));
  // The swap itself runs a moment later; callers measure the page after it.
  return transition.updateCallbackDone.catch(() => undefined);
}

function show(set: VariationSet, index: number) {
  touched.add(set.id);
  showing[set.id] = index;
  remember(set.id, index);
  crossfade(() => apply(set, index)).then(() => nextTick(place));
}

// ---- Loading and keeping

let firstLoad = true;
async function load() {
  let next: VariationSet[] = [];
  try {
    const response = await fetch(`${ENDPOINT}?page=${encodeURIComponent(route.path)}`);
    next = response.ok ? await response.json() : [];
  } catch {
    next = [];
  }
  window.dispatchEvent(new CustomEvent('review-variations', { detail: next }));
  if (firstLoad) {
    for (const set of next) if (set.appliedAt) settled.add(set.id);
    firstLoad = false;
  }
  next = next.filter((set) => !settled.has(set.id) && !retired.has(set.id));
  const ids = new Set(next.map((set) => set.id));
  for (const id of [...applied.keys()]) if (!ids.has(id) && !retired.has(id)) unapply(id);
  for (const id of Object.keys(showing)) if (!ids.has(id) && !retired.has(id)) delete showing[id];
  const changes: (() => void)[] = [];
  for (const set of next) {
    // A fresh set opens on its first variant, so the proposal is on screen straight away.
    let index = showing[set.id] ?? remembered(set.id) ?? 1;
    if (index === 0 && !touched.has(set.id) && set.variants.length > 0 && typeof set.chosen !== 'number') index = 1;
    showing[set.id] = Math.min(Math.max(index, 0), set.variants.length);
    const shown = showing[set.id];
    const variant = shown > 0 ? set.variants[shown - 1] : undefined;
    if (applied.get(set.id)?.signature !== `${shown}:${JSON.stringify(variant ?? null)}`) changes.push(() => apply(set, shown));
    if (set.appliedAt && !retiring[set.id]) {
      retiring[set.id] = true;
      setTimeout(() => {
        retired.add(set.id);
        sets.value = sets.value.filter((candidate) => candidate.id !== set.id);
        delete retiring[set.id];
      }, APPLIED_LINGER_MS);
    }
  }
  if (changes.length) await crossfade(() => changes.forEach((change) => change()));
  sets.value = next;
  await nextTick();
  place();
  // The first pass used an estimated height; now the pickers exist, place them by their real one.
  await nextTick();
  place();
}

async function keep(set: VariationSet) {
  busy.value = set.id;
  try {
    const response = await fetch(`${ENDPOINT}/choose`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ id: set.id, variant: showing[set.id] ?? 0 }),
    });
    if (response.ok) await load();
  } finally {
    busy.value = '';
  }
}

// ---- Placing each picker beside its anchor

const PICKER_WIDTH = 300;
const GAP = 12;
const pickerElements = new Map<string, HTMLElement>();
function registerPicker(id: string, element: unknown) {
  if (element instanceof HTMLElement) pickerElements.set(id, element);
  else pickerElements.delete(id);
}

/**
 * The box around everything a set can change: its anchor and every element any of its variants
 * rewrites. The outline and the picker use it, so neither can undersell what a variant touches.
 */
function regionOf(set: VariationSet): DOMRect | null {
  const selectors = [set.anchor];
  for (const variant of set.variants) {
    for (const patch of [...(variant.text ?? []), ...(variant.html ?? [])]) selectors.push(patch.selector);
  }
  let box: { left: number; top: number; right: number; bottom: number } | null = null;
  for (const selector of selectors) {
    let elements: Element[] = [];
    try {
      elements = Array.from(document.querySelectorAll(selector));
    } catch {
      continue;
    }
    for (const element of elements) {
      const r = element.getBoundingClientRect();
      if (!r.width && !r.height) continue;
      box = box
        ? { left: Math.min(box.left, r.left), top: Math.min(box.top, r.top), right: Math.max(box.right, r.right), bottom: Math.max(box.bottom, r.bottom) }
        : { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
    }
  }
  return box ? new DOMRect(box.left, box.top, box.right - box.left, box.bottom - box.top) : null;
}

/** Above the region, right-aligned like a toolbar; below it only when the page has no room above. Never over it. */
function place() {
  const stacked: Record<string, number> = {};
  for (const set of sets.value) {
    const height = pickerElements.get(set.id)?.offsetHeight || 110;
    const rect = regionOf(set);
    const offset = (stacked[set.anchor] = (stacked[set.anchor] ?? -1) + 1) * (height + 8);
    if (!rect || (rect.width === 0 && rect.height === 0)) {
      placements[set.id] = { top: window.scrollY + 90 + offset, left: window.innerWidth - PICKER_WIDTH - 16, found: false, below: false };
      continue;
    }
    const pageTop = rect.top + window.scrollY;
    const above = pageTop - height - GAP - offset;
    const below = above < 8;
    const top = below ? rect.bottom + window.scrollY + GAP + offset : above;
    const left = Math.min(Math.max(rect.right - PICKER_WIDTH, 12), window.innerWidth - PICKER_WIDTH - 12);
    placements[set.id] = { top, left: left + window.scrollX, found: true, below };
  }
  // A variant can change the layout under the pointer; keep the outline on what it now covers.
  if (hovered) highlight(hovered);
}

let hovered: VariationSet | null = null;
function highlight(set: VariationSet | null) {
  hovered = set;
  const rect = set ? regionOf(set) : null;
  outline.value = rect
    ? { top: rect.top + window.scrollY - 6, left: rect.left + window.scrollX - 6, width: rect.width + 12, height: rect.height + 12 }
    : null;
}

const pickers = computed(() =>
  sets.value.map((set) => {
    const index = showing[set.id] ?? 0;
    const variant = index > 0 ? set.variants[index - 1] : undefined;
    const drafting = set.status === 'drafting';
    let caption = variant ? `${letter(index)} · ${variant.label}${variant.note ? ` — ${variant.note}` : ''}` : 'Now · the page as it is';
    if (drafting && !set.variants.length) {
      const seconds = set.createdAt ? Math.round((now.value - Date.parse(set.createdAt)) / 1000) : 0;
      caption = `Claude is reading the section${seconds >= 2 ? ` · ${seconds}s` : ''}`;
    }
    return {
      set,
      index,
      caption,
      drafting,
      placement: placements[set.id],
      kept: typeof set.chosen === 'number',
      applied: Boolean(set.appliedAt),
    };
  }),
);

let resize: ResizeObserver | undefined;
const onResize = () => place();
onMounted(() => {
  load();
  clock = setInterval(() => {
    if (sets.value.some((set) => set.status === 'drafting' && !set.variants.length)) now.value = Date.now();
  }, 1000);
  window.addEventListener('resize', onResize);
  resize = new ResizeObserver(() => place());
  resize.observe(document.body);
  import.meta.hot?.on('review:changed', (data: { file?: string }) => {
    if (data?.file === 'variations') load();
  });
});
onBeforeUnmount(() => {
  clearInterval(clock);
  for (const id of [...applied.keys()]) unapply(id);
  window.removeEventListener('resize', onResize);
  resize?.disconnect();
});
watch(
  () => route.path,
  () => {
    for (const id of [...applied.keys()]) unapply(id);
    retired.clear();
    sets.value = [];
    // Let the new page render before looking for anchors.
    setTimeout(load, 50);
  },
);
</script>

<template>
  <Teleport to="body">
    <div class="review-variations review-ui">
      <div
        v-if="outline"
        class="variation-outline"
        :style="{ top: `${outline.top}px`, left: `${outline.left}px`, width: `${outline.width}px`, height: `${outline.height}px` }"
      />
      <TransitionGroup name="variation-picker">
        <section
          v-for="picker in pickers"
          :key="picker.set.id"
          :ref="(element) => registerPicker(picker.set.id, element)"
          class="variation-picker"
          :class="{
            'is-lost': picker.placement && !picker.placement.found,
            'is-below': picker.placement?.below,
            'is-applied': picker.applied,
          }"
          :style="picker.placement ? { top: `${picker.placement.top}px`, left: `${picker.placement.left}px` } : { visibility: 'hidden' }"
          :aria-label="`Variations for: ${picker.set.request}`"
          @mouseenter="highlight(picker.set)"
          @mouseleave="highlight(null)"
        >
          <p class="variation-request" :title="picker.set.request">{{ picker.set.request }}</p>
          <div v-if="!picker.kept" class="variation-row">
            <TransitionGroup tag="div" name="variation-option" class="variation-options" role="group" aria-label="Variant">
              <button
                v-for="index in picker.set.variants.length + 1"
                :key="index"
                type="button"
                :aria-pressed="picker.index === index - 1"
                :title="index === 1 ? 'The page as it is' : picker.set.variants[index - 2].label"
                @click="show(picker.set, index - 1)"
              >{{ letter(index - 1) }}</button>
              <span v-if="picker.drafting" key="drafting" class="variation-drafting" aria-hidden="true" />
            </TransitionGroup>
            <button
              type="button"
              class="variation-keep"
              :disabled="busy === picker.set.id || (picker.drafting && !picker.set.variants.length)"
              @click="keep(picker.set)"
            >
              {{ picker.index === 0 ? 'Keep as is' : `Keep ${letter(picker.index)}` }}
            </button>
          </div>
          <p class="variation-caption" aria-live="polite">
            <template v-if="picker.applied">
              <span class="variation-check">✓</span> Applied{{ picker.set.appliedNote ? ` · ${picker.set.appliedNote}` : '' }}
            </template>
            <template v-else-if="picker.kept">
              Kept {{ letter(picker.set.chosen ?? 0) }}. Claude is making the change in the source<span class="variation-dots" />
            </template>
            <template v-else>
              {{ picker.caption }}<span v-if="picker.drafting" class="variation-dots" />
            </template>
          </p>
          <p v-if="picker.placement && !picker.placement.found" class="variation-lost">Can't find {{ picker.set.anchor }} on this page.</p>
        </section>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.variation-picker {
  position: absolute;
  z-index: 190;
  width: 300px;
  display: grid;
  gap: 8px;
  padding: 10px 12px;
  background: var(--paper);
  border: 1px solid var(--ink);
  box-shadow: 4px 4px 0 rgba(34, 35, 31, 0.12);
  font-family: var(--vp-font-family-mono);
  transition: border-color 0.3s, top 0.3s cubic-bezier(0.2, 0.8, 0.2, 1), left 0.3s cubic-bezier(0.2, 0.8, 0.2, 1);
}
.variation-picker::after {
  content: '';
  position: absolute;
  right: 22px;
  bottom: -6px;
  width: 10px;
  height: 10px;
  background: var(--paper);
  border-right: 1px solid var(--ink);
  border-bottom: 1px solid var(--ink);
  transform: rotate(45deg);
}
.variation-picker.is-below::after {
  top: -6px;
  bottom: auto;
  transform: rotate(-135deg);
}
.variation-picker.is-lost::after {
  display: none;
}
.variation-picker.is-applied {
  border-color: #2f7a4b;
}
.variation-request {
  margin: 0;
  font-size: 10px;
  line-height: 1.5;
  color: var(--muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.variation-request::before {
  content: '● ';
  color: var(--signal);
}
.variation-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.variation-options {
  display: flex;
  border: 1px solid var(--ink);
}
.variation-options button {
  min-width: 34px;
  padding: 5px 8px;
  font-size: 11px;
  cursor: pointer;
}
.variation-options > * + * {
  border-left: 1px solid var(--ink);
}
.variation-options button[aria-pressed='true'] {
  background: var(--ink);
  color: var(--paper);
}
.variation-options button:not([aria-pressed='true']):hover {
  background: #eae9e2;
}
.variation-drafting {
  width: 34px;
  background: linear-gradient(90deg, #eae9e2 0%, #fbfaf7 50%, #eae9e2 100%);
  background-size: 200% 100%;
  animation: variation-shimmer 1.1s linear infinite;
}
.variation-keep {
  padding: 6px 10px;
  background: var(--signal);
  color: var(--paper);
  font-size: 11px;
  cursor: pointer;
}
.variation-keep:hover {
  background: #a1301c;
}
.variation-keep:disabled {
  opacity: 0.5;
  cursor: wait;
}
.variation-caption,
.variation-lost {
  margin: 0;
  font-size: 10px;
  line-height: 1.5;
  color: var(--ink);
}
.variation-lost {
  color: var(--signal);
}
.variation-check {
  color: #2f7a4b;
  font-weight: 700;
}
.variation-dots::after {
  content: '…';
  display: inline-block;
  width: 1.2em;
  animation: variation-dots 1.2s steps(4) infinite;
  clip-path: inset(0 100% 0 0);
}
.variation-outline {
  position: absolute;
  z-index: 180;
  pointer-events: none;
  outline: 2px dashed var(--signal);
  outline-offset: 0;
}

/* Arrivals: a picker rises into place, and each option pops in as it is written. */
.variation-picker-enter-active {
  transition: opacity 0.35s ease, transform 0.35s cubic-bezier(0.2, 0.8, 0.2, 1);
}
.variation-picker-leave-active {
  transition: opacity 0.45s ease, transform 0.45s ease;
}
.variation-picker-enter-from {
  opacity: 0;
  transform: translateY(8px);
}
.variation-picker-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}
.variation-option-enter-active {
  transition: opacity 0.3s ease, transform 0.3s cubic-bezier(0.2, 0.8, 0.2, 1);
}
.variation-option-enter-from {
  opacity: 0;
  transform: scale(0.6);
}
@keyframes variation-shimmer {
  to {
    background-position: -200% 0;
  }
}
@keyframes variation-dots {
  to {
    clip-path: inset(0 -0.2em 0 0);
  }
}
@media (prefers-reduced-motion: reduce) {
  .variation-drafting,
  .variation-dots::after {
    animation: none;
  }
  .variation-dots::after {
    clip-path: none;
  }
}
</style>

<style>
/* The crossfade between variants: quick, so flipping between them feels immediate. */
html.rv-switching::view-transition-old(root),
html.rv-switching::view-transition-new(root) {
  animation-duration: 0.26s;
  animation-timing-function: ease;
}
</style>
