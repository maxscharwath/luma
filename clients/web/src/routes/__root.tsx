import { createRootRoute, HeadContent, Scripts, useRouterState } from '@tanstack/react-router';
import type { ReactNode } from 'react';
import { AuthGate } from '#web/components/AuthGate';
import { Intro } from '#web/components/Intro';
import { Sidebar } from '#web/components/Sidebar';
import { AuthProvider } from '#web/lib/auth';
import { LocaleProvider } from '#web/lib/locale';
import appCss from '#web/styles.css?url';

export const Route = createRootRoute({
  // No apiBase injection: the SPA resolves the API origin at runtime (same origin
  // in the packaged build, VITE_LUMA_SERVER in dev — see lib/api `apiBase`).
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
  // The admin console (/admin/*) brings its own full-screen sidebar, so it
  // escapes the main app's two-column grid.
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const isAdmin = pathname.startsWith('/admin');
  return (
    // `lang` is the SSR default; LocaleProvider updates it client-side to match
    // the active locale (account preference → device → browser).
    <html lang="fr">
      <head>
        <HeadContent />
      </head>
      <body className="bg-bg text-text">
        <AuthProvider>
          <LocaleProvider>
            <AuthGate />
            {isAdmin ? (
              children
            ) : (
              <div className="grid min-h-screen grid-cols-[248px_minmax(0,1fr)]">
                <Sidebar />
                {children}
              </div>
            )}
          </LocaleProvider>
        </AuthProvider>
        {/* Brand intro overlay — sits above everything, plays once per session. */}
        <Intro />
        <Scripts />
      </body>
    </html>
  );
}
