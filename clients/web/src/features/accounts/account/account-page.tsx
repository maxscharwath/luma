// The `/account` settings page: profile (avatar/name/email), security (password)
// and playback preferences (UI/audio/subtitle language). Everything here acts on
// the signed-in account only and syncs across devices via the server.

import { useT } from '@luma/ui';
import { IconLogout } from '@tabler/icons-react';
import { PinCard } from '#web/features/accounts/account/pin-card';
import { PreferencesCard } from '#web/features/accounts/account/preferences-card';
import { ProfileCard } from '#web/features/accounts/account/profile-card';
import { SecurityCard } from '#web/features/accounts/account/security-card';
import { UserAvatar } from '#web/features/accounts/user-avatar';
import { useAuth } from '#web/shared/lib/auth';
import { Button } from '#web/shared/ui';

export function AccountPage() {
  const t = useT();
  const { user, logout } = useAuth();

  if (!user) {
    return (
      <main className="max-w-4xl px-(--gutter-web) pb-16 pt-10">
        <p className="text-[15px] text-muted">{t('account.signedOut')}</p>
      </main>
    );
  }

  return (
    <main className="mx-auto w-full max-w-3xl px-(--gutter-web) pb-20 pt-10">
      <header className="mb-8 flex items-center gap-4">
        <UserAvatar name={user.username} avatarUrl={user.avatarUrl} seed={user.id} size={64} />
        <div className="min-w-0 flex-1">
          <h1 className="truncate font-display text-[28px] font-bold tracking-[-.02em]">
            {user.username}
          </h1>
          <p className="truncate text-[14px] text-muted">{user.email}</p>
        </div>
        <Button
          variant="ghost"
          size="sm"
          icon={<IconLogout size={16} />}
          onClick={() => void logout()}
        >
          {t('auth.logout')}
        </Button>
      </header>

      <div className="flex flex-col gap-5">
        <ProfileCard />
        <SecurityCard />
        <PinCard />
        <PreferencesCard />
      </div>
    </main>
  );
}
