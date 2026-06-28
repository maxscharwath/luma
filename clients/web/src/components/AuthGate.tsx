// The login gate. Rendered as a full-screen overlay by the root layout whenever
// no session is active, so the catalogue underneath is never usable until a real
// account is chosen. Visual design follows LUMA.dc.html's "Qui regarde ?" screen
// (rounded-square gradient avatars, Bricolage headings) while keeping real
// account semantics: selecting a profile asks for its password, and new accounts
// are created with email + username + password + an optional uploaded avatar.

import type { PublicUser } from '@luma/core';
import { Logo, useT } from '@luma/ui';
import { IconLock, IconPlus } from '@tabler/icons-react';
import { useLocation } from '@tanstack/react-router';
import { useEffect, useRef, useState } from 'react';
import { avatarGradient, initials, UserAvatar } from '#web/components/UserAvatar';
import { useAuth } from '#web/lib/auth';

type Mode = { kind: 'pick' } | { kind: 'login'; user: PublicUser | null } | { kind: 'register' };

const INPUT =
  'w-full rounded-md border border-border-strong bg-surface-2 px-4 py-3.5 text-[15px] text-text outline-none transition-colors placeholder:text-dim focus:border-accent';

const RADIAL = 'radial-gradient(120% 90% at 50% 0%, #15131C, #0A0A0C 70%)';

export function AuthGate() {
  const { user, ready } = useAuth();
  const { pathname } = useLocation();

  // Logged in → the gate is invisible and the app shows through.
  if (ready && user) return null;
  // The public join page (invitees aren't users yet) must not be gated.
  if (pathname === '/join') return null;

  return (
    <div
      className="fixed inset-0 z-100 flex flex-col items-center justify-center overflow-y-auto px-6 py-12"
      style={{ background: RADIAL }}
    >
      <Brand />
      {ready ? <GateBody /> : <Spinner />}
    </div>
  );
}

function Brand() {
  return (
    <div className="mb-12 flex items-center gap-2.5">
      <Logo markOnly size={30} />
      <span className="font-display text-[24px] font-extrabold tracking-[.16em]">LUMA</span>
    </div>
  );
}

function Spinner() {
  return (
    <div className="h-10 w-10 animate-spin rounded-full border-[3px] border-white/15 border-t-accent" />
  );
}

function GateBody() {
  const t = useT();
  const { client, accounts, login, register, activate, forget } = useAuth();
  const [profiles, setProfiles] = useState<PublicUser[]>([]);
  const [mode, setMode] = useState<Mode>({ kind: 'pick' });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load the existing profiles for the picker (public, no token needed).
  useEffect(() => {
    let cancelled = false;
    client
      .users()
      .then((u) => {
        if (cancelled) return;
        setProfiles(u);
        // Fresh install (no accounts yet) → go straight to registration.
        if (u.length === 0) setMode({ kind: 'register' });
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [client]);

  function fail(e: unknown, fallback: string) {
    setError(
      e instanceof Error && /401|invalid|identifiants/i.test(e.message)
        ? t('auth.invalidCredentials')
        : fallback,
    );
  }

  async function doLogin(identifier: string, password: string) {
    setBusy(true);
    setError(null);
    try {
      await login(identifier, password);
    } catch (e) {
      fail(e, t('auth.loginFailed'));
    } finally {
      setBusy(false);
    }
  }

  async function doRegister(
    email: string,
    username: string,
    password: string,
    avatar: File | null,
  ) {
    setBusy(true);
    setError(null);
    try {
      await register(email, username, password, avatar);
    } catch (e) {
      setError(
        e instanceof Error && /409|déjà|exist/i.test(e.message)
          ? t('auth.emailTaken')
          : t('auth.registerFailed'),
      );
    } finally {
      setBusy(false);
    }
  }

  if (mode.kind === 'login') {
    return (
      <LoginForm
        profile={mode.user}
        busy={busy}
        error={error}
        onBack={() => {
          setError(null);
          setMode({ kind: 'pick' });
        }}
        onSubmit={doLogin}
      />
    );
  }

  if (mode.kind === 'register') {
    return (
      <RegisterForm
        busy={busy}
        error={error}
        canGoBack={profiles.length > 0}
        onBack={() => {
          setError(null);
          setMode({ kind: 'pick' });
        }}
        onSubmit={doRegister}
      />
    );
  }

  // --- picker ---
  return (
    <>
      <h1 className="mb-12 font-display text-[40px] font-semibold">{t('auth.whoWatching')}</h1>
      <div className="flex flex-wrap items-start justify-center gap-9">
        {profiles.map((p) => {
          // Already signed-in on this device → one-tap switch, no password.
          const remembered = accounts.find((a) => a.user.id === p.id);
          return (
            <div key={p.id} className="flex w-37.5 flex-col items-center gap-2">
              <button
                type="button"
                onClick={() => {
                  setError(null);
                  if (remembered) activate(remembered);
                  else setMode({ kind: 'login', user: p });
                }}
                className="group flex flex-col items-center gap-4 transition-transform duration-200 hover:-translate-y-1.5 focus:outline-none"
              >
                <div className="relative rounded-[18px] ring-accent transition-shadow duration-200 group-hover:shadow-[0_0_0_4px_var(--luma-accent),0_16px_40px_rgba(0,0,0,.5)] group-focus-visible:shadow-[0_0_0_4px_var(--luma-accent),0_16px_40px_rgba(0,0,0,.5)]">
                  <UserAvatar name={p.username} avatarUrl={p.avatarUrl} seed={p.id} size={138} />
                  {remembered ? null : (
                    <span
                      className="absolute bottom-2 right-2 flex h-7 w-7 items-center justify-center rounded-full bg-black/70 text-white/85"
                      title={t('auth.passwordRequired')}
                    >
                      <IconLock size={14} stroke={2} />
                    </span>
                  )}
                </div>
                <span className="text-[18px] font-medium text-text/78">{p.username}</span>
              </button>
              {remembered ? (
                <button
                  type="button"
                  onClick={() => forget(p.id)}
                  className="text-[12px] font-medium text-dim transition-colors hover:text-text"
                >
                  {t('auth.logout')}
                </button>
              ) : null}
            </div>
          );
        })}
      </div>

      <button
        type="button"
        onClick={() => setMode({ kind: 'login', user: null })}
        className="mt-14 rounded-lg border border-white/20 px-5 py-2.5 text-[13px] font-semibold uppercase tracking-widest text-text/70 transition-colors hover:border-accent hover:text-accent"
      >
        {t('auth.loginEmail')}
      </button>
    </>
  );
}

function LoginForm({
  profile,
  busy,
  error,
  onBack,
  onSubmit,
}: Readonly<{
  profile: PublicUser | null;
  busy: boolean;
  error: string | null;
  onBack: () => void;
  onSubmit: (identifier: string, password: string) => void;
}>) {
  const t = useT();
  const [identifier, setIdentifier] = useState(profile?.username ?? '');
  const [password, setPassword] = useState('');

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        if (identifier.trim() && password) onSubmit(identifier.trim(), password);
      }}
      className="flex w-full max-w-95 flex-col items-center gap-5"
    >
      {profile ? (
        <UserAvatar
          name={profile.username}
          avatarUrl={profile.avatarUrl}
          seed={profile.id}
          size={96}
        />
      ) : null}
      <h1 className="font-display text-[28px] font-semibold">
        {profile ? profile.username : t('auth.signinTitle')}
      </h1>

      {profile ? null : (
        <input
          className={INPUT}
          placeholder={t('auth.emailOrUsername')}
          autoComplete="username"
          value={identifier}
          onChange={(e) => setIdentifier(e.target.value)}
          autoFocus
        />
      )}
      <input
        className={INPUT}
        type="password"
        placeholder={t('auth.password')}
        autoComplete="current-password"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        // eslint-disable-next-line jsx-a11y/no-autofocus
        autoFocus={Boolean(profile)}
      />

      {error ? <p className="text-[13px] font-medium text-danger">{error}</p> : null}

      <button
        type="submit"
        disabled={busy || !password}
        className="mt-1 w-full rounded-md bg-accent py-3.5 text-[15px] font-bold text-accent-ink transition-colors hover:bg-accent-hover disabled:opacity-50"
      >
        {busy ? t('auth.loggingIn') : t('auth.login')}
      </button>
      <button
        type="button"
        onClick={onBack}
        className="text-[14px] font-medium text-muted hover:text-text"
      >
        ← {t('common.back')}
      </button>
    </form>
  );
}

function RegisterForm({
  busy,
  error,
  canGoBack,
  onBack,
  onSubmit,
}: Readonly<{
  busy: boolean;
  error: string | null;
  canGoBack: boolean;
  onBack: () => void;
  onSubmit: (email: string, username: string, password: string, avatar: File | null) => void;
}>) {
  const t = useT();
  const [email, setEmail] = useState('');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [avatar, setAvatar] = useState<File | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  // Revoke the object URL when the preview changes / unmounts.
  useEffect(() => {
    return () => {
      if (preview) URL.revokeObjectURL(preview);
    };
  }, [preview]);

  function pickFile(f: File | null) {
    if (preview) URL.revokeObjectURL(preview);
    setAvatar(f);
    setPreview(f ? URL.createObjectURL(f) : null);
  }

  const valid = email.includes('@') && username.trim().length > 0 && password.length >= 4;

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        if (valid) onSubmit(email.trim(), username.trim(), password, avatar);
      }}
      className="flex w-full max-w-95 flex-col items-center gap-5"
    >
      <h1 className="font-display text-[28px] font-semibold">{t('auth.newAccount')}</h1>

      {/* Avatar upload — click the tile to choose an image. */}
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
        {busy ? t('auth.creating') : t('auth.createAccount')}
      </button>
      {canGoBack ? (
        <button
          type="button"
          onClick={onBack}
          className="text-[14px] font-medium text-muted hover:text-text"
        >
          ← {t('common.back')}
        </button>
      ) : null}
    </form>
  );
}
