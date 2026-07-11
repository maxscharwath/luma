// The wire shape published by the backend at GET /api/modules. Mirrors the Rust
// `luma_module_sdk::ModuleManifest` (serialized camelCase). The frontend reads
// this to learn which backend modules are active and to reconcile them against
// the frontend modules registered in the host.

/** One capability a backend module provides, as a (kind, id) pair. */
export interface Capability {
  kind: string;
  id: string;
}

/** A dependency on another module: a bare id, an `"id@range"` string, or an
 *  object with a semver range. Version ranges are enforced on the backend; the
 *  frontend registry uses only the id for setup ordering. */
export type Dependency = string | { id: string; version?: string };

/** A capability dependency: satisfied by any module whose `provides` matches. */
export interface CapabilityReq {
  kind: string;
  id?: string;
}

/** One admin-configurable setting a module exposes. */
export interface ConfigField {
  key: string;
  label: string;
  type: 'string' | 'bool' | 'number' | 'select';
  default?: string;
  options?: string[];
}

/** The frontend remote a runtime-loaded module ships (Module Federation). The
 *  entry URL is derived by the host as `/modules/<id>/remoteEntry.js`. */
export interface FeRemote {
  /** The exposed module key to load (the remote's MF `exposes` name). */
  module: string;
}

/** A backend module's self-description. */
export interface ModuleManifest {
  /** Stable id, shared with the `@luma/module-<id>` frontend package. */
  id: string;
  name: string;
  version: string;
  description?: string;
  dependsOn?: Dependency[];
  /** Soft dependencies: ordered first when present, but not required. */
  optionalDependsOn?: Dependency[];
  /** Capability dependencies, satisfied by any providing module. */
  requires?: CapabilityReq[];
  provides?: Capability[];
  /** Account capabilities needed to use the module. */
  permissions?: string[];
  /** Admin-configurable settings. */
  config?: ConfigField[];
  /** Present when the module ships a runtime-loaded frontend remote. */
  feRemote?: FeRemote;
  /** Admin enabled state (from GET /api/modules). A disabled module is hidden. */
  enabled?: boolean;
}
