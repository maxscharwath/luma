// Route guard for authenticated layouts. Once the session has hydrated, it
// sends signed-out visitors to /login with a `redirect` back to where they were,
// so after signing in they land on the page they asked for. Returns the auth
// state so the layout can show a loader until it resolves (and while redirecting).

import { useNavigate, useRouterState } from '@tanstack/react-router';
import { useEffect } from 'react';
import { useAuth } from '#web/shared/lib/auth';

export function useRequireAuth(): { ready: boolean; authed: boolean } {
  const { user, ready } = useAuth();
  const navigate = useNavigate();
  // The current relative href (pathname + search) is where we return to.
  const href = useRouterState({ select: (s) => s.location.href });

  useEffect(() => {
    if (!ready || user) return;
    // While the redirect navigation settles, this layout can re-render with the
    // location ALREADY at /login; navigating again would nest the login URL into
    // its own `redirect` (login?redirect=/login?redirect=/…) once per render.
    // The first navigation carried the real destination, so just stand down.
    if (href.startsWith('/login')) return;
    navigate({ to: '/login', search: { redirect: href }, replace: true });
  }, [ready, user, href, navigate]);

  return { ready, authed: Boolean(user) };
}
