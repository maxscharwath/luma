// @kroma/module-sdk: the frontend module contract. A module package imports the
// types from here and exports a `KromaModule`; the host builds a `ModuleRegistry`
// and a `KromaHost`. Mirrors the Rust `kroma-module-sdk` on the server.

// --- Host-app surface re-exported for module UIs -----------------------------
// A module package depends ONLY on @kroma/module-sdk, never on @kroma/core,
// @kroma/ui or @kroma/admin-kit directly. These re-exports are that facade.
// (core's `KromaEvents` SSE client is re-exported as `KromaEventStream` to avoid
// colliding with the SDK's own `KromaEvents` event-map interface above.)
// The admin UI toolkit: module pages use most of it, so re-export it wholesale.
export * from '@kroma/admin-kit';
export type {
  ClientTestResult,
  DownloadClientView,
  DownloadView,
  EngineCapability,
  IndexerDefinitionDetailView,
  IndexerDefinitionView,
  IndexerTestResult,
  IndexerView,
  ManualReleaseView,
  MessageKey,
  RemoteAccessView,
  SaveDownloadClientBody,
  SaveIndexerBody,
  TorrentAnalysis,
  TorrentFileView,
  VpnTestResult,
} from '@kroma/core';
// @kroma/core surface: the error formatter, the SSE client (re-exported as
// KromaEventStream to avoid the KromaEvents event-map interface above), and the
// shared DTO / view types module pages render.
export { apiErrorText, KromaEvents as KromaEventStream } from '@kroma/core';
// i18n hook from @kroma/ui.
export { useT } from '@kroma/ui';
export type { EventBus, EventKey } from './bus';
export { createEventBus } from './bus';
export type { KromaEvents, ModuleApiRegistry } from './contracts';
export type { DefineModuleOptions, ModuleManifestInput, ModulePage } from './define';
export { defineModule, pageHref } from './define';
export type { HostApi, HostAuth, HostBase, HostI18n, HostNav, KromaHost } from './host';
export { moduleIconUrl } from './icon';
export type {
  KromaModule,
  ModuleComponentProps,
  NavItem,
  RouteDef,
  SettingsPanel,
} from './module';
export type { ModuleNav, ModulePanel, ModuleRoute, ModuleStatus } from './registry';
export { depEntries, ModuleRegistry } from './registry';
export type {
  Capability,
  CapabilityReq,
  ConfigField,
  Dependencies,
  Dependency,
  DependencyMap,
  FeRemote,
  ModuleManifest,
} from './types';
