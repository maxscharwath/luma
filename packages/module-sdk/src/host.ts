// The host context: everything a module receives at setup. The host (the web
// app, or a TV shell later) builds this and hands it to each module, so modules
// never import app internals - they talk to this stable surface.

import type { EventBus } from './bus';
import type { ModuleApiRegistry } from './contracts';
import type { ModuleManifest } from './types';

/** The slice of the server API the host grants modules. The web host adapts its
 *  concrete `LumaClient` to this; a module never depends on the client package. */
export interface HostApi {
  /** GET a JSON resource under the server's `/api`, with auth attached. */
  get<T>(path: string): Promise<T>;
  /** The active backend modules (`GET /api/modules`). */
  listModules(): Promise<ModuleManifest[]>;
}

export interface HostAuth {
  readonly userId: string | null;
  /** Whether the current account holds a capability (permission). */
  can(capability: string): boolean;
}

export interface HostI18n {
  t(key: string, vars?: Record<string, unknown>): string;
  readonly locale: string;
}

export interface HostNav {
  navigate(to: string): void;
}

/** Everything a module receives at setup. */
export interface LumaHost {
  api: HostApi;
  auth: HostAuth;
  i18n: HostI18n;
  nav: HostNav;
  bus: EventBus;
  /** Access another module's exported API. Only meaningful for a module the
   *  caller declared in its `dependsOn`, which guarantees it is set up first. */
  getModuleApi<K extends keyof ModuleApiRegistry>(id: K): ModuleApiRegistry[K] | undefined;
}

/** What the host provides to the registry; `getModuleApi` is wired by the
 *  registry itself, so the caller supplies everything except that. */
export type HostBase = Omit<LumaHost, 'getModuleApi'>;
