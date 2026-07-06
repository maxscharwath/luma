import { hasPermission } from '@luma/core';
import { useT } from '@luma/ui';
import { createFileRoute, Outlet } from '@tanstack/react-router';
import { AdminLayout } from '#web/features/admin/shell';
import { useAuth } from '#web/shared/lib/auth';

// Admin console layout + permission gate. Any management capability
// (users/library/settings) unlocks the console; pages further gate their writes.
export const Route = createFileRoute('/admin')({
  component: AdminRoute,
});

function AdminRoute() {
  const t = useT();
  const { user, ready } = useAuth();
  // Signed-out users see the global <AuthGate> login overlay (from __root).
  if (!ready || !user) return null;

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
