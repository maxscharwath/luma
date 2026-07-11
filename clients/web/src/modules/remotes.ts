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
      const name = mfName(m.id);
      const expose = (m.feRemote as { module: string }).module.replace(/^\.\//, '');
      return {
        name,
        entry: `${apiBase()}/modules/${encodeURIComponent(m.id)}/remoteEntry.js`,
        module: `${name}/${expose}`,
      };
    });
}

/** MF remote name for a module id (must be a valid identifier -- no dots). Kept
 *  in sync with each module's vite `federation({ name })`. */
function mfName(id: string): string {
  return id.replace(/[^a-zA-Z0-9_]/g, '_');
}

// The Module Federation runtime is init'd ONCE (shared React singleton); remotes
// are added incrementally via registerRemotes so a module installed at runtime
// loads with no page reload. `loadedRemotes` tracks which are already registered.
let mfReady: Promise<typeof import('@module-federation/runtime')> | null = null;
const loadedRemotes = new Set<string>();

function ensureMf(): Promise<typeof import('@module-federation/runtime')> {
  if (!mfReady) {
    mfReady = import('@module-federation/runtime')
      .then((mf) => {
        mf.init({
          name: 'luma_web_host',
          remotes: [],
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
        return mf;
      })
      .catch((e) => {
        mfReady = null; // let a later call retry
        throw e;
      });
  }
  return mfReady;
}

/** Discover installed frontend remotes and load any not-yet-loaded ones into
 *  `registry`. RE-CALLABLE: after a Store install, calling it again loads just
 *  the new module (`type: 'module'` = ESM remoteEntry). Best-effort; a failed
 *  remote is logged + skipped, never breaking compile-time modules. Returns the
 *  ids newly registered. No-op during SSR / prerender. */
export async function loadRuntimeRemotes(registry: ModuleRegistry): Promise<string[]> {
  if (typeof window === 'undefined') return [];
  let specs: RemoteSpec[];
  try {
    specs = await discoverRemotes();
  } catch (e) {
    console.warn('[modules] remote discovery failed', e);
    return [];
  }
  const fresh = specs.filter((s) => !loadedRemotes.has(s.name));
  if (fresh.length === 0) return [];

  let mf: typeof import('@module-federation/runtime');
  try {
    mf = await ensureMf();
  } catch (e) {
    console.warn('[modules] federation init failed', e);
    return [];
  }
  mf.registerRemotes(fresh.map((s) => ({ name: s.name, entry: s.entry, type: 'module' as const })));

  const added: string[] = [];
  await Promise.all(
    fresh.map(async (s) => {
      loadedRemotes.add(s.name);
      try {
        const mod = (await mf.loadRemote<{ default: LumaModule }>(s.module))?.default;
        if (mod && !registry.has(mod.id)) {
          registry.register(mod);
          try {
            registry.order(); // validate deps; a bad one must not break the rest
            added.push(mod.id);
          } catch (err) {
            registry.unregister(mod.id);
            loadedRemotes.delete(s.name);
            console.warn(`[modules] runtime remote "${s.name}" has unmet deps/cycle; unregistered`, err);
          }
        }
      } catch (e) {
        loadedRemotes.delete(s.name);
        console.warn(`[modules] runtime remote "${s.name}" failed to load`, e);
      }
    }),
  );
  return added;
}

/** Whether this module's frontend was loaded as a runtime remote (vs compiled in). */
export function isLoadedRemote(id: string): boolean {
  return loadedRemotes.has(mfName(id));
}

/** Forget a remote so a later reinstall re-loads it (the loaded MF code stays in
 *  memory, but its module is unregistered from the app registry). */
export function forgetRemote(id: string): void {
  loadedRemotes.delete(mfName(id));
}
