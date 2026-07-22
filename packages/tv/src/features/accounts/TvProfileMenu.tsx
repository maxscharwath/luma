import { useT } from '@kroma/ui';
import { useFocusNav } from '@kroma/ui/kit';
import { IconLock, IconLogout, IconUsersGroup } from '@tabler/icons-react';
import { useAuth } from '#tv/app/providers/auth';
import { useConnection } from '#tv/app/providers/connection';
import { useNav } from '#tv/app/router';
import { actionItem } from '#tv/app/settings/items';
import { PROFILE_SETTINGS, quitAppItem } from '#tv/app/settings/registry';
import { AuthScreen, ProfileAvatar } from '#tv/shared/ui';
import { SettingsRows } from './SettingsRows';

/** Profile menu (route `profileMenu`): the shared settings block
 * (PROFILE_SETTINGS: language, keyboard, engine, GPU) followed by the
 * account rows built inline - PIN, change profile, sign out, quit. Removing a
 * server happens by signing its profiles out, not from here. Every stateful
 * hook lives inside SettingsRows' row components, so the `!user` early return
 * below can't break hook order. */
export function TvProfileMenu() {
  const nav = useNav();
  const t = useT();
  const { activeServerUrl, client } = useConnection();
  const { user, switchProfile, logout, forget } = useAuth();
  useFocusNav({ onBack: nav.back });

  if (!user) return null;

  const onSignOut = () => {
    if (activeServerUrl) forget(user.id, activeServerUrl);
    else void logout();
  };

  const rows = [
    ...PROFILE_SETTINGS,
    actionItem({
      id: 'pin',
      icon: IconLock,
      label: user.hasPin ? 'profileMenu.removePin' : 'profileMenu.setPin',
      badge: user.hasPin
        ? { label: 'profileMenu.on', tone: 'success' as const }
        : { label: 'profileMenu.off', tone: 'dim' as const },
      run: () => nav.go('pin', { intent: user.hasPin ? 'clear' : 'set' }),
    }),
    actionItem({
      id: 'changeProfile',
      icon: IconUsersGroup,
      label: 'nav.changeProfile',
      run: switchProfile,
    }),
    actionItem({ id: 'signOut', icon: IconLogout, label: 'auth.logout', run: onSignOut }),
    quitAppItem,
  ];

  return (
    <AuthScreen>
      <div className="mb-8 flex flex-col items-center gap-3.5">
        <ProfileAvatar
          name={user.username}
          seed={user.id}
          size={96}
          radius={26}
          src={client?.resolveArt(user.avatarUrl)}
        />
        <h1 className="m-0 font-display text-[32px] font-semibold">{user.username}</h1>
      </div>

      <div className="flex w-full max-w-[560px] flex-col gap-3">
        <SettingsRows items={rows} />
      </div>

      <div className="mt-7 font-sans text-[14px] font-medium text-[rgba(244,243,240,0.4)]">
        {t('profileMenu.navHint')}
      </div>
    </AuthScreen>
  );
}
