import type { QueryClient } from '@tanstack/react-query';
import { QueryClientProvider } from '@tanstack/react-query';
import { createRootRouteWithContext, HeadContent, Scripts } from '@tanstack/react-router';
import { lazy, type ReactNode, Suspense } from 'react';
import { Intro } from '#web/features/catalog/intro';
import { ModuleHostProvider } from '#web/modules/ModuleHostProvider';
import { AuthProvider } from '#web/shared/lib/auth';
import { LocaleProvider } from '#web/shared/lib/locale';
import { MyListProvider } from '#web/shared/lib/mylist';
import { queryClient } from '#web/shared/lib/query';
import { WatchedProvider } from '#web/shared/lib/watched';
import appCss from '#web/styles.css?url';

// Dev-only: lazy so the devtools bundle never ships in the packaged SPA.
const ReactQueryDevtools = import.meta.env.DEV
  ? lazy(() =>
      import('@tanstack/react-query-devtools').then((m) => ({ default: m.ReactQueryDevtools })),
    )
  : () => null;

// The router context every route (incl. loaders) receives; carries the shared
// TanStack Query client for `ensureQueryData` prefetch.
export interface RouterContext {
  queryClient: QueryClient;
}

export const Route = createRootRouteWithContext<RouterContext>()({
  // No apiBase injection: the SPA resolves the API origin at runtime (same origin
  // in the packaged build, VITE_KROMA_SERVER in dev see lib/api `apiBase`).
  head: () => ({
    meta: [
      { charSet: 'utf-8' },
      // viewport-fit=cover extends the canvas under the iPhone notch/home
      // indicator so `env(safe-area-inset-*)` paddings (player, topbar) apply.
      { name: 'viewport', content: 'width=device-width, initial-scale=1, viewport-fit=cover' },
      { title: 'KROMA' },
    ],
    links: [
      { rel: 'stylesheet', href: appCss },
      // The chromatic-wheel symbol; SVG first, PNG fallback for Safari & co.
      { rel: 'icon', type: 'image/svg+xml', href: '/favicon.svg' },
      { rel: 'icon', type: 'image/png', sizes: '32x32', href: '/favicon-32.png' },
      { rel: 'apple-touch-icon', href: '/apple-touch-icon.png' },
    ],
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
        <QueryClientProvider client={queryClient}>
          <AuthProvider>
            <WatchedProvider>
              <MyListProvider>
                {/* Each route picks its own frame: the `_app` layout wraps
                    authenticated pages in the sidebar shell + login gate; login,
                    join and the admin console own their chrome. ModuleHostProvider
                    runs the module runtime app-wide so both the main and admin
                    shells can read module-contributed nav + pages. */}
                <LocaleProvider>
                  <ModuleHostProvider>{children}</ModuleHostProvider>
                </LocaleProvider>
              </MyListProvider>
            </WatchedProvider>
          </AuthProvider>
          {/* Brand intro overlay sits above everything, plays once per session. */}
          <Intro />
          <Suspense fallback={null}>
            <ReactQueryDevtools buttonPosition="bottom-left" />
          </Suspense>
        </QueryClientProvider>
        <Scripts />
      </body>
    </html>
  );
}
