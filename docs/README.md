# Ballista documentation

The documentation site uses [VitePress](https://vitepress.dev/), with no Next.js, React, Markdoc,
Tailwind, or Headless UI dependency. Code groups show each example in TypeScript and Rust.

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

`docs/.vitepress/theme/FieldManual.vue` draws the front page: the cover, with the pre-release status
line under its links; the example gallery (`TemplateGallery.vue`), whose drawings replay each case
without sending a transaction; and the three steps of how it works. The same status line opens Why
Ballista, Getting started and both READMEs, so change them together. Shared styling lives in
`docs/.vitepress/theme/style.css`. Native VitePress code groups keep the TypeScript and Rust tabs,
copy controls, and syntax highlighting.
