// Shared building blocks for the account-creation forms. Both the in-app
// registration screen (`auth-forms.tsx`) and the public `/join` invite page
// render the exact same avatar tile + email/username/password inputs, so that
// block lives here once and is driven by controlled props.

import { Image, useT } from '@kroma/ui';
import { IconPlus } from '@tabler/icons-react';
import { useEffect, useRef, useState } from 'react';
import { avatarGradient, initials } from '#web/features/accounts/user-avatar';

export const INPUT =
  'w-full rounded-md border border-border-strong bg-surface-2 px-4 py-3.5 text-[15px] text-text outline-none transition-colors placeholder:text-dim focus:border-accent';

export type RegisterValues = Readonly<{ email: string; username: string; password: string }>;

/** Avatar picker tile + the three registration inputs, controlled by the parent
 * form. The object-URL preview and hidden file input are managed internally; the
 * chosen File is reported through `onAvatar`. */
export function RegisterFields({
  values,
  onChange,
  onAvatar,
}: Readonly<{
  values: RegisterValues;
  onChange: (values: RegisterValues) => void;
  onAvatar: (avatar: File | null) => void;
}>) {
  const t = useT();
  const [preview, setPreview] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const { email, username, password } = values;

  // Revoke the object URL when the preview changes / unmounts.
  useEffect(() => {
    return () => {
      if (preview) URL.revokeObjectURL(preview);
    };
  }, [preview]);

  function pickFile(f: File | null) {
    if (preview) URL.revokeObjectURL(preview);
    onAvatar(f);
    setPreview(f ? URL.createObjectURL(f) : null);
  }

  return (
    <>
      {/* Avatar upload click the tile to choose an image. */}
      <button
        type="button"
        onClick={() => fileRef.current?.click()}
        className="group relative h-28 w-28 overflow-hidden rounded-xl focus:outline-none"
        aria-label={t('auth.chooseAvatar')}
      >
        {preview ? (
          <Image src={preview} fit="cover" fill />
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
        onChange={(e) => onChange({ ...values, email: e.target.value })}
      />
      <input
        className={INPUT}
        placeholder={t('auth.username')}
        autoComplete="nickname"
        value={username}
        onChange={(e) => onChange({ ...values, username: e.target.value })}
      />
      <input
        className={INPUT}
        type="password"
        placeholder={t('auth.passwordHint')}
        autoComplete="new-password"
        value={password}
        onChange={(e) => onChange({ ...values, password: e.target.value })}
      />
    </>
  );
}
