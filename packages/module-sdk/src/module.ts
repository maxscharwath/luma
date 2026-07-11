// The frontend module contract. A `@luma/module-<id>` package exports one
// `LumaModule` describing the routes, nav entries and settings panels it
// contributes, the API it exports for other modules, and any setup wiring. The
// `id` must match its backend crate's module id.

import type { ComponentType } from 'react';
import type { LumaHost } from './host';
import type { Dependency } from './types';

/** A nav entry a module contributes to the host's navigation. */
export interface NavItem {
  /** Route path this entry links to (a route the module also registers). */
  to: string;
  label: string;
  /** Optional icon name, resolved by the host's icon set. */
  icon?: string;
  /** Which host nav group this belongs to ("library", "admin"). */
  section?: string;
  /** Capability the account needs; the host hides the entry otherwise. */
  requires?: string;
}

/** Props every module-provided screen receives. */
export interface ModuleComponentProps {
  host: LumaHost;
}

/** A route a module registers under the host's module mount point. */
export interface RouteDef {
  /** Path segment under the mount point, e.g. "acquisition". */
  path: string;
  /** The screen. Wrap in `React.lazy` so each module is its own chunk. */
  component: ComponentType<ModuleComponentProps>;
}

export interface SettingsPanel {
  id: string;
  label: string;
  component: ComponentType<ModuleComponentProps>;
}

export interface LumaModule<Exports = unknown> {
  /** Stable id, shared with the backend crate's module manifest. */
  id: string;
  version: string;
  /** Modules that must be present + set up before this one (version ranges are
   *  enforced on the backend; the frontend uses the id for setup ordering). */
  dependsOn?: Dependency[];
  /** Soft dependencies: set up first when present, but not required. */
  optionalDependsOn?: Dependency[];
  routes?: RouteDef[];
  navItems?: NavItem[];
  settingsPanels?: SettingsPanel[];
  /** The module's own message catalogs, keyed by locale code then message key
   *  (e.g. `{ en: { title: "Torrents" }, fr: { title: "Torrents" } }`). The host
   *  resolves a module's `label`s + its `host.i18n.t` against these first, then
   *  falls back to the core catalogs -- so a module ships its own translations
   *  without touching the app's typed key union. */
  locales?: Record<string, Record<string, string>>;
  /** A typed API other modules reach via `host.getModuleApi(id)`. Computed once
   *  at start, in dependency order. */
  exports?: (host: LumaHost) => Exports;
  /** Imperative wiring: subscribe to events, warm caches. Runs once at start. */
  setup?: (host: LumaHost) => void | Promise<void>;
}
