// The wire shape published by the backend at GET /api/modules. Mirrors the Rust
// `luma_module_sdk::ModuleManifest` (serialized camelCase). The frontend reads
// this to learn which backend modules are active and to reconcile them against
// the frontend modules registered in the host.

/** One capability a backend module provides. `kind`+`id` are the interface and
 *  implementation; engine capabilities (download-client, indexer-engine) may also
 *  carry UI metadata so the admin's add-picker is data-driven. */
export interface Capability {
  kind: string;
  id: string;
  /** Display name shown in the add-picker (engine capabilities only). */
  label?: string;
  /** The add-form schema the admin renders for this engine. */
  fields?: ConfigField[];
  /** Discriminator for a non-form add-flow (e.g. `"definition"` for the native
   *  Cardigann picker); the host page renders that flow itself. */
  flow?: string;
}

/** One entry in the legacy array dependency form: a bare id, an `"id@range"`
 *  string, or an object with a semver range. */
export type Dependency = string | { id: string; version?: string };

/** The package.json-style dependency form: a map of module id to semver range
 *  (a bare `"*"` means any version). This is what the backend now serializes;
 *  the array form is still accepted for older manifests. Version ranges are
 *  enforced on the backend; the frontend registry uses only the id for setup
 *  ordering. */
export type DependencyMap = Record<string, string>;

/** Either dependency form a manifest may carry. */
export type Dependencies = DependencyMap | Dependency[];

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
  /** Placeholder text for a text/URL input. */
  placeholder?: string;
  /** Render as a password input; the value is treated write-only. */
  secret?: boolean;
  /** The field must be non-empty before the form can submit. */
  required?: boolean;
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
