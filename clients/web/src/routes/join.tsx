import { Logo, useT } from '@luma/ui';
import { IconPlus } from '@tabler/icons-react';
import { createFileRoute, useNavigate } from '@tanstack/react-router';
import { useEffect, useRef, useState } from 'react';
import { avatarGradient, initials } from '#web/features/accounts/user-avatar';
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

const INPUT =
  'w-full rounded-md border border-border-strong bg-surface-2 px-4 py-3.5 text-[15px] text-text outline-none transition-colors placeholder:text-dim focus:border-accent';
const RADIAL = 'radial-gradient(120% 90% at 50% 0%, #15131C, #0A0A0C 70%)';

function JoinPage() {
  const t = useT();
  const { invite } = Route.useSearch();
  const { client, register } = useAuth();
  const navigate = useNavigate();

  const [status, setStatus] = useState<'checking' | 'invalid' | 'ok'>('checking');
  const [email, setEmail] = useState('');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [avatar, setAvatar] = useState<File | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

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

  useEffect(
    () => () => {
      if (preview) URL.revokeObjectURL(preview);
    },
    [preview],
  );

  function pickFile(f: File | null) {
    if (preview) URL.revokeObjectURL(preview);
    setAvatar(f);
    setPreview(f ? URL.createObjectURL(f) : null);
  }

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
      className="fixed inset-0 z-100 flex flex-col items-center justify-center overflow-y-auto px-6 py-12"
      style={{ background: RADIAL }}
    >
      <div className="mb-10 flex items-center gap-2.5">
        <Logo markOnly size={30} />
        <span className="font-display text-[24px] font-extrabold tracking-[.16em]">LUMA</span>
      </div>

      {status === 'checking' ? (
        <div className="h-10 w-10 animate-spin rounded-full border-[3px] border-white/15 border-t-accent" />
      ) : null}
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
          <h1 className="font-display text-[28px] font-semibold">{t('auth.joinLuma')}</h1>

          <button
            type="button"
            onClick={() => fileRef.current?.click()}
            className="group relative h-28 w-28 overflow-hidden rounded-xl focus:outline-none"
            aria-label={t('auth.chooseAvatar')}
          >
            {preview ? (
              <img src={preview} alt="" className="h-full w-full object-cover" />
            ) : (
              <div
                className="flex h-full w-full items-center justify-center text-white/85"
                style={{ background: avatarGradient(username || email || 'new') }}
              >
                {username.trim() ? (
                  <span className="font-display text-[40px] font-bold">{initials(username)}</span>
                ) : (
                  <IconPlus size={34} stroke={1.6} />
                )}
              </div>
            )}
            <span className="absolute inset-x-0 bottom-0 bg-black/55 py-1 text-center text-[11px] font-semibold text-white opacity-0 transition-opacity group-hover:opacity-100">
              {t('common.photo')}
            </span>
          </button>
          <input
            ref={fileRef}
            type="file"
            accept="image/*"
            className="hidden"
            onChange={(e) => pickFile(e.target.files?.[0] ?? null)}
          />

          <input
            className={INPUT}
            type="email"
            placeholder={t('auth.email')}
            autoComplete="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
          />
          <input
            className={INPUT}
            placeholder={t('auth.username')}
            autoComplete="nickname"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
          />
          <input
            className={INPUT}
            type="password"
            placeholder={t('auth.passwordHint')}
            autoComplete="new-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />

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
    </main>
  );
}
