import { fileURLToPath } from 'node:url';
import { kromaModule } from '@kroma/module-sdk/vite';
import babel from '@rolldown/plugin-babel';
import tailwindcss from '@tailwindcss/vite';
import { tanstackStart } from '@tanstack/react-start/plugin/vite';
import react, { reactCompilerPreset } from '@vitejs/plugin-react';
import { defineConfig, type Plugin } from 'vite';
import { buildInfoPlugin } from './build-info';

// `vite build` used to sit idle for exactly 5 minutes after the prerender: the
// TanStack Start shell-prerender's render server leaks a handle that only Node's
// default 300s http requestTimeout releases. Everything is on disk once the
// start plugin's own buildApp hook (build + prerender) resolves, so this hook -
// which vite runs sequentially AFTER it - can end the process. Exit code 0 keeps
// the `vite build && precompress` chain working; a failed build never gets here.
const exitAfterBuild = (): Plugin => ({
  name: 'kroma:exit-after-build',
  // Must sort AFTER "tanstack-start-core:post-build" (enforce post + hook order
  // post), which is what runs the prerender; without `enforce` this hook fired
  // first and the build skipped the shell prerender entirely.
  enforce: 'post',
  buildApp: {
    order: 'post',
    async handler() {
      setImmediate(() => process.exit(0));
    },
  },
});

const repoRoot = fileURLToPath(new URL('../..', import.meta.url));

// In dev the Rust API runs on its own port; Vite reverse-proxies `/api` to it so
// the whole app lives on a single origin (`:3000`) same-origin as prod, no CORS,
// one port to open. Override the target with KROMA_SERVER_URL if the server moved.
const apiTarget = process.env.KROMA_SERVER_URL ?? 'http://localhost:4040';

export default defineConfig({
  // Tailwind v4 + TanStack Start in SPA mode + React. The build prerenders only an
  // app shell (index.html) and the client renders/loads at runtime so the whole
  // app ships as static files the Rust server serves on the same origin (the
  // single-binary Synology package). No Node runtime needed in production.
  plugins: [
    // Injects each module's manifest + locales into its defineModule() call by
    // convention (must precede the transforms that expand import.meta.glob).
    kromaModule(),
    // Exposes `virtual:build-info` (version, commit, branch, build date).
    buildInfoPlugin(),
    tailwindcss(),
    tanstackStart({ spa: { enabled: true } }),
    react(),
    // React Compiler auto-memoizes components/hooks (React 19 default target →
    // uses React's built-in `react/compiler-runtime`, no extra runtime package).
    // Since plugin-react v6 dropped its built-in Babel pass for an Oxc transform,
    // the compiler runs as a separate Rolldown/Babel preset, which also compiles
    // the aliased @kroma/ui / @kroma/core source.
    babel({ presets: [reactCompilerPreset()] }),
    exitAfterBuild(),
  ],
  resolve: {
    // `#web/*` → this app's src (mirrors tsconfig.base paths; Vite needs it explicitly).
    alias: { '#web': fileURLToPath(new URL('./src', import.meta.url)) },
    // One React copy: the other clients stay on their own React, so pin this
    // bundle to a single react/react-dom (guards against "Invalid hook call").
    dedupe: ['react', 'react-dom'],
  },
  server: {
    // Allow importing TS source from the workspace packages (@kroma/ui, @kroma/core).
    fs: { allow: [repoRoot] },
    // Single-port dev: forward `/api/*` (REST + posters + streams + HLS) and the
    // `/api/events` WebSocket (`ws: true`) to the Rust server. The web client is
    // same-origin in dev (see `apiBase()`), so every request rides this proxy.
    proxy: {
      '/api': { target: apiTarget, changeOrigin: true, ws: true },
    },
  },
  // Workspace packages ship raw TS source bundle them for SSR (don't externalize).
  ssr: { noExternal: ['@kroma/ui', '@kroma/core'] },
  optimizeDeps: { exclude: ['@kroma/ui', '@kroma/core'] },
});
