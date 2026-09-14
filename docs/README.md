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
