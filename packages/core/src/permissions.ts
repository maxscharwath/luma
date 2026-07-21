// The single source of truth for the capability picker: every grantable
// permission with its i18n label + hint. Clients render from this array, so
// adding a permission is a one-line change here (plus its two i18n keys and
// the server-side enum) rather than editing every invite / user-edit screen.

import type { Permission } from '@kroma/client';
import type { MessageKey } from './i18n';

export interface PermissionMeta {
  key: Permission;
  labelKey: MessageKey;
  hintKey: MessageKey;
}

/** All grantable permissions, in display order. Keep in sync with the Rust
 * `Permission` enum (`server/src/domain/accounts.rs`). */
export const PERMISSIONS: readonly PermissionMeta[] = [
  { key: 'playback', labelKey: 'admin.permPlayback', hintKey: 'admin.permPlaybackHint' },
  { key: 'library.manage', labelKey: 'admin.permLibrary', hintKey: 'admin.permLibraryHint' },
  { key: 'users.manage', labelKey: 'admin.permUsers', hintKey: 'admin.permUsersHint' },
  { key: 'settings.manage', labelKey: 'admin.permSettings', hintKey: 'admin.permSettingsHint' },
  {
    key: 'requests.create',
    labelKey: 'admin.permRequestCreate',
    hintKey: 'admin.permRequestCreateHint',
  },
  {
    key: 'requests.manage',
    labelKey: 'admin.permRequestManage',
    hintKey: 'admin.permRequestManageHint',
  },
  { key: 'requests.auto', labelKey: 'admin.permRequestAuto', hintKey: 'admin.permRequestAutoHint' },
  {
    key: 'reports.manage',
    labelKey: 'admin.permReportsManage',
    hintKey: 'admin.permReportsManageHint',
  },
];
