# Ballista documentation

The documentation site uses [VitePress](https://vitepress.dev/), with no Next.js, React, Markdoc,
Tailwind, or Headless UI dependency. Code groups pair TypeScript template authoring with Rust
artifact upload and execution examples.

From the repository root:

```bash
pnpm install
pnpm docs:dev
```

Build the same static output used by GitHub Pages with `pnpm docs:build`. The generated site is
written to `docs/.vitepress/dist` and is intentionally ignored by Git.

## Visual system

The site is typeset as a technical field manual: a paper background, serif headings, monospace
annotations, ruled tables, and one vermilion signal color. The same system covers the landing page,
navigation, search, and reference pages. Fonts use local system stacks.

`docs/.vitepress/theme/FieldManual.vue` contains the cover and interactive ATA control-flow drawing.
Its two states illustrate guarded ordinary creation; switching states does not send a transaction.
Shared styling lives in `docs/.vitepress/theme/style.css`. Native VitePress code groups retain the
TypeScript/Rust tabs, copy controls, and syntax highlighting.
