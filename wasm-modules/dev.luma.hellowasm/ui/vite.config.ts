import { fileURLToPath } from 'node:url';
import { federation } from '@module-federation/vite';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
// Consumed by relative path (like the @luma/module-sdk source alias below): this
// remote build isn't a package dependent of @luma/module-sdk.
import { lumaModule } from '../../../packages/module-sdk/vite';

// The Hello WASM module's frontend: a Module Federation remote built to static
// files (remoteEntry.js + chunks) the host fetches at runtime. `base` matches
// where the Rust server serves it -- `/modules/<id>/` (from the install dir's
// fe/) -- so asset URLs resolve same-origin. The MF `name` is the module id
// sanitized to a valid identifier (matches clients/web/src/modules/remotes.ts).
//
// The module builds its OWN Tailwind CSS (the shared @luma/ui/tailwind.css entry
// imported by src/styles.css): Tailwind scans THIS module's src, so the remote
// carries a self-contained stylesheet. It is emitted at the fixed name `style.css`
// (no hash), so the host loads `/modules/<id>/style.css` when it installs the
// remote (see clients/web/src/modules/remotes.ts).
export default defineConfig({
  base: '/modules/dev.luma.hellowasm/',
  plugins: [
    // Injects this module's manifest + locales into its defineModule() call.
    lumaModule(),
    tailwindcss(),
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
  build: {
    target: 'chrome89',
    minify: false,
    cssCodeSplit: false,
    outDir: 'dist',
    rollupOptions: {
      output: {
        // Fixed name for the single stylesheet so the host can `<link>` it.
        assetFileNames: (info) =>
          info.name?.endsWith('.css') ? 'style.css' : 'assets/[name]-[hash][extname]',
      },
    },
  },
});
