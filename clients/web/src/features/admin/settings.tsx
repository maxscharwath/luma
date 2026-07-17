// Shim: the generic settings renderer now lives in `@kroma/admin-kit` as
// `SettingsView` (shared by the built-in settings pages and the VPN / Acquisition
// module pages). Re-exported as `SettingsPage` so existing call sites keep
// importing from `#web/features/admin/settings`.
export { SettingsView as SettingsPage } from '@kroma/admin-kit';
