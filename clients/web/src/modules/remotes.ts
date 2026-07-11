// The web + desktop runtime-load tier (Module Federation).
//
// Uses the Module Federation *runtime* directly (no vite plugin), so the
// TanStack Start build is untouched. Remotes are DISCOVERED from the backend:
// `GET /api/modules` lists every installed module, and each one that ships a
// `feRemote` is loaded from `/modules/<id>/remoteEntry.js` (served same-origin by
// the Rust server from the module's install dir). Its exposed `LumaModule` is
// registered into the same ModuleRegistry the compile-time modules use.
//
// The Chromium-53 TV tier never runs this (no dynamic import / import maps); TVs
// stay compile-time bundled. `@module-federation/runtime` is imported inside the
// browser-only path so it never runs during the SSR shell prerender.

import { sessionToken } from '@luma/core';
import type { LumaModule, ModuleManifest, ModuleRegistry } from '@luma/module-sdk';
import * as React from 'react';
import * as ReactDOM from 'react-dom';
import { apiBase } from '#web/shared/lib/api';

interface RemoteSpec {
  /** MF remote name (= module id). */
  name: string;
  /** Absolute remoteEntry.js URL (same origin as the API). */
  entry: string;
  /** loadRemote key: `<name>/<exposedKey>`. */
  module: string;
}

/** Discover installed frontend remotes from the backend module list. Only
 *  enabled modules that ship a `feRemote` are loaded; a disabled module's page
 *  stays gone. */
async function discoverRemotes(): Promise<RemoteSpec[]> {
  const token = sessionToken();
  const res = await fetch(`${apiBase()}/api/modules`, {
    headers: token ? { Authorization: `Bearer ${token}` } : {},
  });
  if (!res.ok) return [];
  const mods = (await res.json()) as ModuleManifest[];
  return mods
    .filter((m) => m.feRemote != null && m.enabled !== false)
    .map((m) => {
      // MF remote names must be valid identifiers (no dots), so a reverse-DNS id
      // is sanitized the same way the module's vite `federation({ name })` is.
      const name = m.id.replace(/[^a-zA-Z0-9_]/g, '_');
      const expose = (m.feRemote as { module: string }).module.replace(/^\.\//, '');
      return {
        name,
        entry: `${apiBase()}/modules/${encodeURIComponent(m.id)}/remoteEntry.js`,
        module: `${name}/${expose}`,
      };
    });
}

async function doLoad(registry: ModuleRegistry): Promise<void> {
  const specs = await discoverRemotes();
  if (specs.length === 0) return;

  const { init, loadRemote } = await import('@module-federation/runtime');
  init({
    name: 'luma_web_host',
    // `type: 'module'` loads remoteEntry.js as an ESM module (dynamic import), not
    // a classic <script> (which throws for a Vite ESM remote).
    remotes: specs.map((s) => ({ name: s.name, entry: s.entry, type: 'module' as const })),
    shared: {
      react: {
        version: React.version,
        lib: () => React,
        shareConfig: { singleton: true, requiredVersion: '^19' },
      },
      'react-dom': {
        version: React.version,
        lib: () => ReactDOM,
        shareConfig: { singleton: true, requiredVersion: '^19' },
      },
    },
  });

  await Promise.all(
    specs.map(async (s) => {
      try {
        const loaded = await loadRemote<{ default: LumaModule }>(s.module);
        if (!loaded) throw new Error('loadRemote returned null');
        const mod = loaded.default;
        if (!registry.has(mod.id)) {
          registry.register(mod);
          try {
            // Validate the remote's deps resolve; a bad one must not break order()
            // for the compile-time modules.
            registry.order();
          } catch (err) {
            registry.unregister(mod.id);
            console.warn(`[modules] runtime remote "${s.name}" has unmet deps/cycle; unregistered`, err);
          }
        }
      } catch (e) {
        console.warn(`[modules] runtime remote "${s.name}" failed to load`, e);
      }
    }),
  );
}

// Cache the whole load as a promise so it runs once per page load (a StrictMode /
// HMR double-invoke awaits the same run). A page reload after install/uninstall
// re-discovers the current set.
let runOnce: Promise<void> | null = null;

/** Discover + load every installed frontend remote into `registry`. Best-effort:
 *  a remote that fails is logged and skipped, never breaking compile-time
 *  modules. No-op during SSR / prerender. */
export function loadRuntimeRemotes(registry: ModuleRegistry): Promise<void> {
  if (typeof window === 'undefined') return Promise.resolve();
  if (!runOnce) {
    runOnce = doLoad(registry).catch((e) => {
      console.warn('[modules] runtime remotes failed', e);
    });
  }
  return runOnce;
}
