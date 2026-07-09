// The signed-in app shell: a pathless layout route that frames every
// authenticated page with the sidebar and holds a session. Its children (the
// catalogue, search, player, account, …) render into <Outlet/>. Signed-out
// visitors are redirected to /login (with a redirect back here); public routes
// (login, join) and the admin console live outside this layout.

import { createFileRoute, Outlet } from '@tanstack/react-router';
import { GateLoading } from '#web/features/accounts/auth-gate';
import { Sidebar } from '#web/features/catalog/sidebar';
import { useRequireAuth } from '#web/shared/lib/require-auth';

export const Route = createFileRoute('/_app')({
  component: AppLayout,
});

function AppLayout() {
  const { ready, authed } = useRequireAuth();
  // Hold the shell (and its per-user route fetches) until a session exists;
  // useRequireAuth redirects to /login once we know there isn't one.
  if (!(ready && authed)) return <GateLoading />;
  return (
    <div className="grid min-h-screen grid-cols-[248px_minmax(0,1fr)]">
      <Sidebar />
      <Outlet />
    </div>
  );
}
