import { useT } from '@luma/ui';
import { createFileRoute } from '@tanstack/react-router';
import { useState } from 'react';
import { useAuth } from '#web/shared/lib/auth';
import { Otp } from '#web/shared/ui';

// "Connecter un appareil" the approver side of Quick Connect. A TV shows a
// short code (or a QR pointing here with `?code=`); a signed-in user enters it
// to grant that device a session for their account. The global AuthGate already
// ensures the user is logged in before this page is usable.
export const Route = createFileRoute('/connect')({
  validateSearch: (s: Record<string, unknown>): { code?: string } => ({
    code: typeof s.code === 'string' ? s.code : undefined,
  }),
  component: ConnectPage,
});

function ConnectPage() {
  const t = useT();
  const { code: initial } = Route.useSearch();
  const { client, user } = useAuth();
  const [code, setCode] = useState(initial ?? '');
  const [status, setStatus] = useState<'idle' | 'ok' | 'err'>('idle');
  const [busy, setBusy] = useState(false);

  async function submit(value?: string) {
    const c = (value ?? code).trim();
    if (!c) return;
    setBusy(true);
    setStatus('idle');
    try {
      await client.quickConnectAuthorize(c);
      setStatus('ok');
    } catch {
      setStatus('err');
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="flex min-h-screen items-center justify-center px-6 py-16">
      <div className="w-full max-w-105 rounded-2xl border border-border bg-surface-1 p-8 text-center shadow-card">
        <h1 className="mb-2 font-display text-[26px] font-bold">{t('connect.title')}</h1>
        <p className="mb-7 text-[14px] leading-relaxed text-muted">
          {user ? t('connect.codePromptForUser', { name: user.username }) : t('connect.codePrompt')}
        </p>

        {status === 'ok' ? (
          <div className="rounded-xl border border-success/40 bg-success/10 px-4 py-6">
            <div className="mb-1 text-[40px]">✓</div>
            <div className="font-display text-[18px] font-bold text-text">
              {t('connect.connected')}
            </div>
            <p className="mt-1 text-[13px] text-muted">{t('connect.willConnectSoon')}</p>
          </div>
        ) : (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void submit();
            }}
            className="flex flex-col items-center gap-4"
          >
            <Otp
              value={code}
              onChange={(v) => {
                setCode(v);
                setStatus('idle');
              }}
              onComplete={(v) => void submit(v)}
              autoFocus
              disabled={busy}
              ariaLabel={t('connect.title')}
            />
            {status === 'err' ? (
              <p className="text-[13px] font-medium text-danger">{t('connect.invalidCode')}</p>
            ) : null}
            <button
              type="submit"
              disabled={busy || code.trim().length < 4}
              className="w-full rounded-md bg-accent py-3.5 text-[15px] font-bold text-accent-ink transition-colors hover:bg-accent-hover disabled:opacity-50"
            >
              {busy ? t('auth.loggingIn') : t('connect.authorize')}
            </button>
          </form>
        )}
      </div>
    </main>
  );
}
