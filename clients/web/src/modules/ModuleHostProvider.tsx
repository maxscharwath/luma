// Runs the frontend module runtime for the whole authenticated app instead of a
// single page: it loads Module Federation remotes, runs each module's
// setup()/exports in dependency order, and exposes the wired host plus the nav
// entries and routes modules contribute. Mounted once near the root so both the
// main app shell and the admin shell can read module contributions. This is the
// single runtime mount point that replaced the old /modules page.

import { hasPermission, type Locale, type MessageKey, translateIn, type TVars } from '@luma/core';
import type { LumaHost, ModuleNav, ModulePanel, ModuleRoute } from '@luma/module-sdk';
import { useLocale, useT } from '@luma/ui';
import { useQuery } from '@tanstack/react-query';
import { createContext, type ReactNode, useContext, useMemo } from 'react';
import { useModuleHost } from '#web/modules/host';
import { moduleRegistry } from '#web/modules/registry';
import { useAuth } from '#web/shared/lib/auth';

/** A translator bound to one module's own catalogs, falling back to the core
 *  catalogs (so a module can also reuse a core key). Used for the module's nav
 *  labels and for the `host.i18n.t` its pages receive. */
export function useModuleT(moduleId: string): (key: string, vars?: TVars) => string {
  const locale = useLocale();
  const t = useT();
  return useMemo(() => {
    const catalogs = moduleRegistry.localesOf(moduleId) ?? {};
    return (key: string, vars?: TVars) =>
      translateIn(catalogs, locale as Locale, key, vars) ?? t(key as MessageKey, vars);
  }, [moduleId, locale, t]);
}

interface ModuleHostValue {
  host: LumaHost | null;
  nav: ModuleNav[];
  routes: ModuleRoute[];
  panels: ModulePanel[];
  disabledIds: ReadonlySet<string>;
}

const EMPTY: ModuleHostValue = {
  host: null,
  nav: [],
  routes: [],
  panels: [],
  disabledIds: new Set(),
};

const ModuleHostContext = createContext<ModuleHostValue>(EMPTY);

export function ModuleHostProvider({ children }: Readonly<{ children: ReactNode }>) {
  const host = useModuleHost();

  // The backend's active-module list carries the enabled flags. Keyed ['modules']
  // so it dedupes with the host's own fetch. Disabled modules keep their nav
  // hidden and their pages return the not-found state.
  const { data: manifest } = useQuery({
    queryKey: ['modules'],
    queryFn: () => (host ? host.api.listModules() : Promise.resolve([])),
    enabled: host != null,
  });
  const disabledIds = useMemo(
    () => new Set((manifest ?? []).filter((m) => m.enabled === false).map((m) => m.id)),
    [manifest],
  );

  // Read the registry's contributions once the host is ready (remotes have
  // registered by then). `host` is set exactly once (null -> wired), so this memo
  // runs once and returns a stable array of stable lazy component refs, which is
  // what keeps the panels from retrying into a Suspense loop under the compiler.
  const contrib = useMemo<{ nav: ModuleNav[]; routes: ModuleRoute[]; panels: ModulePanel[] }>(() => {
    if (!host) return { nav: [], routes: [], panels: [] };
    try {
      return {
        nav: moduleRegistry.navItems(),
        routes: moduleRegistry.routes(),
        panels: moduleRegistry.settingsPanels(),
      };
    } catch {
      // A graph that failed to resolve (start() fell back to a no-op host) has no
      // usable contributions; keep them empty rather than crash the whole app.
      return { nav: [], routes: [], panels: [] };
    }
  }, [host]);

  const value = useMemo<ModuleHostValue>(
    () => ({ host, nav: contrib.nav, routes: contrib.routes, panels: contrib.panels, disabledIds }),
    [host, contrib, disabledIds],
  );
  return <ModuleHostContext.Provider value={value}>{children}</ModuleHostContext.Provider>;
}

/** The wired module host, or null until modules finish starting. */
export function useModuleHostValue(): LumaHost | null {
  return useContext(ModuleHostContext).host;
}

/** Every module nav entry the current account may see (enabled module + met
 *  `requires` capability), each `label` localized through its module's catalog. A
 *  module label is a key resolved against the module's own catalog; a plain
 *  string (no matching key) passes through as-is. */
export function useModuleNavAll(): ModuleNav[] {
  const { nav, disabledIds } = useContext(ModuleHostContext);
  const { user } = useAuth();
  const locale = useLocale();
  const t = useT();
  return useMemo(
    () =>
      nav
        .filter((n) => {
          if (disabledIds.has(n.moduleId)) return false;
          if (n.requires) {
            const cap = n.requires as Parameters<typeof hasPermission>[1];
            if (!user || !hasPermission(user, cap)) return false;
          }
          return true;
        })
        .map((n) => ({
          ...n,
          label:
            translateIn(moduleRegistry.localesOf(n.moduleId) ?? {}, locale as Locale, n.label) ??
            t(n.label as MessageKey),
        })),
    [nav, disabledIds, user, locale, t],
  );
}

/** Module nav entries for a nav-group id ("acquisition", "media", "library",
 *  "admin", ...). `section` defaults to "library". */
export function useModuleNav(section: string): ModuleNav[] {
  const all = useModuleNavAll();
  return useMemo(() => all.filter((n) => (n.section ?? 'library') === section), [all, section]);
}

/** The module route mounted at `path` under a splat host route, if its module is
 *  registered and enabled. */
export function useModuleRoute(path: string): ModuleRoute | undefined {
  const { routes, disabledIds } = useContext(ModuleHostContext);
  return useMemo(
    () => routes.find((r) => r.path === path && !disabledIds.has(r.moduleId)),
    [routes, disabledIds, path],
  );
}

/** Settings panels a module contributes (plus the host to render them with), for
 *  the admin Modules page. Empty when the module ships none. */
export function useModuleSettingsPanels(moduleId: string): {
  host: LumaHost | null;
  panels: ModulePanel[];
} {
  const { host, panels } = useContext(ModuleHostContext);
  const forModule = useMemo(
    () => panels.filter((p) => p.moduleId === moduleId),
    [panels, moduleId],
  );
  return { host, panels: forModule };
}
