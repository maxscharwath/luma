import { fileURLToPath } from 'node:url';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import { tvFrame } from '../tv-frame.vite';

const repoRoot = fileURLToPath(new URL('../..', import.meta.url));

// Supported TV floor: Chrome 99 (Tizen 8 / webOS 24, 2024+ models). Tailwind v4
// requires cascade layers (Chrome 99) so that is the hard minimum; Lightning CSS
// then down-levels the remaining modern CSS (color-mix(), oklch()) that only
// landed in Chrome 111 to plain fallbacks. Version encoding: major << 16.
const TV_CSS_TARGETS = { chrome: 99 << 16 };

export default defineConfig({
  // `tvFrame()` is dev-only (apply: 'serve') it letterboxes the app into a
  // 1920×1080 16:9 stage in the browser; never injected into `vite build` output.
  plugins: [tailwindcss(), react(), tvFrame()],
  // `#tv/*` → the @luma/tv package src (mirrors tsconfig.base paths; Vite needs it explicitly).
  resolve: { alias: { '#tv': fileURLToPath(new URL('../../packages/tv/src', import.meta.url)) } },
  // Packaged webOS apps load via file:// assets must be referenced relatively.
  base: './',
  server: {
    port: 5175,
    fs: { allow: [repoRoot] },
  },
  optimizeDeps: { exclude: ['@luma/ui', '@luma/core', '@luma/tv'] },
  // Down-level Tailwind v4's modern CSS (cascade layers, color-mix, oklch) to plain
  // fallbacks for old webOS webviews. Fonts load via <link> in index.html so no
  // remote @import reaches the transformer.
  css: {
    transformer: 'lightningcss',
    lightningcss: { targets: TV_CSS_TARGETS },
  },
  // webOS 24+ Chromium (108+, 2024 models) modern target, lean output.
  build: {
    target: 'es2020',
    outDir: 'dist',
    cssCodeSplit: false,
    cssMinify: 'lightningcss',
    modulePreload: { polyfill: false },
    reportCompressedSize: true,
    rollupOptions: { output: { manualChunks: undefined } },
  },
  esbuild: { drop: ['console', 'debugger'], legalComments: 'none' },
});
