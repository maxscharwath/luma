// Shared Vite config factory for every TV shell (Tizen, webOS, Android TV, ...).
// A shell declares WHAT it targets in its `tv.target.ts` (platform, dev port,
// engine floors); this factory turns that into the HOW - so all shells build the
// same way and a new platform is a 4-line target file, not a copied config.
//
// Two tiers per target:
//  - modern (always): ESM / ES2020, Tailwind v4 untouched, Lightning CSS
//    down-levels color-mix()/oklch() to `chromeFloor` (default 99 - Tailwind's
//    cascade layers make Chrome 99 the hard minimum for this tier).
//  - legacy (opt-in via `legacyChrome`): a second self-contained ES2015 IIFE +
//    flattened stylesheet for engines below the floor (e.g. webOS 4.x-23 =
//    Chromium 53-94). dist/index.html is rewritten into an ES5 loader that
//    picks the tier at runtime. See legacy-css.ts / legacy-finalize.ts.

import { networkInterfaces } from 'node:os';
import { fileURLToPath } from 'node:url';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import type { ConfigEnv, UserConfig } from 'vite';
import { tvFrame } from '../tv-frame.vite';
import { legacyFinalize } from './legacy-finalize';

export interface TvTarget {
  /** Which TV this shell is for (diagnostics label; playback wiring is runtime-detected). */
  platform: 'tizen' | 'webos' | 'androidtv';
  /** Vite dev-server port for this shell. */
  port: number;
  /** Chrome floor of the MODERN bundle's Lightning CSS down-level. Tailwind v4
   * keeps its cascade layers, so 99 (the default) is the hard minimum. */
  chromeFloor?: number;
  /** Also emit the LEGACY tier (ES2015 IIFE + flattened CSS in dist/legacy/,
   * runtime-gated loader in dist/index.html) down to this Chrome major.
   * Omit for modern-only shells. */
  legacyChrome?: number;
  /** Honor KROMA_TV_DEVICE=1 on-device dev (LAN HMR, no letterbox frame, keep
   * console.* so on-TV logs reach the platform log collector). */
  deviceDev?: boolean;
}

/** This machine's LAN IPv4 a dev TV connects back to for the HMR websocket.
 * KROMA_TV_HOST (set by dev-device.sh) wins; the scan is only a fallback. */
function lanIp(): string | undefined {
  if (process.env.KROMA_TV_HOST) return process.env.KROMA_TV_HOST;
  return Object.values(networkInterfaces())
    .flatMap((addrs) => addrs ?? [])
    .find((a) => a.family === 'IPv4' && !a.internal)?.address;
}

/** The MODERN tier config. `shellUrl` is the calling vite.config's import.meta.url. */
export function tvShellConfig(shellUrl: string, target: TvTarget) {
  const repoRoot = fileURLToPath(new URL('../..', shellUrl));
  const deviceDev = target.deviceDev === true && process.env.KROMA_TV_DEVICE === '1';
  const floor = target.chromeFloor ?? 99;
  return ({ command }: ConfigEnv): UserConfig => ({
    // `tvFrame()` is dev-only (apply: 'serve'): letterboxes the app into a
    // 1920x1080 stage in a desktop browser; on a real TV the panel already is
    // that canvas, so device mode turns it off.
    plugins: [tailwindcss(), react(), tvFrame({ enabled: !deviceDev })],
    // `#tv/*` -> the @kroma/tv package src (mirrors tsconfig.base paths).
    resolve: { alias: { '#tv': fileURLToPath(new URL('../../packages/tv/src', shellUrl)) } },
    // Packaged TV apps load from a local path: assets must be referenced relatively.
    base: './',
    server: {
      host: deviceDev ? true : undefined,
      port: target.port,
      hmr: deviceDev ? { host: lanIp(), protocol: 'ws' } : undefined,
      fs: { allow: [repoRoot] },
    },
    optimizeDeps: { exclude: ['@kroma/ui', '@kroma/core', '@kroma/tv'] },
    // Down-level the modern CSS Tailwind emits (color-mix, oklch) to plain
    // fallbacks. Fonts load via <link> in index.html so no remote @import
    // reaches the transformer. Version encoding: major << 16.
    css: {
      transformer: 'lightningcss',
      lightningcss: { targets: { chrome: floor << 16 } },
    },
    build: {
      target: 'es2020',
      outDir: 'dist',
      // One JS + one CSS file: fewer round-trips on a TV's slow connection.
      cssCodeSplit: false,
      cssMinify: 'lightningcss',
      modulePreload: { polyfill: false },
      reportCompressedSize: true,
      rolldownOptions: {
        // Strip logging from shipped bundles; dev keeps console.* so on-TV logs
        // still surface in the platform log collector. vite 8 IGNORES
        // `esbuild.drop` (oxc took over), so dropping lives in the oxc minifier
        // output options now. Legal comments are already stripped by minify.
        output: {
          minify:
            command === 'build'
              ? { compress: { dropConsole: true, dropDebugger: true }, mangle: true, codegen: true }
              : undefined,
        },
      },
    },
  });
}

/** The LEGACY tier config (only for targets with `legacyChrome`). Builds
 * src/main.legacy.ts (polyfills + the same app) into dist/legacy/ and rewrites
 * dist/index.html into the runtime engine gate - run it AFTER the modern build,
 * then `bun ../tv-build/check-legacy.ts` to guard the output. */
export function tvShellLegacyConfig(shellUrl: string, target: TvTarget): UserConfig {
  const repoRoot = fileURLToPath(new URL('../..', shellUrl));
  const chrome = target.legacyChrome;
  if (!chrome) throw new Error(`tv.target for ${target.platform} has no legacyChrome`);
  return {
    plugins: [
      tailwindcss(),
      react(),
      legacyFinalize({ distDir: fileURLToPath(new URL('dist', shellUrl)), chrome }),
    ],
    resolve: { alias: { '#tv': fileURLToPath(new URL('../../packages/tv/src', shellUrl)) } },
    base: './',
    // appinfo/manifest + icons are already copied into dist/ by the modern build.
    publicDir: false,
    server: { fs: { allow: [repoRoot] } },
    // Assets emitted by this build live under dist/legacy/, but URLs resolve
    // against the document (dist/index.html) - prefix them with the subdirectory.
    experimental: {
      renderBuiltUrl: (filename: string) => `./legacy/${filename}`,
    },
    build: {
      target: 'es2015',
      outDir: 'dist/legacy',
      emptyOutDir: true,
      cssCodeSplit: false,
      // Keep @layer intact for the post-build pass; legacyFinalize minifies.
      cssMinify: false,
      modulePreload: { polyfill: false },
      reportCompressedSize: true,
      rolldownOptions: {
        input: fileURLToPath(new URL('src/main.legacy.ts', shellUrl)),
        output: {
          // No <script type=module> on old engines: one classic self-contained file.
          format: 'iife',
          inlineDynamicImports: true,
          entryFileNames: 'index.js',
          assetFileNames: (info: { names?: string[] }) =>
            (info.names?.[0] ?? '').endsWith('.css')
              ? 'style.css'
              : 'assets/[name]-[hash][extname]',
          // vite 8 ignores `esbuild.drop`: console/debugger stripping moved to
          // the oxc minifier output options.
          minify: {
            compress: { dropConsole: true, dropDebugger: true },
            mangle: true,
            codegen: true,
          },
        },
      },
    },
  };
}
