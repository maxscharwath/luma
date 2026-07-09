// The dedicated /login route. The app normally gates auth with an overlay
// (`AuthGate`) that reveals the current page once you sign in, so no redirect is
// needed. This route is the explicit, bookmarkable entry used when we DO need to
// send someone to sign in and back e.g. the 401 error page which links here
// with `?redirect=<path>`. It renders the same gate UI and, once a session
// exists, forwards to the requested destination.

import { createFileRoute, useNavigate } from '@tanstack/react-router';
import { useEffect, useRef, useState } from 'react';
import { LoginGate } from '#web/features/accounts/auth-gate';
import { useAuth } from '#web/shared/lib/auth';

/** Accept only a safe internal path (single leading slash) as the destination
 * guards against open-redirects to another origin. */
function safeRedirect(v: unknown): string | undefined {
  return typeof v === 'string' && v.startsWith('/') && !v.startsWith('//') ? v : undefined;
}

export const Route = createFileRoute('/login')({
  validateSearch: (s: Record<string, unknown>): { redirect?: string } => ({
    redirect: safeRedirect(s.redirect),
  }),
  component: LoginPage,
});

function LoginPage() {
  const { redirect } = Route.useSearch();
  const { user, ready, switchProfile } = useAuth();
  const navigate = useNavigate();
  const didInit = useRef(false);
  // We only navigate away after a sign-in that happens ON this page. `armed` is
  // set once the session is cleared, so being already signed in on arrival never
  // triggers an immediate bounce.
  const [armed, setArmed] = useState(false);

  // Entering /login behaves like the "change profile" action: drop the current
  // session (remembered accounts are kept) so the picker shows, rather than
  // redirecting a signed-in visitor straight back out. Runs once.
  useEffect(() => {
    if (didInit.current) return;
    didInit.current = true;
    switchProfile();
  }, [switchProfile]);

  useEffect(() => {
    if (!ready) return;
    if (!armed) {
      if (!user) setArmed(true); // session cleared → ready to accept a sign-in
      return;
    }
    if (user) navigate({ to: redirect ?? '/', replace: true });
  }, [armed, ready, user, redirect, navigate]);

  return <LoginGate />;
}
