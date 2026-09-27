import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
// Type-only: VitePress bundles this file as CommonJS, which cannot require the ESM-only package.
import type { LoaderModule } from 'vitepress';

/** Facts the front page quotes, read from the repository at build time so they cannot go stale. */
export interface Facts {
  /** How many examples the cookbook has. */
  examples: number;
}

declare const data: Facts;
export { data };

const benchmarksPath = '../../../fixtures/benchmarks.json';

const loader: LoaderModule = {
  watch: [benchmarksPath],
  load(): Facts {
    const benchmarks = JSON.parse(readFileSync(fileURLToPath(new URL(benchmarksPath, import.meta.url)), 'utf8'));
    return { examples: Object.keys(benchmarks).length };
  },
};

export default loader;
