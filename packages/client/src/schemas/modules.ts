// Module registry wire types (`GET /api/modules`). Each running module reports
// its admin `enabled` flag and the capabilities it provides; an engine capability
// (`download-client`, `indexer-engine`, ...) carries the add-form schema the admin
// console renders, so the ADD flows are data-driven (disabling a module hides its
// add-UI; adding an engine needs no frontend change).
//
// Hand-authored (no ts-rs binding): mirrors `Capability` / `ConfigField` in the
// Rust `luma-module-sdk` crate. Kept as plain interfaces since these are read-only
// response DTOs the client never validates or sends.

/** One field in an engine's add-form (mirrors the Rust `ConfigField`). */
export interface EngineField {
  key: string;
  /** i18n key (or literal) for the field label. */
  label: string;
  /** How the value is entered / interpreted. */
  type: 'string' | 'bool' | 'number' | 'select';
  default?: string;
  /** Choices for `type === 'select'`. */
  options?: string[];
  placeholder?: string;
  /** Render as a password input; the value is write-only. */
  secret?: boolean;
  /** Must be non-empty before the form can submit. */
  required?: boolean;
}

/** One capability a module provides, as a (`kind`, `id`) pair. Engine capabilities
 * with an add-flow also carry a display `label` plus either `fields` (a plain form)
 * or a custom `flow` discriminator the host page renders itself. */
export interface EngineCapability {
  kind: string;
  id: string;
  label?: string;
  fields?: EngineField[];
  flow?: string;
}

/** One module from `GET /api/modules`: its manifest identity, admin `enabled` flag
 * (default true), and the capabilities it provides. */
export interface ModuleInfo {
  id: string;
  name: string;
  enabled?: boolean;
  provides?: EngineCapability[];
}
