import { Logo, useT } from '@kroma/ui';
import { createFileRoute, useNavigate } from '@tanstack/react-router';
import { useEffect, useState } from 'react';
import { RegisterFields, type RegisterValues } from '#web/features/accounts/auth-fields';
import { Spinner } from '#web/features/accounts/auth-gate';
import { useAuth } from '#web/shared/lib/auth';

// Public invitation acceptance page. An admin (with `users.manage`) shares
// `/join?invite=TOKEN`; the invitee creates their account here. The global
// AuthGate is bypassed on this path so a not-yet-user can reach it.
export const Route = createFileRoute('/join')({
  validateSearch: (s: Record<string, unknown>): { invite?: string } => ({
    invite: typeof s.invite === 'string' ? s.invite : undefined,
  }),
  component: JoinPage,
});

const RADIAL = 'radial-gradient(120% 90% at 50% 0%, #15131C, #0A0A0C 70%)';

function JoinPage() {
  const t = useT();
  const { invite } = Route.useSearch();
  const { client, register } = useAuth();
  const navigate = useNavigate();

  const [status, setStatus] = useState<'checking' | 'invalid' | 'ok'>('checking');
  const [values, setValues] = useState<RegisterValues>({ email: '', username: '', password: '' });
  const [avatar, setAvatar] = useState<File | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { email, username, password } = values;

  // Validate the token up front.
  useEffect(() => {
    if (!invite) {
      setStatus('invalid');
      return;
    }
    let cancelled = false;
    client
      .checkInvite(invite)
      .then((r) => {
        if (!cancelled) setStatus(r.valid ? 'ok' : 'invalid');
      })
      .catch(() => {
        if (!cancelled) setStatus('invalid');
      });
    return () => {
      cancelled = true;
    };
  }, [client, invite]);

  const valid = email.includes('@') && username.trim().length > 0 && password.length >= 4;

  async function submit() {
    if (!valid || !invite) return;
    setBusy(true);
    setError(null);
    try {
      await register(email.trim(), username.trim(), password, avatar, invite);
      // register() signs us in and invalidates the router, so a plain navigation
      // home lands in the (now authenticated) app shell no full reload needed.
      void navigate({ to: '/' });
    } catch (e) {
      let msg = t('auth.registerFailed');
      if (e instanceof Error && /403|invalid|expir/i.test(e.message)) msg = t('auth.inviteInvalid');
      else if (e instanceof Error && /409|déjà|exist/i.test(e.message)) msg = t('auth.emailTaken');
      setError(msg);
    } finally {
      setBusy(false);
    }
  }

  return (
    <main
      className="fixed inset-0 z-100 flex flex-col overflow-y-auto px-6 py-12"
      style={{ background: RADIAL }}
    >
      {/* Auto margins (not justify-center) so a form taller than a small phone
          viewport scrolls instead of clipping its top. */}
      <div className="m-auto flex w-full flex-col items-center">
        <div className="mb-10">
          <Logo size={24} />
        </div>

        {status === 'checking' ? <Spinner /> : null}
        {status === 'invalid' ? (
          <div className="text-center">
            <h1 className="mb-2 font-display text-[28px] font-bold">
              {t('auth.inviteInvalidTitle')}
            </h1>
            <p className="text-[14px] text-muted">{t('auth.inviteInvalidDesc')}</p>
          </div>
        ) : null}
        {status === 'ok' ? (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void submit();
            }}
            className="flex w-full max-w-95 flex-col items-center gap-5"
          >
            <h1 className="font-display text-[28px] font-semibold">{t('auth.joinKroma')}</h1>

            <RegisterFields values={values} onChange={setValues} onAvatar={setAvatar} />

            {error ? <p className="text-[13px] font-medium text-danger">{error}</p> : null}

            <button
              type="submit"
              disabled={busy || !valid}
              className="mt-1 w-full rounded-md bg-accent py-3.5 text-[15px] font-bold text-accent-ink transition-colors hover:bg-accent-hover disabled:opacity-50"
            >
              {busy ? t('auth.creating') : t('auth.createMyAccount')}
            </button>
          </form>
        ) : null}
      </div>
    </main>
  );
}
