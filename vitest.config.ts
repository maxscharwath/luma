import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';
import { WEB_EXTENSIONS } from './clients/tv-build/rnw';

const dir = (p: string) => fileURLToPath(new URL(p, import.meta.url));

// Pure-logic unit tests run in the `node` environment (no DOM). The `#tv`/`#web`
// subpath aliases (from tsconfig paths) are resolved here so source files that
// use them are importable under vitest.
export default defineConfig({
  resolve: {
    alias: [
      { find: /^#tv\//, replacement: dir('./packages/tv/src/') },
      { find: /^#web\//, replacement: dir('./clients/web/src/') },
      // @kroma/ui is written against React Native. Under the test runner (as
      // in every browser target) that resolves to react-native-web, exactly the
      // way the Tizen / webOS / desktop bundles wire it.
      { find: /^react-native$/, replacement: 'react-native-web' },
    ],
    // `.web.*` wins over the plain file, so the kit's web focus engine and web
    // focus transition are what the DOM tests exercise. This mirrors the shells'
    // Vite config; Metro applies the opposite precedence for the native apps.
    extensions: WEB_EXTENSIONS,
    // bun installs per workspace, so a renderer test can otherwise end up with
    // @testing-library's React and the component's React being two different
    // physical copies ("Invalid hook call"). Collapse them onto the root install.
    dedupe: ['react', 'react-dom', 'react-native-web'],
  },
  test: {
    environment: 'node',
    include: [
      'packages/*/src/**/*.test.ts',
      'packages/*/src/**/*.test.tsx',
      'packages/*/worker/**/*.test.ts',
      'clients/web/src/**/*.test.ts',
      'clients/web/src/**/*.test.tsx',
      'clients/desktop/src/**/*.test.ts',
    ],
    // Inline zod so Vite resolves it (via the `import` condition -> built
    // index.js) instead of Bun externalizing it and matching zod's `@zod/source`
    // condition -> raw TS source, whose `z` export is undefined under the runner.
    // react-native-web ships CommonJS; inlining it lets Vite interop it too.
    // Every React Native package MUST be inlined, not externalised: an
    // externalised dep is loaded by Node directly, which bypasses the
    // `react-native` -> `react-native-web` alias and lands on React Native's
    // Flow source ("SyntaxError: Unexpected token 'typeof'").
    server: {
      deps: { inline: ['zod', /react-native/, /@tabler\/icons-react-native/] },
    },
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
