// @luma/module-sdk: the frontend module contract. A module package imports the
// types from here and exports a `LumaModule`; the host builds a `ModuleRegistry`
// and a `LumaHost`. Mirrors the Rust `luma-module-sdk` on the server.

// --- Host-app surface re-exported for module UIs -----------------------------
// A module package depends ONLY on @luma/module-sdk, never on @luma/core,
// @luma/ui or @luma/admin-kit directly. These re-exports are that facade.
// (core's `LumaEvents` SSE client is re-exported as `LumaEventStream` to avoid
// colliding with the SDK's own `LumaEvents` event-map interface above.)
// The admin UI toolkit: module pages use most of it, so re-export it wholesale.
export * from '@luma/admin-kit';
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
} from '@luma/core';
// @luma/core surface: the error formatter, the SSE client (re-exported as
// LumaEventStream to avoid the LumaEvents event-map interface above), and the
// shared DTO / view types module pages render.
export { apiErrorText, LumaEvents as LumaEventStream } from '@luma/core';
// i18n hook from @luma/ui.
export { useT } from '@luma/ui';
export type { EventBus, EventKey } from './bus';
export { createEventBus } from './bus';
export type { LumaEvents, ModuleApiRegistry } from './contracts';
export type { DefineModuleOptions, ModuleManifestInput, ModulePage } from './define';
export { defineModule, pageHref } from './define';
export type { HostApi, HostAuth, HostBase, HostI18n, HostNav, LumaHost } from './host';
export { moduleIconUrl } from './icon';
export type {
  LumaModule,
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
