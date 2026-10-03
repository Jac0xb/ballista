// Code groups labelled "<Language> · <Part>" (e.g. "TypeScript · Template", "Rust · Run") get tabs
// for the parts and a language dropdown. The reader's language is remembered and applied to every
// such group, on this page and later ones. VitePress's own markup is kept: its blocks are shown and
// hidden and its radio inputs kept checked, so copy buttons and line numbers work as before.
// Groups whose labels do not all follow the convention are left alone.

type Lang = 'typescript' | 'rust';
type Part = 'template' | 'run';

const STORAGE_KEY = 'ballista:code-lang';
const LANG_NAMES: Record<Lang, string> = { typescript: 'TypeScript', rust: 'Rust' };
const PART_NAMES: Record<Part, string> = { template: 'Template', run: 'Run' };
const LANG_ORDER: Lang[] = ['typescript', 'rust'];
const PART_ORDER: Part[] = ['template', 'run'];

const LANG_WORDS: Record<string, Lang> = { typescript: 'typescript', ts: 'typescript', rust: 'rust', rs: 'rust' };
const PART_WORDS: Record<string, Part> = { template: 'template', write: 'template', run: 'run' };

interface Entry {
  lang: Lang;
  part: Part;
  input: HTMLInputElement;
  block: HTMLElement;
}
interface Group {
  root: HTMLElement;
  entries: Entry[];
  parts: Part[];
  part: Part;
  tabs: HTMLButtonElement[];
  select: HTMLSelectElement;
  note: HTMLElement;
}

/** "TypeScript · write it", "Run · Rust", "rust · template" → { lang, part }; anything else → null. */
export function parseLabel(label: string): { lang: Lang; part: Part } | null {
  const pieces = label.split('·').map((piece) => piece.trim().toLowerCase().replace(/\s+it$/, ''));
  if (pieces.length !== 2) return null;
  const [a, b] = pieces;
  if (LANG_WORDS[a] && PART_WORDS[b]) return { lang: LANG_WORDS[a], part: PART_WORDS[b] };
  if (LANG_WORDS[b] && PART_WORDS[a]) return { lang: LANG_WORDS[b], part: PART_WORDS[a] };
  return null;
}

function readLang(): Lang {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === 'typescript' || stored === 'rust') return stored;
  } catch {
    // Storage can be blocked; TypeScript is the default.
  }
  return 'typescript';
}
function writeLang(lang: Lang) {
  try {
    localStorage.setItem(STORAGE_KEY, lang);
  } catch {
    // Still applies to this page; it just won't be remembered.
  }
}

const groups = new Set<Group>();
let uid = 0;

function langsFor(group: Group, part: Part) {
  return LANG_ORDER.filter((lang) => group.entries.some((entry) => entry.part === part && entry.lang === lang));
}

function render(group: Group, lang: Lang) {
  const available = langsFor(group, group.part);
  const shown = available.includes(lang) ? lang : available[0];
  const entry = group.entries.find((item) => item.part === group.part && item.lang === shown)!;
  // The same state VitePress keeps: the matching radio checked and its block active.
  entry.input.checked = true;
  for (const block of group.root.querySelectorAll<HTMLElement>(':scope > .blocks > *')) block.classList.toggle('active', block === entry.block);
  group.tabs.forEach((tab, index) => {
    const selected = group.parts[index] === group.part;
    tab.setAttribute('aria-selected', String(selected));
    tab.tabIndex = selected ? 0 : -1;
  });
  group.select.value = shown;
  for (const option of group.select.options) {
    const has = available.includes(option.value as Lang);
    option.disabled = !has;
    option.textContent = LANG_NAMES[option.value as Lang] + (has ? '' : ' (n/a)');
  }
  const fallback = shown !== lang;
  group.root.toggleAttribute('data-lang-fallback', fallback);
  group.note.textContent = fallback ? `${LANG_NAMES[shown]} only` : '';
  group.note.hidden = !fallback;
}

function enhance(root: HTMLElement): Group | null {
  const tabsEl = root.querySelector<HTMLElement>(':scope > .tabs');
  const blocksEl = root.querySelector<HTMLElement>(':scope > .blocks');
  if (!tabsEl || !blocksEl) return null;
  const inputs = [...tabsEl.querySelectorAll<HTMLInputElement>(':scope > input')];
  const blocks = [...blocksEl.children] as HTMLElement[];
  if (inputs.length < 2 || inputs.length !== blocks.length) return null;

  const entries: Entry[] = [];
  for (const [index, input] of inputs.entries()) {
    const label = tabsEl.querySelector<HTMLLabelElement>(`label[for="${CSS.escape(input.id)}"]`);
    const parsed = parseLabel(label?.dataset.title ?? label?.textContent ?? '');
    if (!parsed) return null;
    if (entries.some((entry) => entry.lang === parsed.lang && entry.part === parsed.part)) return null;
    entries.push({ ...parsed, input, block: blocks[index] });
  }
  const parts = PART_ORDER.filter((part) => entries.some((entry) => entry.part === part));

  const id = `cg-${++uid}`;
  const bar = document.createElement('div');
  bar.className = 'cg-bar';
  const tablist = document.createElement('div');
  tablist.className = 'cg-parts';
  tablist.setAttribute('role', 'tablist');
  tablist.setAttribute('aria-label', 'Code');
  blocksEl.id ||= `${id}-panel`;
  blocksEl.setAttribute('role', 'tabpanel');

  const tabs = parts.map((part) => {
    const tab = document.createElement('button');
    tab.type = 'button';
    tab.className = 'cg-part';
    tab.id = `${id}-${part}`;
    tab.setAttribute('role', 'tab');
    tab.setAttribute('aria-controls', blocksEl.id);
    tab.textContent = PART_NAMES[part];
    tablist.append(tab);
    return tab;
  });

  const picker = document.createElement('label');
  picker.className = 'cg-lang';
  const pickerText = document.createElement('span');
  pickerText.className = 'cg-visually-hidden';
  pickerText.textContent = 'Language';
  const select = document.createElement('select');
  for (const lang of LANG_ORDER) select.add(new Option(LANG_NAMES[lang], lang));
  const note = document.createElement('span');
  note.className = 'cg-note';
  note.hidden = true;
  picker.append(pickerText, select);
  bar.append(tablist, note, picker);

  tabsEl.classList.add('cg-hidden');
  root.insertBefore(bar, tabsEl);
  root.classList.add('cg-enhanced');

  const group: Group = { root, entries, parts, part: parts[0], tabs, select, note };
  const choosePart = (index: number, focus = false) => {
    group.part = parts[index];
    render(group, readLang());
    if (focus) tabs[index].focus();
  };
  tabs.forEach((tab, index) => {
    tab.addEventListener('click', () => choosePart(index));
    tab.addEventListener('keydown', (event) => {
      const step = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
      if (step) {
        event.preventDefault();
        choosePart((index + step + parts.length) % parts.length, true);
      } else if (event.key === 'Home' || event.key === 'End') {
        event.preventDefault();
        choosePart(event.key === 'Home' ? 0 : parts.length - 1, true);
      }
    });
  });
  select.addEventListener('change', () => setCodeLang(select.value as Lang));
  select.setAttribute('aria-describedby', `${id}-note`);
  note.id = `${id}-note`;
  return group;
}

/** Sets the reader's language and applies it to every enhanced group on the page. */
export function setCodeLang(lang: Lang) {
  writeLang(lang);
  syncAll(lang);
}

function syncAll(lang = readLang()) {
  for (const group of groups) {
    if (!group.root.isConnected) groups.delete(group);
    else render(group, lang);
  }
}

/** Finds new code groups, enhances the ones that follow the convention, and re-applies the language. */
export function enhanceCodeGroups() {
  if (typeof document === 'undefined') return;
  for (const root of document.querySelectorAll<HTMLElement>('.vp-code-group')) {
    if (root.dataset.cgChecked) continue;
    root.dataset.cgChecked = 'true';
    const group = enhance(root);
    if (group) groups.add(group);
  }
  syncAll();
}

let listening = false;
/** Keeps groups in step with changes made in other tabs. Call once, in the browser. */
export function listenForCodeLang() {
  if (listening || typeof window === 'undefined') return;
  listening = true;
  window.addEventListener('storage', (event) => {
    if (event.key === STORAGE_KEY) syncAll();
  });
}
