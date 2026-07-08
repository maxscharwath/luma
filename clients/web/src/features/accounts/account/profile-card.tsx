// Profile card: avatar upload + editable display name and email. Each row saves
// independently through `PATCH /auth/me` (or the avatar endpoint) and mirrors the
// result into the auth session so the sidebar/picker update immediately.

import { useT } from '@luma/ui';
import { useRef, useState } from 'react';
import { Card, Field, StatusText, useSave } from '#web/features/accounts/account/ui';
import { UserAvatar } from '#web/features/accounts/user-avatar';
import { useAuth } from '#web/shared/lib/auth';
import { Button } from '#web/shared/ui';

export function ProfileCard() {
  const t = useT();
  const { user, client, updateUser } = useAuth();
  const [username, setUsername] = useState(user?.username ?? '');
  const [email, setEmail] = useState(user?.email ?? '');
  const fileRef = useRef<HTMLInputElement>(null);
  const avatar = useSave();
  const name = useSave();
  const mail = useSave();

  if (!user) return null;

  const pickAvatar = (file: File | null) => {
    if (!file) return;
    avatar.run(async () => {
      const { avatarUrl } = await client.uploadAvatar(file);
      updateUser({ avatarUrl });
    }, t('account.avatarFailed'));
  };

  const saveName = () => {
    const next = username.trim();
    if (!next || next === user.username) return;
    name.run(async () => {
      const { user: u } = await client.updateAccount({ username: next });
      updateUser({ username: u.username });
    }, t('account.saveFailed'));
  };

  const saveEmail = () => {
    const next = email.trim();
    if (!next || next === user.email) return;
    mail.run(async () => {
      const { user: u } = await client.updateAccount({ email: next });
      updateUser({ email: u.email });
    }, t('account.saveFailed'));
  };

  return (
    <Card title={t('account.profile')} desc={t('account.profileSub')}>
      {/* Avatar */}
      <div className="flex items-center gap-5">
        <UserAvatar name={user.username} avatarUrl={user.avatarUrl} seed={user.id} size={72} />
        <div className="flex flex-col items-start gap-1.5">
          <Button
            variant="glass"
            size="sm"
            onClick={() => fileRef.current?.click()}
            disabled={avatar.status === 'saving'}
          >
            {avatar.status === 'saving' ? t('common.saving') : t('account.changePhoto')}
          </Button>
          <StatusText status={avatar.status} error={avatar.error} />
        </div>
        <input
          ref={fileRef}
          type="file"
          accept="image/*"
          className="hidden"
          onChange={(e) => pickAvatar(e.target.files?.[0] ?? null)}
        />
      </div>

      {/* Username */}
      <div className="flex flex-col gap-2">
        <Field
          label={t('auth.username')}
          value={username}
          autoComplete="nickname"
          onChange={(e) => setUsername(e.target.value)}
        />
        <div className="flex items-center gap-3">
          <Button
            size="sm"
            onClick={saveName}
            disabled={
              name.status === 'saving' || !username.trim() || username.trim() === user.username
            }
          >
            {name.status === 'saving' ? t('common.saving') : t('common.save')}
          </Button>
          <StatusText status={name.status} error={name.error} />
        </div>
      </div>

      {/* Email */}
      <div className="flex flex-col gap-2">
        <Field
          label={t('auth.email')}
          type="email"
          value={email}
          autoComplete="email"
          onChange={(e) => setEmail(e.target.value)}
        />
        <div className="flex items-center gap-3">
          <Button
            size="sm"
            onClick={saveEmail}
            disabled={mail.status === 'saving' || !email.trim() || email.trim() === user.email}
          >
            {mail.status === 'saving' ? t('common.saving') : t('common.save')}
          </Button>
          <StatusText status={mail.status} error={mail.error} />
        </div>
      </div>
    </Card>
  );
}
