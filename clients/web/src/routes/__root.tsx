import { createRootRoute, HeadContent, Scripts, useRouterState } from '@tanstack/react-router';
import type { ReactNode } from 'react';
import { AuthGate } from '#web/features/accounts/auth-gate';
import { Intro } from '#web/features/catalog/intro';
import { Sidebar } from '#web/features/catalog/sidebar';
import { AuthProvider, useAuth } from '#web/shared/lib/auth';
import { LocaleProvider } from '#web/shared/lib/locale';
import { MyListProvider } from '#web/shared/lib/mylist';
import { WatchedProvider } from '#web/shared/lib/watched';
import appCss from '#web/styles.css?url';

export const Route = createRootRoute({
  // No apiBase injection: the SPA resolves the API origin at runtime (same origin
  // in the packaged build, VITE_LUMA_SERVER in dev see lib/api `apiBase`).
  head: () => ({
    meta: [
      { charSet: 'utf-8' },
      { name: 'viewport', content: 'width=device-width, initial-scale=1' },
      { title: 'LUMA' },
    ],
    links: [{ rel: 'stylesheet', href: appCss }],
  }),
  shellComponent: RootDocument,
});

function RootDocument({ children }: Readonly<{ children: ReactNode }>) {
  return (
    // `lang` is the SSR default; LocaleProvider updates it client-side to match
    // the active locale (account preference → device → browser).
    <html lang="fr">
      <head>
        <HeadContent />
      </head>
      <body className="bg-bg text-text">
        <AuthProvider>
          <WatchedProvider>
            <MyListProvider>
              <LocaleProvider>
                <AuthGate />
                <AppShell>{children}</AppShell>
              </LocaleProvider>
            </MyListProvider>
          </WatchedProvider>
        </AuthProvider>
        {/* Brand intro overlay sits above everything, plays once per session. */}
        <Intro />
        <Scripts />
      </body>
    </html>
  );
}

/** Renders the routed app, but only once a session exists so per-user route
 * components (which fetch auth-gated data on mount) never run behind the login
 * gate. Public pages the gate lets through (e.g. `/join`) still render signed
 * out. The gate overlay covers the empty signed-out state. */
function AppShell({ children }: Readonly<{ children: ReactNode }>) {
  const { user, ready } = useAuth();
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const isAdmin = pathname.startsWith('/admin');
  const isPublic = pathname === '/join';

  if (!isPublic && !(ready && user)) return null;
  // The admin console brings its own full-screen sidebar, so it escapes the
  // main app's two-column grid.
  if (isAdmin) return <>{children}</>;
  return (
    <div className="grid min-h-screen grid-cols-[248px_minmax(0,1fr)]">
      <Sidebar />
      {children}
    </div>
  );
}
