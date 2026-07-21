import { fileURLToPath } from 'node:url';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig, type UserConfig } from 'vite';
import { clientVersion } from '../tv-build/shell';

const repoRoot = fileURLToPath(new URL('../..', import.meta.url));

// Unlike the Tizen / webOS shells, the Steam Deck runs a current desktop Chromium
// (SteamOS 3, Arch-based), so there is no old-webview floor: no Lightning CSS
// down-levelling and a modern build target. The shared @kroma/tv CSS (Tailwind v4
// cascade layers, color-mix, oklch) is emitted as-is and the browser handles it.
// Return type pinned to UserConfig so the config resolves against a single
// defineConfig overload (some tsgo builds otherwise report a spurious TS2769
// "no overload matches" on the function form across platforms).
export default defineConfig(
  ({ command }): UserConfig => ({
    plugins: [tailwindcss(), react()],
    // This build's version, for the server-compatibility banner (see @kroma/tv
    // CompatBanner / @kroma/core checkServerCompat).
    define: { __KROMA_VERSION__: JSON.stringify(clientVersion(repoRoot)) },
    // `#tv/*` -> the @kroma/tv package src (mirrors tsconfig.base paths; Vite needs it explicitly).
    resolve: { alias: { '#tv': fileURLToPath(new URL('../../packages/tv/src', import.meta.url)) } },
    // Loadable both from a served origin and directly via file:// in a kiosk, so keep
    // assets relative. The app talks to the KROMA server cross-origin either way (same
    // as the TV clients), via the in-app connect flow.
    base: './',
    server: {
      // Bind 0.0.0.0 so a Deck on the LAN can load the dev server and get HMR while
      // you iterate on a real device.
      host: true,
      // 5174 = tizen, 5175 = webos, 5178 = steamdeck (5176/5177 are commonly taken by
      // other local dev servers; keeping the Deck a couple ports clear avoids clashes).
      port: 5178,
      fs: { allow: [repoRoot] },
    },
    optimizeDeps: { exclude: ['@kroma/ui', '@kroma/core', '@kroma/tv'] },
    build: {
      target: 'es2022',
      outDir: 'dist',
      // One JS + one CSS file: simplest to host / drop onto the device.
      cssCodeSplit: false,
      modulePreload: { polyfill: false },
      reportCompressedSize: true,
      rollupOptions: { output: { manualChunks: undefined } },
    },
    // Keep console.* during dev (on-device debugging over the LAN); strip in builds.
    // `drop` is a valid runtime esbuild/Vite option, but it is absent from some
    // resolved `ESBuildOptions` type versions (CI resolves a Vite whose type omits
    // it, local resolves one that has it), which broke `tsc` on the fresh literal.
    // Cast to the field type so it compiles either way.
    esbuild: {
      drop: command === 'build' ? ['console', 'debugger'] : [],
      legalComments: 'none',
    } as UserConfig['esbuild'],
  }),
);
