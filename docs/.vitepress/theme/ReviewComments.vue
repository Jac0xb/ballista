<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useData, useRoute } from 'vitepress';
import ReviewVariations from './ReviewVariations.vue';

// Highlight text on any page of the dev server, write a comment, and it is appended to
// .local/review-comments.jsonl through the endpoint in ../review-comments.ts. Variations written
// in answer to a comment are previewed by ReviewVariations.

interface ReviewComment {
  id: string;
  createdAt: string;
  kind?: 'comment' | 'edit';
  page: string;
  file?: string;
  heading: string;
  selection: string;
  comment: string;
  original?: string;
  edited?: string;
  appliedAt?: string;
}
interface Draft {
  selection: string;
  selector: string;
  offsets: [number, number];
  before: string;
  after: string;
  heading: string;
  top: number;
  left: number;
  block: Element;
  editable: boolean;
}
interface Editing {
  element: HTMLElement;
  selector: string;
  heading: string;
  original: string;
  originalHtml: string;
  /** Copies of the element's children, put back on Cancel. */
  saved: Node[];
}

const ENDPOINT = '/__review/comments';
const route = useRoute();
const { page } = useData();
const pending = ref<Draft | null>(null);
const draft = ref<Draft | null>(null);
const text = ref('');
const comments = ref<ReviewComment[]>([]);
const panelOpen = ref(false);
const notice = ref('');
const saving = ref(false);
const field = ref<HTMLTextAreaElement>();
const editing = ref<Editing | null>(null);
const editBar = ref<{ top: number; left: number } | null>(null);
/** The comment just sent, marked where it was made until Claude's options for it arrive. */
const awaiting = ref<{ id: string; pageTop: number; left: number } | null>(null);
const awaitingAt = ref<{ top: number; left: number } | null>(null);
let awaitingTimer: ReturnType<typeof setTimeout> | undefined;
/** Edits already known to be in the source, so each "applied" notice shows once. */
const appliedEdits = new Set<string>();
let firstLoad = true;
let noticeTimer: ReturnType<typeof setTimeout> | undefined;

/** Blocks whose text can be edited in place. Buttons and drawings stay comment-only. */
const EDITABLE = 'p, li, h1, h2, h3, h4, h5, h6, td, th, blockquote, figcaption, pre, dd, dt';

// VitePress puts zero-width spaces in heading anchors; they are noise in saved text.
const squash = (value: string) => value.replace(/[\u200b-\u200d\ufeff]/g, '').replace(/\s+/g, ' ').trim();
const clamp = (value: number, min: number, max: number) => Math.min(Math.max(value, min), Math.max(min, max));
/** The text an element shows, line breaks kept, without VitePress's zero-width heading anchors. */
const shownText = (element: HTMLElement) =>
  (element.innerText ?? element.textContent ?? '').replace(/[\u200b-\u200d\ufeff]/g, '').replace(/[ \t]+\n/g, '\n').trim();

/** The nearest block around a node, so the saved context is the sentence or line it sits in. */
function blockOf(node: Node): Element {
  const element = node.nodeType === Node.ELEMENT_NODE ? (node as Element) : node.parentElement;
  return (
    element?.closest('p, li, pre, td, th, h1, h2, h3, h4, blockquote, figcaption, button, svg, .plate-readout') ??
    element ??
    document.body
  );
}

/**
 * A CSS selector that matches exactly this element: the path of tags down from the nearest ancestor
 * with an id, each disambiguated with :nth-of-type. Ids go through CSS.escape, so any id stays a
 * valid selector, and the whole string is stored through JSON.stringify.
 */
function selectorFor(element: Element): string {
  const path = (anchorOnId: boolean) => {
    const parts: string[] = [];
    for (let node: Element | null = element; node && node !== document.documentElement; node = node.parentElement) {
      if (anchorOnId && node.id) {
        parts.unshift(`#${CSS.escape(node.id)}`);
        break;
      }
      const tag = node.tagName.toLowerCase();
      const siblings = node.parentElement ? Array.from(node.parentElement.children).filter((child) => child.tagName === node!.tagName) : [];
      parts.unshift(siblings.length > 1 ? `${tag}:nth-of-type(${siblings.indexOf(node) + 1})` : tag);
    }
    return parts.join(' > ');
  };
  const anchored = path(true);
  try {
    if (document.querySelectorAll(anchored).length === 1) return anchored;
  } catch {
    // Fall through to the path from the document root.
  }
  return path(false);
}

/** The last heading before the selection, which says where on the page it is. */
function headingBefore(node: Node): string {
  let heading = '';
  for (const candidate of document.querySelectorAll('.VPContent h1, .VPContent h2, .VPContent h3')) {
    if (candidate.compareDocumentPosition(node) & Node.DOCUMENT_POSITION_FOLLOWING) {
      // innerText keeps the line breaks the page shows, so "Execution,<em>composed.</em>" reads as two words.
      heading = squash((candidate as HTMLElement).innerText ?? candidate.textContent ?? '').replace(/[\u200b#\s]+$/u, '');
    } else {
      break;
    }
  }
  return heading;
}

function snapshot(): Draft | null {
  const selection = window.getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount === 0) return null;
  const selected = squash(selection.toString());
  if (!selected) return null;
  const range = selection.getRangeAt(0);
  const container = range.commonAncestorContainer;
  const element = container.nodeType === Node.ELEMENT_NODE ? (container as Element) : container.parentElement;
  if (element?.closest('.review-ui')) return null;
  const block = blockOf(container);
  const before = document.createRange();
  before.selectNodeContents(block);
  before.setEnd(range.startContainer, range.startOffset);
  const after = document.createRange();
  after.selectNodeContents(block);
  after.setStart(range.endContainer, range.endOffset);
  const rect = range.getBoundingClientRect();
  const start = before.toString().length;
  return {
    selection: selected,
    selector: selectorFor(block),
    offsets: [start, start + range.toString().length],
    before: squash(before.toString()).slice(-160),
    after: squash(after.toString()).slice(0, 160),
    heading: headingBefore(range.startContainer),
    top: rect.bottom,
    left: rect.left + rect.width / 2,
    block,
    editable: block.matches(EDITABLE) && !block.closest('svg, button, .review-ui'),
  };
}

function onSelectionEnd(event: Event) {
  if (editing.value || (event.target as Element | null)?.closest?.('.review-ui')) return;
  // The selection settles after the event that ended it.
  setTimeout(() => {
    pending.value = draft.value ? null : snapshot();
  }, 0);
}
function onKeyUp(event: KeyboardEvent) {
  if (event.shiftKey || event.key === 'Shift') onSelectionEnd(event);
}
function onScroll() {
  pending.value = null;
  if (editing.value) placeEditBar();
  if (awaiting.value) placeAwaiting();
}

function placeAwaiting() {
  if (!awaiting.value) return;
  awaitingAt.value = {
    top: clamp(awaiting.value.pageTop - window.scrollY + 8, 8, window.innerHeight - 40),
    left: clamp(awaiting.value.left - 70, 8, window.innerWidth - 180),
  };
}
function stopAwaiting() {
  awaiting.value = null;
  awaitingAt.value = null;
  clearTimeout(awaitingTimer);
}
function onVariations(event: Event) {
  const sets = (event as CustomEvent<{ commentId?: string }[]>).detail ?? [];
  if (awaiting.value && sets.some((set) => set.commentId === awaiting.value?.id)) stopAwaiting();
}

const triggerStyle = computed(() =>
  pending.value
    ? {
        top: `${clamp(pending.value.top + 8, 8, window.innerHeight - 44)}px`,
        left: `${clamp(pending.value.left - 76, 8, window.innerWidth - 176)}px`,
      }
    : {},
);
const popoverStyle = computed(() =>
  draft.value
    ? {
        top: `${clamp(draft.value.top + 10, 12, window.innerHeight - 300)}px`,
        left: `${clamp(draft.value.left - 180, 12, window.innerWidth - 372)}px`,
      }
    : {},
);

function say(message: string) {
  notice.value = message;
  clearTimeout(noticeTimer);
  noticeTimer = setTimeout(() => (notice.value = ''), 5000);
}

function open() {
  if (!pending.value) return;
  draft.value = pending.value;
  pending.value = null;
  text.value = '';
  nextTick(() => field.value?.focus());
}
function cancel() {
  draft.value = null;
  text.value = '';
}

// ---- Editing text in place

function placeEditBar() {
  const rect = editing.value?.element.getBoundingClientRect();
  if (!rect) return;
  editBar.value = {
    top: clamp(rect.top > 64 ? rect.top - 48 : rect.bottom + 10, 8, window.innerHeight - 52),
    left: clamp(rect.right - 360, 8, window.innerWidth - 368),
  };
}

function onEditKey(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault();
    cancelEdit();
  } else if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
    event.preventDefault();
    saveEdit();
  }
}

function startEdit() {
  const target = pending.value;
  if (!target?.editable) return;
  pending.value = null;
  const element = target.block as HTMLElement;
  // Keep the highlighted text selected, so typing replaces it.
  const selection = window.getSelection();
  const range = selection && selection.rangeCount ? selection.getRangeAt(0).cloneRange() : null;
  editing.value = {
    element,
    selector: target.selector,
    heading: target.heading,
    original: shownText(element),
    originalHtml: element.innerHTML,
    saved: Array.from(element.childNodes, (node) => node.cloneNode(true)),
  };
  element.setAttribute('contenteditable', 'plaintext-only');
  if (element.contentEditable !== 'plaintext-only') element.setAttribute('contenteditable', 'true');
  element.setAttribute('data-review-editing', '');
  element.addEventListener('keydown', onEditKey);
  element.focus({ preventScroll: true });
  if (range && selection) {
    selection.removeAllRanges();
    selection.addRange(range);
  }
  placeEditBar();
}

function stopEditing() {
  const current = editing.value;
  if (!current) return;
  current.element.removeAttribute('contenteditable');
  current.element.removeAttribute('data-review-editing');
  current.element.removeEventListener('keydown', onEditKey);
  editing.value = null;
  editBar.value = null;
}

function cancelEdit() {
  const current = editing.value;
  if (!current) return;
  current.element.replaceChildren(...current.saved);
  stopEditing();
}

async function saveEdit() {
  const current = editing.value;
  if (!current || saving.value) return;
  const edited = shownText(current.element);
  if (edited === current.original) {
    cancelEdit();
    return;
  }
  saving.value = true;
  try {
    const response = await fetch(ENDPOINT, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        kind: 'edit',
        page: route.path,
        file: page.value.relativePath,
        title: document.title,
        heading: current.heading,
        selection: current.original,
        selector: current.selector,
        original: current.original,
        edited,
        originalHtml: current.originalHtml,
      }),
    });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error ?? response.statusText);
    // The page keeps your text, marked, until the source says the same.
    current.element.setAttribute('data-review-edited', result.id);
    stopEditing();
    window.getSelection()?.removeAllRanges();
    say('Edit saved. Claude will make it in the source.');
    await load();
  } catch (error) {
    say(`Not saved: ${(error as Error).message}`);
  } finally {
    saving.value = false;
  }
}

const counts = computed(() => {
  const edits = comments.value.filter((item) => item.kind === 'edit').length;
  const notes = comments.value.length - edits;
  const plural = (count: number, word: string) => `${count} ${word}${count === 1 ? '' : 's'}`;
  return edits ? `${plural(notes, 'comment')} · ${plural(edits, 'edit')}` : `${plural(notes, 'comment')} on this page`;
});

async function load() {
  try {
    const response = await fetch(`${ENDPOINT}?page=${encodeURIComponent(route.path)}`);
    comments.value = response.ok ? await response.json() : [];
  } catch {
    comments.value = [];
  }
  // Once an edit is in the source, the page shows it for real; drop the mark.
  for (const item of comments.value) {
    if (item.kind === 'edit' && item.appliedAt) {
      document.querySelector(`[data-review-edited="${CSS.escape(item.id)}"]`)?.removeAttribute('data-review-edited');
      if (!appliedEdits.has(item.id)) {
        appliedEdits.add(item.id);
        if (!firstLoad) say('✓ Claude applied your edit.');
      }
    }
  }
  firstLoad = false;
}

async function save() {
  if (!draft.value || !text.value.trim() || saving.value) return;
  // Mark the spot where Comment was pressed, in page coordinates so it scrolls with the text.
  const spot = { pageTop: draft.value.top + window.scrollY, left: draft.value.left };
  saving.value = true;
  try {
    const response = await fetch(ENDPOINT, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        page: route.path,
        file: page.value.relativePath,
        title: document.title,
        heading: draft.value.heading,
        selection: draft.value.selection,
        selector: draft.value.selector,
        offsets: draft.value.offsets,
        before: draft.value.before,
        after: draft.value.after,
        comment: text.value,
      }),
    });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error ?? response.statusText);
    draft.value = null;
    text.value = '';
    window.getSelection()?.removeAllRanges();
    say('Sent to Claude.');
    awaiting.value = { id: result.id, ...spot };
    placeAwaiting();
    clearTimeout(awaitingTimer);
    awaitingTimer = setTimeout(stopAwaiting, 10 * 60_000);
    await load();
  } catch (error) {
    say(`Not saved: ${(error as Error).message}`);
  } finally {
    saving.value = false;
  }
}

async function remove(id: string) {
  await fetch(`${ENDPOINT}?id=${encodeURIComponent(id)}`, { method: 'DELETE' });
  await load();
}

onMounted(() => {
  document.addEventListener('mouseup', onSelectionEnd);
  document.addEventListener('keyup', onKeyUp);
  window.addEventListener('scroll', onScroll, { passive: true });
  load();
  import.meta.hot?.on('review:changed', (data: { file?: string }) => {
    if (data?.file === 'comments') load();
  });
  window.addEventListener('review-variations', onVariations);
  window.addEventListener('resize', placeAwaiting);
});
onBeforeUnmount(() => {
  document.removeEventListener('mouseup', onSelectionEnd);
  document.removeEventListener('keyup', onKeyUp);
  window.removeEventListener('scroll', onScroll);
  window.removeEventListener('review-variations', onVariations);
  window.removeEventListener('resize', placeAwaiting);
  clearTimeout(noticeTimer);
  cancelEdit();
  stopAwaiting();
});
watch(
  () => route.path,
  () => {
    pending.value = null;
    draft.value = null;
    cancelEdit();
    stopAwaiting();
    load();
  },
);
</script>

<template>
  <div class="review-ui">
    <ReviewVariations />
    <div v-if="pending" class="review-trigger" :style="triggerStyle" @mousedown.prevent>
      <button type="button" @click="open">Comment</button>
      <button v-if="pending.editable" type="button" @click="startEdit">Edit text</button>
    </div>

    <div v-if="awaiting && awaitingAt" class="review-awaiting" :style="{ top: `${awaitingAt.top}px`, left: `${awaitingAt.left}px` }" role="status">
      <span class="review-pulse" aria-hidden="true" />
      Sent to Claude
      <button type="button" aria-label="Hide" @click="stopAwaiting">×</button>
    </div>

    <div v-if="editing && editBar" class="review-editbar" :style="{ top: `${editBar.top}px`, left: `${editBar.left}px` }" @mousedown.prevent>
      <span class="review-hint">Editing · ⌘/Ctrl + Enter saves · Esc cancels</span>
      <button type="button" class="review-secondary" @click="cancelEdit">Cancel</button>
      <button type="button" class="review-primary" :disabled="saving" @click="saveEdit">Save edit</button>
    </div>

    <form v-if="draft" class="review-popover" :style="popoverStyle" @submit.prevent="save" @keydown.esc.prevent="cancel">
      <p class="review-label">Commenting on{{ draft.heading ? ` · ${draft.heading}` : '' }}</p>
      <blockquote class="review-quote">{{ draft.selection }}</blockquote>
      <textarea
        ref="field"
        v-model="text"
        rows="4"
        placeholder="What should change?"
        @keydown.meta.enter.prevent="save"
        @keydown.ctrl.enter.prevent="save"
      />
      <div class="review-actions">
        <span class="review-hint">⌘/Ctrl + Enter saves · Esc cancels</span>
        <button type="button" class="review-secondary" @click="cancel">Cancel</button>
        <button type="submit" class="review-primary" :disabled="saving || !text.trim()">Save</button>
      </div>
    </form>

    <div class="review-dock">
      <p v-if="notice" class="review-notice" role="status">{{ notice }}</p>
      <button type="button" class="review-count" :aria-expanded="panelOpen" @click="panelOpen = !panelOpen">
        {{ counts }}
      </button>
    </div>

    <aside v-if="panelOpen" class="review-panel" aria-label="Comments on this page">
      <p v-if="!comments.length" class="review-empty">Highlight any text on the page, then press Comment.</p>
      <article v-for="item in comments" :key="item.id" class="review-item">
        <template v-if="item.kind === 'edit'">
          <p class="review-label">Edit{{ item.appliedAt ? ' · in the source' : ' · waiting for Claude' }}</p>
          <blockquote class="review-quote review-was">{{ item.original }}</blockquote>
          <blockquote class="review-quote review-now">{{ item.edited }}</blockquote>
        </template>
        <template v-else>
          <blockquote class="review-quote">{{ item.selection }}</blockquote>
          <p class="review-comment">{{ item.comment }}</p>
        </template>
        <p class="review-meta">
          <span>{{ item.heading || 'Top of the page' }} · {{ new Date(item.createdAt).toLocaleString() }}</span>
          <button type="button" @click="remove(item.id)">Delete</button>
        </p>
      </article>
    </aside>
  </div>
</template>

<style scoped>
.review-trigger,
.review-editbar,
.review-popover,
.review-dock,
.review-panel {
  position: fixed;
  z-index: 200;
  font-family: var(--vp-font-family-mono);
}
.review-trigger {
  display: flex;
  box-shadow: 0 4px 14px rgba(34, 35, 31, 0.2);
}
.review-trigger button {
  padding: 7px 12px;
  background: var(--ink);
  color: var(--paper);
  font-size: 11px;
  letter-spacing: 0.03em;
  cursor: pointer;
}
.review-trigger button + button {
  border-left: 1px solid #55564f;
}
.review-trigger button:hover {
  background: var(--signal);
}
.review-awaiting {
  position: fixed;
  z-index: 200;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px 6px 10px;
  background: var(--ink);
  color: var(--paper);
  font-family: var(--vp-font-family-mono);
  font-size: 11px;
  box-shadow: 0 4px 14px rgba(34, 35, 31, 0.2);
}
.review-awaiting button {
  color: #b9b8b0;
  cursor: pointer;
}
.review-pulse {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--signal);
  animation: review-pulse 1s ease-in-out infinite;
}
@keyframes review-pulse {
  50% {
    opacity: 0.25;
    transform: scale(0.7);
  }
}
.review-editbar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 8px 7px 12px;
  background: var(--paper);
  border: 1px solid var(--ink);
  box-shadow: 4px 4px 0 rgba(34, 35, 31, 0.12);
}
.review-was {
  text-decoration: line-through;
  text-decoration-color: #be3b2399;
  color: var(--muted);
}
.review-now {
  border-left-color: var(--ink);
}
:global([data-review-editing]) {
  outline: 2px solid var(--signal);
  outline-offset: 4px;
  background: #fffdf6;
  caret-color: var(--signal);
}
:global([data-review-edited]) {
  background: #be3b230d;
  box-shadow: inset 0 -2px 0 #be3b2366;
}
.review-popover {
  width: 360px;
  display: grid;
  gap: 10px;
  padding: 14px;
  background: var(--paper);
  border: 1px solid var(--ink);
  box-shadow: 0 10px 30px rgba(34, 35, 31, 0.18);
}
.review-label {
  margin: 0;
  font-size: 10px;
  color: var(--muted);
  text-transform: uppercase;
  letter-spacing: 0.06em;
}
.review-quote {
  margin: 0;
  max-height: 96px;
  overflow: auto;
  padding: 8px 10px;
  background: #eeede7;
  border-left: 2px solid var(--signal);
  font-size: 11px;
  line-height: 1.6;
  color: var(--ink);
}
.review-popover textarea {
  width: 100%;
  min-height: 92px;
  padding: 8px 10px;
  border: 1px solid var(--rule);
  background: #fbfaf7;
  font-family: var(--vp-font-family-base);
  font-size: 13px;
  line-height: 1.5;
  resize: vertical;
}
.review-popover textarea:focus {
  outline: 2px solid var(--signal);
  outline-offset: -1px;
}
.review-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}
.review-hint {
  margin-right: auto;
  font-size: 9px;
  color: var(--muted);
}
.review-primary,
.review-secondary {
  padding: 6px 12px;
  font-size: 11px;
  cursor: pointer;
}
.review-primary {
  background: var(--ink);
  color: var(--paper);
}
.review-primary:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
.review-secondary {
  border: 1px solid var(--rule);
}
.review-dock {
  right: 16px;
  bottom: 16px;
  display: grid;
  justify-items: end;
  gap: 8px;
}
.review-count {
  padding: 8px 12px;
  background: var(--paper);
  border: 1px solid var(--ink);
  font-size: 11px;
  cursor: pointer;
}
.review-count:hover {
  color: var(--signal);
  border-color: var(--signal);
}
.review-notice {
  margin: 0;
  padding: 7px 10px;
  background: var(--ink);
  color: var(--paper);
  font-size: 11px;
}
.review-panel {
  right: 16px;
  bottom: 60px;
  width: min(380px, calc(100vw - 32px));
  max-height: 60vh;
  overflow: auto;
  padding: 14px;
  background: var(--paper);
  border: 1px solid var(--ink);
  box-shadow: 0 10px 30px rgba(34, 35, 31, 0.18);
  display: grid;
  gap: 14px;
}
.review-empty {
  margin: 0;
  font-size: 11px;
  color: var(--muted);
}
.review-item {
  display: grid;
  gap: 6px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--rule);
}
.review-comment {
  margin: 0;
  font-family: var(--vp-font-family-base);
  font-size: 13px;
  line-height: 1.5;
  color: var(--ink);
}
.review-meta {
  margin: 0;
  display: flex;
  justify-content: space-between;
  gap: 10px;
  font-size: 9px;
  color: var(--muted);
}
.review-meta button {
  color: var(--signal);
  text-decoration: underline;
  cursor: pointer;
}
</style>
