// Full-page error / not-found screens, wired into the router as
// `defaultErrorComponent` (thrown errors → 401 / 403 / 500) and
// `defaultNotFoundComponent` (unmatched routes → 404). Styled to the KROMA
// design: deep charcoal, a single amber accent, a big cinematic status number.

import { apiErrorText, KromaApiError, type MessageKey } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconHome, IconLogin, IconRefresh } from '@tabler/icons-react';
import { useNavigate, useRouter, useRouterState } from '@tanstack/react-router';
import { Button, Logo } from '#web/shared/ui';

type Kind = 'notFound' | 'unauthorized' | 'forbidden' | 'server';

const COPY: Record<Kind, { code: string; title: MessageKey; body: MessageKey }> = {
  notFound: { code: '404', title: 'error.notFoundTitle', body: 'error.notFoundBody' },
  unauthorized: { code: '401', title: 'error.unauthorizedTitle', body: 'error.unauthorizedBody' },
  forbidden: { code: '403', title: 'error.forbiddenTitle', body: 'error.forbiddenBody' },
  server: { code: '500', title: 'error.serverTitle', body: 'error.serverBody' },
};

/** Map a thrown value to a screen variant via its HTTP status (if any). */
function kindOf(error: unknown): Kind {
  const status = error instanceof KromaApiError ? error.status : undefined;
  if (status === 404) return 'notFound';
  if (status === 401) return 'unauthorized';
  if (status === 403) return 'forbidden';
  return 'server';
}

const RADIAL = 'radial-gradient(120% 90% at 50% 0%, #15131C, #0A0A0C 70%)';

function ErrorScreen({
  kind,
  detail,
  onRetry,
  onSignIn,
}: Readonly<{
  kind: Kind;
  detail?: string | null;
  onRetry?: () => void;
  onSignIn?: () => void;
}>) {
  const t = useT();
  const router = useRouter();
  const { code, title, body } = COPY[kind];

  return (
    <main
      className="flex min-h-screen w-full flex-col items-center justify-center px-6 py-16 text-center"
      style={{ background: RADIAL }}
    >
      <div className="flex w-full max-w-[440px] flex-col items-center">
        <div className="mb-8 opacity-90">
          <Logo size={20} />
        </div>

        {/* Big cinematic status number with an amber-tinted glow. */}
        <div
          className="font-display text-[104px] font-extrabold leading-none tracking-[-.04em] text-transparent"
          style={{
            backgroundImage: 'linear-gradient(180deg, #F4F3F0 0%, rgba(244,243,240,.28) 100%)',
            WebkitBackgroundClip: 'text',
            backgroundClip: 'text',
          }}
        >
          {code}
        </div>

        <h1 className="mt-6 font-display text-[24px] font-bold tracking-[-.02em]">{t(title)}</h1>
        <p className="mt-3 max-w-[380px] text-[14.5px] leading-relaxed text-muted">{t(body)}</p>

        {detail ? (
          <p className="mt-4 max-w-[380px] break-words rounded-md border border-border bg-surface-1 px-3.5 py-2.5 text-[12.5px] font-medium text-dim">
            {detail}
          </p>
        ) : null}

        <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
          {onRetry ? (
            <Button variant="glass" size="sm" icon={<IconRefresh size={16} />} onClick={onRetry}>
              {t('error.retry')}
            </Button>
          ) : null}
          {onSignIn ? (
            <Button size="sm" icon={<IconLogin size={16} />} onClick={onSignIn}>
              {t('auth.login')}
            </Button>
          ) : null}
          <Button
            variant={onSignIn ? 'glass' : 'primary'}
            size="sm"
            icon={<IconHome size={16} />}
            onClick={() => void router.navigate({ to: '/' })}
          >
            {t('error.home')}
          </Button>
        </div>
      </div>
    </main>
  );
}

/** Router `defaultErrorComponent`: a thrown error (loader/component). Picks the
 * variant from the error's status and offers a retry that re-runs the route. */
export function RouteError({ error, reset }: Readonly<{ error: Error; reset: () => void }>) {
  const router = useRouter();
  const navigate = useNavigate();
  // Where to return to after signing in (the current path + query).
  const href = useRouterState({ select: (s) => s.location.href });
  const kind = kindOf(error);

  // Server errors are often transient → offer a retry that re-runs the route.
  const onRetry =
    kind === 'server'
      ? () => {
          reset();
          void router.invalidate();
        }
      : undefined;

  // 401: the session is gone but `user` may still be cached locally, so the
  // ambient gate won't show. Send them to /login (which drops the stale session
  // and shows the picker); it returns here once they sign back in.
  const onSignIn =
    kind === 'unauthorized'
      ? () => void navigate({ to: '/login', search: { redirect: href } })
      : undefined;

  // Only surface the raw message for server errors (404/401/403 are self-evident
  // and the message would just be noise).
  const detail = kind === 'server' ? apiErrorText(error, '') || null : null;
  return <ErrorScreen kind={kind} detail={detail} onRetry={onRetry} onSignIn={onSignIn} />;
}

/** Router `defaultNotFoundComponent`: an unmatched route (404). */
export function NotFound() {
  return <ErrorScreen kind="notFound" />;
}
