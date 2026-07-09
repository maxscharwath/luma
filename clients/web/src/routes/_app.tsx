// The signed-in app shell: a pathless layout route that frames every
// authenticated page with the sidebar and holds a session. Its children (the
// catalogue, search, player, account, …) render into <Outlet/>. Signed-out
// visitors are redirected to /login (with a redirect back here); public routes
// (login, join) and the admin console live outside this layout.

import { createFileRoute, Outlet } from '@tanstack/react-router';
import { GateLoading } from '#web/features/accounts/auth-gate';
import { MobileTopbar, Sidebar } from '#web/features/catalog/sidebar';
import { ensureSession, isAuthed } from '#web/shared/lib/api';
import { useRequireAuth } from '#web/shared/lib/require-auth';

export const Route = createFileRoute('/_app')({
  // Runs before any child loader (beforeLoad resolves top-down ahead of loaders):
  // exchange the stored access token for a session bearer up front so the
  // catalogue prefetch is authorised on its first try. Without this the loaders
  // race the boot exchange and 401-then-retry every request on each reload.
  beforeLoad: async () => {
    if (isAuthed()) await ensureSession();
  },
  component: AppLayout,
});

function AppLayout() {
  const { ready, authed } = useRequireAuth();
  // Hold the shell (and its per-user route fetches) until a session exists;
  // useRequireAuth redirects to /login once we know there isn't one.
  if (!(ready && authed)) return <GateLoading />;
  // Desktop (lg+): fixed 248px sidebar rail + content grid. Below lg the rail
  // is hidden and a sticky topbar (hamburger → nav drawer) takes over.
  return (
    <div className="flex min-h-screen flex-col lg:grid lg:grid-cols-[248px_minmax(0,1fr)]">
      <Sidebar />
      <MobileTopbar />
      <Outlet />
    </div>
  );
}
