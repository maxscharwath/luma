// `defineModule` — the compact way to declare a frontend module. It builds a
// `LumaModule` from the module's `module.json` plus a list of pages, so a module
// author never re-derives id/version/dependsOn, never hand-writes the nav `to`
// (it is derived from the page's section + path), and never lists locales twice.

import type { ComponentType } from 'react';
import type { LumaHost } from './host';
import type { LumaModule, ModuleComponentProps, NavItem, SettingsPanel } from './module';
import type { Dependencies } from './types';

/** Admin nav-group ids (see the admin shell's NAV_GROUPS) plus the generic
 *  `admin` fallback. A page in one of these sections mounts under `/admin`;
 *  every other section (e.g. `library`) mounts under the app root. */
const ADMIN_SECTIONS = new Set([
  'management',
  'media',
  'acquisition',
  'system',
  'maintenance',
  'admin',
]);

/** The absolute URL a page's nav entry links to, derived from its section +
 *  path: admin sections live under `/admin/<path>`, everything else under
 *  `/<path>`. This is the single source of truth for a module page's URL, so the
 *  route path and the nav link can never drift (and there is no `m/` segment). */
export function pageHref(section: string, path: string): string {
  const clean = path.replace(/^\/+/, '');
  return ADMIN_SECTIONS.has(section) ? `/admin/${clean}` : `/${clean}`;
}

/** One page a module contributes: the route (path + component) and, when it
 *  should show in a sidebar, its nav metadata. Omit `nav` for routes with no
 *  sidebar entry (deep links, detail pages). `section` picks the shell. */
export interface ModulePage {
  /** Path segment under the module mount point, e.g. `"vpn"`. */
  path: string;
  /** The screen. Wrap in `React.lazy` so each page is its own chunk. */
  component: ComponentType<ModuleComponentProps>;
  /** Sidebar entry for this page; the `to` is derived, never written by hand. */
  nav?: Omit<NavItem, 'to'> & { section: string };
}

/** The fields `defineModule` reads from a module's `module.json`. */
export interface ModuleManifestInput {
  id: string;
  version: string;
  dependsOn?: Dependencies;
  optionalDependsOn?: Dependencies;
}

export interface DefineModuleOptions<Exports = unknown> {
  /** The module's `module.json`. Injected automatically by the
   *  `@luma/module-sdk/vite` plugin (which fills it + `locales` from the module's
   *  folder), so the options-only `defineModule({ pages })` form works. Pass it
   *  explicitly via the two-arg form when the plugin is not in play. */
  manifest?: ModuleManifestInput;
  /** Message catalogs. Injected by the `@luma/module-sdk/vite` plugin from the
   *  module's `locales/` folder. Accepts a plain `{ en, fr }` map OR the result of
   *  `import.meta.glob('../../locales/*.json', { eager: true, import: 'default' })`
   *  — path keys like `../../locales/en.json` are normalized to the locale code. */
  locales?: Record<string, Record<string, string>>;
  /** The module's pages (routes + optional nav), one entry per screen. */
  pages?: ModulePage[];
  settingsPanels?: SettingsPanel[];
  exports?: (host: LumaHost) => Exports;
  setup?: (host: LumaHost) => void | Promise<void>;
  /** Override the manifest-derived dependencies (rarely needed). */
  dependsOn?: Dependencies;
  optionalDependsOn?: Dependencies;
}

/** Build a `LumaModule` from its manifest + pages: id/version/dependsOn come from
 *  the manifest, locales are normalized (so a glob import works), and each nav
 *  `to` is derived from its page's section + path.
 *
 *  Two call forms:
 *  - `defineModule({ pages })` — the manifest + locales are injected from the
 *    module's folder by the `@luma/module-sdk/vite` plugin (the default).
 *  - `defineModule(manifest, { pages })` — explicit, for when the plugin is off. */
export function defineModule<Exports = unknown>(
  manifestOrOptions: ModuleManifestInput | DefineModuleOptions<Exports>,
  maybeOptions?: DefineModuleOptions<Exports>,
): LumaModule<Exports> {
  const explicit = maybeOptions !== undefined;
  const options = (explicit ? maybeOptions : manifestOrOptions) as DefineModuleOptions<Exports>;
  const manifest = (explicit ? manifestOrOptions : options.manifest) as
    | ModuleManifestInput
    | undefined;
  if (!manifest) {
    throw new Error(
      'defineModule: no manifest. Pass it as the first argument, or enable the ' +
        '@luma/module-sdk/vite plugin, which injects the manifest + locales by convention.',
    );
  }
  const pages = options.pages ?? [];
  const routes = pages.map((p) => ({ path: p.path, component: p.component }));
  const navItems: NavItem[] = pages.flatMap((p) =>
    p.nav ? [{ ...p.nav, to: pageHref(p.nav.section, p.path) }] : [],
  );
  return {
    id: manifest.id,
    version: manifest.version,
    dependsOn: options.dependsOn ?? manifest.dependsOn,
    optionalDependsOn: options.optionalDependsOn ?? manifest.optionalDependsOn,
    routes: routes.length > 0 ? routes : undefined,
    navItems: navItems.length > 0 ? navItems : undefined,
    settingsPanels: options.settingsPanels,
    locales: normalizeLocales(options.locales),
    exports: options.exports,
    setup: options.setup,
  };
}

/** Normalize locales that may be path-keyed (from `import.meta.glob`) into a
 *  `{ localeCode: catalog }` map. A plain `{ en, fr }` map passes through. */
function normalizeLocales(
  locales: DefineModuleOptions['locales'],
): Record<string, Record<string, string>> | undefined {
  if (!locales) return undefined;
  const out: Record<string, Record<string, string>> = {};
  for (const [key, catalog] of Object.entries(locales)) {
    // '../../locales/en.json' -> 'en'; a bare 'en' is left as-is.
    const code = key.replace(/^.*\//, '').replace(/\.json$/, '');
    out[code] = catalog;
  }
  return Object.keys(out).length > 0 ? out : undefined;
}
