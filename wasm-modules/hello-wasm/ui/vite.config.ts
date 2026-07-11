import { federation } from '@module-federation/vite';
import react from '@vitejs/plugin-react';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';

// The Hello WASM module's frontend: a Module Federation remote built to static
// files (remoteEntry.js + chunks) the host fetches at runtime. `base` matches
// where the Rust server serves it -- `/modules/<id>/` (from the install dir's
// fe/) -- so asset URLs resolve same-origin. The MF `name` is the module id
// sanitized to a valid identifier (matches clients/web/src/modules/remotes.ts).
export default defineConfig({
  base: '/modules/dev.luma.hellowasm/',
  plugins: [
    react(),
    federation({
      name: 'dev_luma_hellowasm',
      filename: 'remoteEntry.js',
      exposes: { './module': './src/module.tsx' },
      shared: {
        react: { singleton: true, requiredVersion: '^19' },
        'react-dom': { singleton: true, requiredVersion: '^19' },
      },
    }),
  ],
  resolve: {
    alias: {
      // The shared contract, consumed as source (types erased; React is a shared
      // singleton supplied by the host).
      '@luma/module-sdk': fileURLToPath(
        new URL('../../../packages/module-sdk/src/index.ts', import.meta.url),
      ),
    },
  },
  // chrome89: the MF runtime needs modern JS; this tier is web + desktop only.
  build: { target: 'chrome89', minify: false, cssCodeSplit: false, outDir: 'dist' },
});
