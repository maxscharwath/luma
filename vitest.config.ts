import { defineConfig } from 'vitest/config';

// Pure-logic unit tests: the shared core (engine selection, audio-track
// resolution, master-variant + URL builders) and the TV engine's native AVPlay
// audio mapping. No DOM is needed, but jsdom is available for future component
// tests. Test files must use relative imports (no `#tv`/`#web` path aliases).
export default defineConfig({
  test: {
    environment: 'node',
    include: [
      'packages/client/src/**/*.test.ts',
      'packages/core/src/**/*.test.ts',
      'packages/tv/src/**/*.test.ts',
    ],
  },
});
