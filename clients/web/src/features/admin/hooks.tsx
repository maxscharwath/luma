// Shim: the admin data hooks + capability helpers now live in `@luma/admin-kit`.
// `useCap` reads the current user from the kit's host context, which the admin
// shell (`AdminProvider`) mounts. Re-exported so existing call sites keep
// importing from `#web/features/admin/hooks`.
export { Denied, isAnyAdmin, useAsyncAction, useCap, usePoll } from '@luma/admin-kit';
