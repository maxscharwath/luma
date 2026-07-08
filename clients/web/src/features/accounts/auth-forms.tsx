// The sign-in and registration forms used by the login gate. Split out of
// `AuthGate.tsx`, which owns the gate/routing + profile picker and composes
// these two screens.

import { isEmail, isPassword, isUsername, type PublicUser } from '@luma/core';
import { useT } from '@luma/ui';
import { IconPlus } from '@tabler/icons-react';
import { useEffect, useRef, useState } from 'react';
import { avatarGradient, initials, UserAvatar } from '#web/features/accounts/user-avatar';

const INPUT =
  'w-full rounded-md border border-border-strong bg-surface-2 px-4 py-3.5 text-[15px] text-text outline-none transition-colors placeholder:text-dim focus:border-accent';

export function LoginForm({
  profile,
  busy,
  error,
  canGoBack = true,
  onBack,
  onSubmit,
}: Readonly<{
  profile: PublicUser | null;
  busy: boolean;
  error: string | null;
  /** Hide the back link when sign-in is the root screen (roster hidden, no
   * picker to return to). Defaults to shown. */
  canGoBack?: boolean;
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

      {/* Avatar upload click the tile to choose an image. */}
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
