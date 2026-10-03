<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useRoute } from 'vitepress';

// How long each section of the page has been read, as a minimap in the bottom-right corner.
// A section is a `##` or `###` heading and everything under it, plus the intro above the first one.
// A section is fully read after the time a careful proofread of its words takes, at PROOFREAD_WPM.
// Each section is fingerprinted by its text, so changing it starts its reading time over. Time
// counts only while the tab is visible and focused and the reader moved in the last minute, and
// only for sections at least 75% on screen (or filling 75% of it); each second is split across
// those by how much of the screen they fill. Times are kept by the dev server in
// .local/review-reading.json; see ../review-comments.ts.

interface Section {
  fingerprint: string;
  heading: string;
  /** The heading's id, to jump to, or '' for the intro. */
  anchor: string;
  elements: HTMLElement[];
  /** Seconds a careful proofread of the section takes: its word count at PROOFREAD_WPM. */
  full: number;
  top: number;
  bottom: number;
}
interface Stored {
  heading: string;
  seconds: number;
}

const ENDPOINT = '/__review/reading';
/** Proofreading speed: a section counts as fully read after its words at this many a minute. */
const PROOFREAD_WPM = 100;
const IDLE_AFTER = 60_000;
/** How much of a section must be on screen for it to be read. */
const VISIBLE_SHARE = 0.75;
const FLUSH_EVERY = 10_000;

const route = useRoute();
const sections = ref<Section[]>([]);
const seconds = ref<Record<string, number>>({});
const view = ref({ top: 0, bottom: 0 });
const hovered = ref<number | null>(null);
/** Whether time is counting right now; the minimap says so when it isn't. */
const reading = ref(false);
let unsent: Record<string, Stored> = {};
let lastActivity = Date.now();
let tickTimer: ReturnType<typeof setInterval> | undefined;
let flushTimer: ReturnType<typeof setInterval> | undefined;
let observer: MutationObserver | undefined;
let rescanTimer: ReturnType<typeof setTimeout> | undefined;

/** FNV-1a, 32 bits, as hex: short and stable, enough to tell one version of a section from another. */
function fnv1a(text: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < text.length; index++) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, '0');
}

const textOf = (element: HTMLElement) =>
  (element.textContent ?? '').replace(/[​-‍﻿]/g, '').replace(/\s+/g, ' ').trim();

function scan() {
  const root = document.querySelector<HTMLElement>('.VPContent .vp-doc > div');
  if (!root) {
    sections.value = [];
    return;
  }
  const found: Omit<Section, 'fingerprint' | 'top' | 'bottom'>[] = [];
  const title = textOf(root.querySelector('h1') ?? root).replace(/#$/, '').trim();
  let current = { heading: title || 'Intro', anchor: '', elements: [] as HTMLElement[] };
  for (const child of Array.from(root.children) as HTMLElement[]) {
    if (child.matches('h2, h3')) {
      if (current.elements.length) found.push(current);
      current = { heading: textOf(child).replace(/#$/, '').trim(), anchor: child.id, elements: [] };
    }
    current.elements.push(child);
  }
  if (current.elements.length) found.push(current);
  sections.value = found.map((section) => {
    const text = section.elements.map(textOf).join('\n');
    const words = text.split(/\s+/).filter(Boolean).length;
    return {
      ...section,
      // The heading is part of the text, so two sections that read alike still differ.
      fingerprint: fnv1a(text),
      full: Math.max(5, (words / PROOFREAD_WPM) * 60),
      top: 0,
      bottom: 0,
    };
  });
  measure();
}

function measure() {
  for (const [index, section] of sections.value.entries()) {
    const first = section.elements[0].getBoundingClientRect();
    const next = sections.value[index + 1]?.elements[0].getBoundingClientRect();
    const last = section.elements[section.elements.length - 1].getBoundingClientRect();
    section.top = first.top + window.scrollY;
    section.bottom = (next ? next.top : last.bottom) + window.scrollY;
  }
  view.value = { top: window.scrollY, bottom: window.scrollY + window.innerHeight };
}

function tick() {
  measure();
  reading.value = isReading();
  if (!reading.value) return;
  const { top, bottom } = view.value;
  // A section counts only while at least VISIBLE_SHARE of it is on screen, or of the screen when
  // the section is taller than the screen.
  const overlaps = sections.value.map((section) => {
    const overlap = Math.max(0, Math.min(bottom, section.bottom) - Math.max(top, section.top));
    const fits = Math.min(section.bottom - section.top, bottom - top);
    return fits > 0 && overlap / fits >= VISIBLE_SHARE ? overlap : 0;
  });
  const total = overlaps.reduce((sum, overlap) => sum + overlap, 0);
  if (!total) return;
  const next = { ...seconds.value };
  for (const [index, section] of sections.value.entries()) {
    const share = overlaps[index] / total;
    if (!share) continue;
    next[section.fingerprint] = (next[section.fingerprint] ?? 0) + share;
    const pending = (unsent[section.fingerprint] ??= { heading: section.heading, seconds: 0 });
    pending.seconds += share;
  }
  seconds.value = next;
}

function flush(useBeacon = false) {
  const add = unsent;
  if (!Object.keys(add).length) return;
  unsent = {};
  const body = JSON.stringify({ page: route.path, add });
  if (useBeacon && navigator.sendBeacon) {
    navigator.sendBeacon(ENDPOINT, new Blob([body], { type: 'application/json' }));
    return;
  }
  fetch(ENDPOINT, { method: 'POST', headers: { 'content-type': 'application/json' }, body }).catch(() => {
    // Keep the time for the next flush rather than lose it.
    for (const [fingerprint, entry] of Object.entries(add)) {
      const pending = (unsent[fingerprint] ??= { heading: entry.heading, seconds: 0 });
      pending.seconds += entry.seconds;
    }
  });
}

async function load() {
  try {
    const response = await fetch(`${ENDPOINT}?page=${encodeURIComponent(route.path)}`);
    const stored: Record<string, Stored> = response.ok ? await response.json() : {};
    seconds.value = Object.fromEntries(Object.entries(stored).map(([fingerprint, entry]) => [fingerprint, entry.seconds]));
  } catch {
    seconds.value = {};
  }
}

function watchContent() {
  observer?.disconnect();
  const root = document.querySelector('.VPContent .vp-doc');
  if (!root) return;
  // Edits, hot reloads and previews change the text; rescan once they settle.
  observer = new MutationObserver(() => {
    clearTimeout(rescanTimer);
    rescanTimer = setTimeout(scan, 400);
  });
  observer.observe(root, { childList: true, subtree: true, characterData: true });
}

function start() {
  // The new page renders after the route changes.
  setTimeout(() => {
    scan();
    watchContent();
  }, 50);
  load();
}

/**
 * Reading needs the browser window itself focused: the window's own focus and blur events, which
 * fire when another app or window takes over, as well as document.hasFocus().
 */
let windowFocused = typeof document !== 'undefined' && document.hasFocus();
const isReading = () =>
  windowFocused && document.visibilityState === 'visible' && document.hasFocus() && Date.now() - lastActivity < IDLE_AFTER;
// Movement over an unfocused window is not reading.
const onActivity = () => {
  if (windowFocused) lastActivity = Date.now();
};
const onFocus = () => {
  windowFocused = true;
  lastActivity = Date.now();
  reading.value = isReading();
};
const onBlur = () => {
  windowFocused = false;
  reading.value = false;
};
const onHide = () => flush(true);
const onVisibility = () => document.visibilityState === 'hidden' && flush(true);

// ---- The minimap

const MAP_HEIGHT = 220;
const progress = (section: Section) => Math.min(1, (seconds.value[section.fingerprint] ?? 0) / section.full);
/** White when unread, darkening to black once proofread. */
const colour = (p: number) => `color-mix(in oklab, #000 ${Math.round(p * 100)}%, #fff)`;
// Bar heights come from each section's proofreading time, not from the page layout, so they hold
// still while the page scrolls or reflows and change only when a section's text does.
const bars = computed(() => {
  const total = sections.value.reduce((sum, section) => sum + section.full, 0) || 1;
  return sections.value.map((section) => ({
    section,
    p: progress(section),
    height: Math.max(3, (section.full / total) * MAP_HEIGHT),
  }));
});
const mapHeight = computed(() => bars.value.reduce((sum, bar) => sum + bar.height, 0));
/** Where a page position falls on the map: through its section's bar, in proportion. */
function mapAt(y: number) {
  let offset = 0;
  for (const bar of bars.value) {
    const { top, bottom } = bar.section;
    if (y <= top) return offset;
    if (y < bottom) return offset + ((y - top) / Math.max(1, bottom - top)) * bar.height;
    offset += bar.height;
  }
  return offset;
}
const frame = computed(() => {
  const top = mapAt(view.value.top);
  const bottom = mapAt(view.value.bottom);
  return { top: `${top}px`, height: `${Math.max(2, bottom - top)}px`, display: bottom > top ? 'block' : 'none' };
});
const overall = computed(() => {
  const total = sections.value.reduce((sum, section) => sum + section.full, 0);
  if (!total) return 0;
  const read = sections.value.reduce((sum, section) => sum + progress(section) * section.full, 0);
  return Math.round((read / total) * 100);
});
const minutes = (value: number) => {
  const whole = Math.floor(value);
  return whole < 60 ? `${whole}s` : `${Math.floor(whole / 60)}m ${String(whole % 60).padStart(2, '0')}s`;
};
const tip = computed(() => {
  if (hovered.value === null) return '';
  const section = sections.value[hovered.value];
  return section ? `${section.heading} · ${minutes(seconds.value[section.fingerprint] ?? 0)} of ${minutes(section.full)}` : '';
});

function jump(section: Section) {
  window.scrollTo({ top: Math.max(0, section.top - 72), behavior: 'smooth' });
}

onMounted(() => {
  for (const event of ['mousemove', 'keydown', 'scroll', 'wheel', 'touchstart', 'mousedown']) {
    window.addEventListener(event, onActivity, { passive: true });
  }
  window.addEventListener('pagehide', onHide);
  window.addEventListener('focus', onFocus);
  window.addEventListener('blur', onBlur);
  document.addEventListener('visibilitychange', onVisibility);
  window.addEventListener('resize', measure);
  window.addEventListener('scroll', measure, { passive: true });
  tickTimer = setInterval(tick, 1000);
  flushTimer = setInterval(() => flush(), FLUSH_EVERY);
  start();
});
onBeforeUnmount(() => {
  for (const event of ['mousemove', 'keydown', 'scroll', 'wheel', 'touchstart', 'mousedown']) {
    window.removeEventListener(event, onActivity);
  }
  window.removeEventListener('pagehide', onHide);
  window.removeEventListener('focus', onFocus);
  window.removeEventListener('blur', onBlur);
  document.removeEventListener('visibilitychange', onVisibility);
  window.removeEventListener('resize', measure);
  window.removeEventListener('scroll', measure);
  clearInterval(tickTimer);
  clearInterval(flushTimer);
  clearTimeout(rescanTimer);
  observer?.disconnect();
  flush(true);
});
watch(
  () => route.path,
  (_path, previous) => {
    // Time read on the page being left belongs to it.
    if (Object.keys(unsent).length) {
      const add = unsent;
      unsent = {};
      fetch(ENDPOINT, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ page: previous, add }) }).catch(() => {});
    }
    start();
  },
);
</script>

<template>
  <nav v-if="sections.length" class="reading-map" :class="{ 'reading-paused': !reading }" aria-label="Reading progress by section">
    <p class="reading-total">{{ overall }}% read{{ reading ? '' : ' · paused' }}</p>
    <div class="reading-bars" :style="{ height: `${mapHeight}px` }" @mouseleave="hovered = null">
      <button
        v-for="(bar, index) in bars"
        :key="bar.section.fingerprint + index"
        type="button"
        class="reading-bar"
        :style="{ height: `${bar.height}px`, background: colour(bar.p) }"
        :aria-label="`${bar.section.heading}: ${Math.round(bar.p * 100)}% read`"
        @mouseenter="hovered = index"
        @click="jump(bar.section)"
      />
      <span class="reading-frame" :style="frame" aria-hidden="true" />
    </div>
    <p v-if="tip" class="reading-tip" role="status">{{ tip }}</p>
  </nav>
</template>

<style scoped>
.reading-map {
  position: fixed;
  right: 16px;
  bottom: 64px;
  z-index: 190;
  display: grid;
  justify-items: end;
  gap: 6px;
  font-family: var(--vp-font-family-mono);
}
.reading-paused .reading-bars {
  opacity: 0.55;
}
.reading-total {
  margin: 0;
  font-size: 10px;
  color: var(--vp-c-text-2);
}
.reading-bars {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 44px;
  padding: 0;
  background: var(--vp-c-bg-soft);
  outline: 1px solid var(--vp-c-divider);
}
.reading-bar {
  display: block;
  width: 100%;
  border-bottom: 1px solid #8c8c86;
  cursor: pointer;
  transition: background 1s linear;
}
.reading-bar:hover {
  filter: brightness(1.15);
}
.reading-frame {
  position: absolute;
  left: -3px;
  right: -3px;
  border: 1.5px solid var(--signal);
  pointer-events: none;
}
.reading-tip {
  position: absolute;
  right: 54px;
  bottom: 0;
  margin: 0;
  padding: 5px 8px;
  max-width: 420px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  background: var(--vp-c-text-1);
  color: var(--vp-c-bg);
  font-size: 11px;
}
@media (max-width: 768px) {
  .reading-map {
    display: none;
  }
}
</style>
