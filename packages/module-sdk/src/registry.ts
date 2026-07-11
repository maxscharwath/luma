// The frontend module registry: the host-side mirror of the Rust `Registry`.
// It gathers `LumaModule`s, resolves their dependency graph (same Kahn topo
// sort, same missing-dep / cycle / duplicate errors), runs setup in order,
// collects the routes/nav/panels, and reconciles the registered set against the
// backend's `/api/modules` manifest.

import type { HostBase, LumaHost } from './host';
import type { LumaModule, NavItem, RouteDef, SettingsPanel } from './module';
import type { Dependency, ModuleManifest } from './types';

/** The module id of a dependency (a bare id, an "id@range" string, or an object). */
function depId(dep: Dependency): string {
  return typeof dep === 'string' ? (dep.split('@')[0] ?? dep) : dep.id;
}

/** A route with the id of the module that registered it. */
export type ModuleRoute = RouteDef & { moduleId: string };

/** A nav entry with the id of the module that contributed it. */
export type ModuleNav = NavItem & { moduleId: string };

/** A settings panel with its owning module id. */
export type ModulePanel = SettingsPanel & { moduleId: string };

/** Whether a registered frontend module has a matching active backend module. */
export interface ModuleStatus {
  id: string;
  frontend: true;
  backend: boolean;
  manifest?: ModuleManifest;
}

export class ModuleRegistry {
  private readonly modules = new Map<string, LumaModule>();
  // Module ids whose setup() has run, so re-entering start() (e.g. re-visiting
  // the page) does not re-run a module's setup side effects.
  private readonly setupDone = new Set<string>();

  register(module: LumaModule): this {
    if (this.modules.has(module.id)) {
      throw new Error(`module "${module.id}" registered twice`);
    }
    this.modules.set(module.id, module);
    return this;
  }

  /** Remove a module (used to roll back a runtime remote whose deps don't
   *  resolve, so it can't break order() for everyone else). */
  unregister(id: string): void {
    this.modules.delete(id);
    this.setupDone.delete(id);
  }

  has(id: string): boolean {
    return this.modules.has(id);
  }

  /** A module's own message catalogs (locale -> key -> string), if it ships any.
   *  The host resolves that module's labels + `host.i18n.t` against these. */
  localesOf(id: string): Record<string, Record<string, string>> | undefined {
    return this.modules.get(id)?.locales;
  }

  /** Modules in initialization order (dependencies first). Throws on a missing
   *  hard dependency or a cycle. Edges = hard deps (must be registered) + any
   *  optional deps that happen to be present. (Version ranges + capability deps
   *  are enforced on the backend; the frontend only needs setup ordering.) */
  order(): LumaModule[] {
    const mods = [...this.modules.values()];
    const edgesOf = (m: LumaModule): string[] => {
      const ids: string[] = [];
      for (const dep of m.dependsOn ?? []) {
        const id = depId(dep);
        if (!this.modules.has(id)) {
          throw new Error(`module "${m.id}" depends on "${id}", which is not registered`);
        }
        ids.push(id);
      }
      for (const dep of m.optionalDependsOn ?? []) {
        const id = depId(dep);
        if (this.modules.has(id)) ids.push(id);
      }
      return ids;
    };

    const indegree = new Map<string, number>();
    const dependents = new Map<string, string[]>();
    for (const m of mods) indegree.set(m.id, 0);
    for (const m of mods) {
      for (const dep of edgesOf(m)) {
        indegree.set(m.id, (indegree.get(m.id) ?? 0) + 1);
        const list = dependents.get(dep) ?? [];
        list.push(m.id);
        dependents.set(dep, list);
      }
    }

    const queue = mods.filter((m) => (indegree.get(m.id) ?? 0) === 0).map((m) => m.id);
    const orderedIds: string[] = [];
    for (let i = 0; i < queue.length; i++) {
      const id = queue[i];
      if (id === undefined) continue;
      orderedIds.push(id);
      for (const dependent of dependents.get(id) ?? []) {
        const next = (indegree.get(dependent) ?? 0) - 1;
        indegree.set(dependent, next);
        if (next === 0) queue.push(dependent);
      }
    }

    if (orderedIds.length !== mods.length) {
      const stuck = mods.map((m) => m.id).filter((id) => !orderedIds.includes(id));
      throw new Error(`module dependency cycle among [${stuck.join(', ')}]`);
    }
    return orderedIds.map((id) => this.modules.get(id)).filter((m): m is LumaModule => m != null);
  }

  /** Resolve the graph, compute each module's exports, and run its setup - all
   *  in dependency order - then return the fully-wired host. Modules in
   *  `skipSetup` (e.g. admin-disabled ones) have their setup skipped, and every
   *  module's setup runs at most once across calls. */
  async start(base: HostBase, skipSetup?: ReadonlySet<string>): Promise<LumaHost> {
    const exports = new Map<string, unknown>();
    const host: LumaHost = {
      ...base,
      getModuleApi: (id) => exports.get(id as string) as never,
    };
    for (const module of this.order()) {
      if (module.exports) exports.set(module.id, module.exports(host));
      if (skipSetup?.has(module.id) || this.setupDone.has(module.id)) continue;
      await module.setup?.(host);
      this.setupDone.add(module.id);
    }
    return host;
  }

  navItems(): ModuleNav[] {
    return this.order().flatMap((m) =>
      (m.navItems ?? []).map((n) => ({ ...n, moduleId: m.id })),
    );
  }

  routes(): ModuleRoute[] {
    // Route paths are the URL under the mount point (/m/<path>), so they must be
    // unique across modules. Keep the first registrant and skip a collision (with
    // a warning) rather than silently shadowing one page with another.
    const out: ModuleRoute[] = [];
    const owner = new Map<string, string>();
    for (const m of this.order()) {
      for (const r of m.routes ?? []) {
        const taken = owner.get(r.path);
        if (taken) {
          console.warn(
            `[modules] route path "${r.path}" from "${m.id}" collides with "${taken}"; ignoring the duplicate`,
          );
          continue;
        }
        owner.set(r.path, m.id);
        out.push({ ...r, moduleId: m.id });
      }
    }
    return out;
  }

  settingsPanels(): ModulePanel[] {
    return this.order().flatMap((m) =>
      (m.settingsPanels ?? []).map((p) => ({ ...p, moduleId: m.id })),
    );
  }

  /** Cross-check registered frontend modules against the backend manifest. A
   *  frontend module whose id is absent from `/api/modules` has no backend
   *  installed (`backend: false`); the host can hide or disable it. */
  reconcile(manifest: ModuleManifest[]): ModuleStatus[] {
    const backend = new Map(manifest.map((m) => [m.id, m]));
    return [...this.modules.values()].map((m) => ({
      id: m.id,
      frontend: true as const,
      backend: backend.has(m.id),
      manifest: backend.get(m.id),
    }));
  }
}
