import { hasPermission } from '@luma/core';
import { useT } from '@luma/ui';
import { createFileRoute, Outlet } from '@tanstack/react-router';
import { GateLoading } from '#web/features/accounts/auth-gate';
import { AdminLayout } from '#web/features/admin/shell';
import { useAuth } from '#web/shared/lib/auth';
import { useRequireAuth } from '#web/shared/lib/require-auth';

// Admin console layout + permission gate. Any management capability
// (users/library/settings) unlocks the console; pages further gate their writes.
export const Route = createFileRoute('/admin')({
  component: AdminRoute,
});

function AdminRoute() {
  const t = useT();
  const { user } = useAuth();
  const { ready } = useRequireAuth();
  // Signed-out users are redirected to /login (by useRequireAuth); show a loader
  // until then. The `user` check also narrows it non-null for the checks below.
  if (!ready || !user) return <GateLoading />;

  const allowed =
    hasPermission(user, 'users.manage') ||
    hasPermission(user, 'library.manage') ||
    hasPermission(user, 'settings.manage') ||
    hasPermission(user, 'requests.manage');

  if (!allowed) {
    return (
      <main className="flex min-h-screen items-center justify-center px-6">
        <p className="text-[15px] text-muted">{t('admin.noAdminAccess')}</p>
      </main>
    );
  }

  return (
    <AdminLayout>
      <Outlet />
    </AdminLayout>
  );
}
