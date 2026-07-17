// Photo section: the account avatar with an upload control. Picking a file
// uploads immediately through the avatar endpoint and mirrors the resulting URL
// into the auth session so the sidebar/picker update at once. (The server has no
// avatar-removal endpoint, so there is no reset here.)

import { useT } from '@kroma/ui';
import { IconCamera } from '@tabler/icons-react';
import { useRef } from 'react';
import { Panel, StatusText, useSave } from '#web/features/accounts/account/ui';
import { UserAvatar } from '#web/features/accounts/user-avatar';
import { useAuth } from '#web/shared/lib/auth';
import { Button } from '#web/shared/ui';

export function PhotoCard() {
  const t = useT();
  const { user, client, updateUser } = useAuth();
  const fileRef = useRef<HTMLInputElement>(null);
  const avatar = useSave();

  if (!user) return null;

  const pickAvatar = (file: File | null) => {
    if (!file) return;
    avatar.run(async () => {
      const { avatarUrl } = await client.uploadAvatar(file);
      updateUser({ avatarUrl });
    }, t('account.avatarFailed'));
  };

  return (
    <Panel className="flex flex-wrap items-center gap-6 p-5.5">
      <div className="relative flex-none">
        <UserAvatar
          name={user.username}
          avatarUrl={user.avatarUrl}
          seed={user.id}
          size={96}
          radius={26}
        />
        <span className="absolute -bottom-1 -right-1 flex size-8 items-center justify-center rounded-full border border-border-strong bg-surface-2 text-accent">
          <IconCamera size={15} stroke={1.9} />
        </span>
      </div>

      <div className="min-w-[min(240px,100%)] flex-1">
        <p className="mb-3 text-[13px] font-semibold text-muted">{t('account.photoHint')}</p>
        <div className="flex items-center gap-3">
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
      </div>

      <input
        ref={fileRef}
        type="file"
        accept="image/*"
        className="hidden"
        onChange={(e) => pickAvatar(e.target.files?.[0] ?? null)}
      />
    </Panel>
  );
}
