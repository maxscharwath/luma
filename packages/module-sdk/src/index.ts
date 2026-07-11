// @luma/module-sdk: the frontend module contract. A module package imports the
// types from here and exports a `LumaModule`; the host builds a `ModuleRegistry`
// and a `LumaHost`. Mirrors the Rust `luma-module-sdk` on the server.
export type {
  Capability,
  CapabilityReq,
  ConfigField,
  Dependency,
  FeRemote,
  ModuleManifest,
} from './types';
export { moduleIconUrl } from './icon';
export type { ModuleApiRegistry, LumaEvents } from './contracts';
export { createEventBus } from './bus';
export type { EventBus, EventKey } from './bus';
export type { HostApi, HostAuth, HostBase, HostI18n, HostNav, LumaHost } from './host';
export type {
  LumaModule,
  ModuleComponentProps,
  NavItem,
  RouteDef,
  SettingsPanel,
} from './module';
export { ModuleRegistry } from './registry';
export type { ModuleNav, ModulePanel, ModuleRoute, ModuleStatus } from './registry';
