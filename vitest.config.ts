import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

const dir = (p: string) => fileURLToPath(new URL(p, import.meta.url));

// Pure-logic unit tests run in the `node` environment (no DOM). The `#tv`/`#web`
// subpath aliases (from tsconfig paths) are resolved here so source files that
// use them are importable under vitest.
export default defineConfig({
  resolve: {
    alias: [
      { find: /^#tv\//, replacement: dir('./packages/tv/src/') },
      { find: /^#web\//, replacement: dir('./clients/web/src/') },
    ],
  },
  test: {
    environment: 'node',
    include: [
      'packages/*/src/**/*.test.ts',
      'packages/*/worker/**/*.test.ts',
      'clients/web/src/**/*.test.ts',
      'clients/desktop/src/**/*.test.ts',
    ],
    // Inline zod so Vite resolves it (via the `import` condition -> built
    // index.js) instead of Bun externalizing it and matching zod's `@zod/source`
    // condition -> raw TS source, whose `z` export is undefined under the runner.
    server: { deps: { inline: ['zod'] } },
    coverage: {
      // istanbul (source-instrumented) works under Bun's runtime; the v8
      // provider needs node:inspector coverage APIs Bun doesn't implement.
      // Emits lcov for SonarCloud (coverage/lcov.info) + a text summary in CI.
      // Scope/exclusions live in sonar-project.properties.
      provider: 'istanbul',
      reporter: ['text', 'lcov'],
      reportsDirectory: './coverage',
    },
  },
});
