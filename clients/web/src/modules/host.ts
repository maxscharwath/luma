// Adapts the web app's providers (auth, i18n, router, session) into the neutral
// LumaHost the module SDK defines, then resolves + starts the registry. A module
// only ever sees this host surface, never the app internals - which is exactly
// what lets a module be compiled in today or runtime-loaded later without
// touching the module's own code.

import { hasPermission, type MessageKey, sessionToken, type TVars } from '@luma/core';
import {
  createEventBus,
  type HostBase,
  type LumaHost,
  type ModuleManifest,
} from '@luma/module-sdk';
import { useLocale, useT } from '@luma/ui';
import { useNavigate } from '@tanstack/react-router';
import { useEffect, useRef, useState } from 'react';
import { apiBase } from '#web/shared/lib/api';
import { moduleRegistry } from '#web/modules/registry';
import { loadRuntimeRemotes } from '#web/modules/remotes';
import { useAuth } from '#web/shared/lib/auth';

/** GET a JSON resource under the server's /api, carrying the session bearer. */
async function apiGet<T>(path: string): Promise<T> {
  const token = sessionToken();
  const res = await fetch(`${apiBase()}/api${path}`, {
    headers: token ? { Authorization: `Bearer ${token}` } : {},
  });
  if (!res.ok) throw new Error(`GET /api${path} failed (${res.status})`);
  return (await res.json()) as T;
}

// One event bus for the whole app, so a module's setup() subscriptions (which
// run once, see the registry's setupDone guard) stay valid across page visits.
const appBus = createEventBus();

/** Resolve + start the module registry against a host adapted from the app's
 *  providers, returning the wired host once ready. The host is built ONCE and
 *  reads the latest auth / i18n / router values through a ref, so its identity
 *  never changes: re-creating it each render would re-run the effect, call
 *  setHost, and loop (React error #185). */
export function useModuleHost(): LumaHost | null {
  const navigate = useNavigate();
  const t = useT();
  const locale = useLocale();
  const auth = useAuth();

  // Keep the newest provider values in a ref WITHOUT mutating during render
  // (which the React Compiler dislikes and can turn into an update loop). The
  // host is built once, in a run-once effect, and reads through this ref.
  const latest = useRef({ navigate, t, locale, auth });
  useEffect(() => {
    latest.current = { navigate, t, locale, auth };
  });

  const [host, setHost] = useState<LumaHost | null>(null);
  // Only wire modules once there is a session: a module's setup() must not run on
  // the pre-auth login screen, and `/api/modules` would 401 anyway. Re-running on
  // sign-in is safe (loadRuntimeRemotes + start() are idempotent via the
  // registry's `has`/`setupDone`), so there is no run-once ref guard.
  const authed = auth.user != null;
  useEffect(() => {
    if (!authed) return;
    let alive = true;
    const base: HostBase = {
      api: {
        get: apiGet,
        listModules: () => apiGet<ModuleManifest[]>('/modules'),
      },
      auth: {
        get userId() {
          return latest.current.auth.user?.id ?? null;
        },
        can: (capability) => {
          const u = latest.current.auth.user;
          return !!u && hasPermission(u, capability as Parameters<typeof hasPermission>[1]);
        },
      },
      i18n: {
        // Core-catalog translator (the per-module catalog overlay is applied by
        // the host each module actually receives -- see ModuleRouteOutlet). Pass
        // `vars` through so plural/interpolation works.
        t: (key, vars) => latest.current.t(key as MessageKey, vars as TVars | undefined),
        get locale() {
          return latest.current.locale;
        },
      },
      // The module system uses plain string paths; the concrete router types its
      // routes, so the cast lives at this single boundary.
      nav: { navigate: (to) => void latest.current.navigate({ to } as never) },
      bus: appBus,
    };
    void (async () => {
      try {
        // Pull in any runtime-loaded (Module Federation) modules first, so they
        // resolve and set up alongside the compile-time ones.
        await loadRuntimeRemotes(moduleRegistry);
        // Don't run a disabled module's setup() (its panel is hidden too).
        let skip: Set<string> | undefined;
        try {
          const listed = await apiGet<ModuleManifest[]>('/modules');
          skip = new Set(listed.filter((m) => m.enabled === false).map((m) => m.id));
        } catch {
          skip = undefined; // enabled-state unavailable; set up everything
        }
        const wired = await moduleRegistry.start(base, skip);
        if (alive) setHost(wired);
      } catch (e) {
        // A start/federation failure must not hang the page on "Wiring
        // modules..." forever; render with a no-op module API instead.
        console.error('[modules] host start failed', e);
        if (alive) setHost({ ...base, getModuleApi: () => undefined });
      }
    })();
    return () => {
      alive = false;
    };
  }, [authed]);
  return host;
}
