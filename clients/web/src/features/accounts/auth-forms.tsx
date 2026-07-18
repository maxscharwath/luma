// The sign-in and registration forms used by the login gate. Split out of
// `AuthGate.tsx`, which owns the gate/routing + profile picker and composes
// these two screens.

import { isEmail, isPassword, isUsername, type PublicUser } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconInfoCircle, IconKey } from '@tabler/icons-react';
import { useState } from 'react';
import { INPUT, RegisterFields, type RegisterValues } from '#web/features/accounts/auth-fields';
import { UserAvatar } from '#web/features/accounts/user-avatar';

export function LoginForm({
  profile,
  busy,
  error,
  notice = null,
  canGoBack = true,
  canUsePasskey = false,
  onBack,
  onSubmit,
  onPasskey,
}: Readonly<{
  profile: PublicUser | null;
  busy: boolean;
  error: string | null;
  /** A calm, non-error info line (e.g. "your session expired") shown above the
   * fields distinct from the red `error`. */
  notice?: string | null;
  /** Hide the back link when sign-in is the root screen (roster hidden, no
   * picker to return to). Defaults to shown. */
  canGoBack?: boolean;
  /** Show the "sign in with a passkey" button (secure context + a handler). */
  canUsePasskey?: boolean;
  onBack: () => void;
  onSubmit: (identifier: string, password: string) => void;
  onPasskey?: () => void;
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

      {notice ? (
        <div className="flex w-full items-center gap-2.5 rounded-md border border-accent/25 bg-accent-soft px-3.5 py-2.5 text-[13.5px] font-medium text-accent">
          <IconInfoCircle size={17} stroke={1.9} className="flex-none" />
          <span>{notice}</span>
        </div>
      ) : null}

      {profile ? null : (
        <input
          className={INPUT}
          placeholder={t('auth.emailOrUsername')}
          autoComplete="username"
          value={identifier}
          onChange={(e) => setIdentifier(e.target.value)}
          // biome-ignore lint/a11y/noAutofocus: deliberate initial focus on the sign-in field
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
        // biome-ignore lint/a11y/noAutofocus: deliberate initial focus on the password when a profile is preselected
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
      {canUsePasskey && onPasskey ? (
        <button
          type="button"
          disabled={busy}
          onClick={() => onPasskey()}
          className="flex w-full items-center justify-center gap-2 rounded-md border border-border-strong py-3 text-[14px] font-semibold text-text transition-colors hover:bg-white/5 disabled:opacity-50"
        >
          <IconKey size={17} stroke={1.8} />
          {t('auth.passkeyLogin')}
        </button>
      ) : null}
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

export function RegisterForm({
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
  const [values, setValues] = useState<RegisterValues>({ email: '', username: '', password: '' });
  const [avatar, setAvatar] = useState<File | null>(null);
  const { email, username, password } = values;

  // Shared field rules (mirrors the server), so client + every app validate alike.
  const valid = isEmail(email) && isUsername(username) && isPassword(password);

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        if (valid) onSubmit(email.trim(), username.trim(), password, avatar);
      }}
      className="flex w-full max-w-95 flex-col items-center gap-5"
    >
      <h1 className="font-display text-[28px] font-semibold">{t('auth.newAccount')}</h1>

      <RegisterFields values={values} onChange={setValues} onAvatar={setAvatar} />

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
