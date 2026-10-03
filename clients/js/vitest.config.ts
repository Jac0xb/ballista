import { fileURLToPath } from 'node:url';

import { defineConfig } from 'vitest/config';

// The examples import the SDK by its package name, as a reader's code would. Inside this package
// that name resolves to the source, as `paths` in tsconfig.json does for tsc and tsx.
export default defineConfig({
  resolve: {
    alias: [
      { find: /^@jac0xb\/ballista\/kit$/, replacement: fileURLToPath(new URL('./src/kit.ts', import.meta.url)) },
      { find: /^@jac0xb\/ballista$/, replacement: fileURLToPath(new URL('./src/index.ts', import.meta.url)) },
    ],
  },
});
