import { hasPermission, LOCALES, type MessageKey } from '@luma/core';
import { useLocale, useSetLocale, useT } from '@luma/ui';
import * as DropdownMenu from '@radix-ui/react-dropdown-menu';
import {
  IconDeviceDesktop,
  IconDeviceTv,
  IconHome,
  IconInbox,
  IconListDetails,
  IconLogout,
  IconMovie,
  IconSearch,
  IconSettings,
  IconUserCircle,
  IconUserPlus,
  IconUsers,
  type TablerIcon,
} from '@tabler/icons-react';
import { Link, useNavigate, useRouterState } from '@tanstack/react-router';
import type { ReactNode } from 'react';
import { CapabilityChip } from '#web/features/accounts/capability-chip';
import { UserAvatar } from '#web/features/accounts/user-avatar';
import { useAuth } from '#web/shared/lib/auth';
import { Logo } from '#web/shared/ui';

const itemCls =
  'flex items-center gap-3.5 rounded-[11px] px-3.5 py-3 text-[15px] font-semibold text-muted no-underline transition-colors duration-200 hover:bg-white/4 hover:text-text aria-[current=page]:bg-accent-soft aria-[current=page]:text-accent';

const NAV: { labelKey: MessageKey; to: string; icon: TablerIcon; exact?: boolean }[] = [
  { labelKey: 'nav.home', to: '/', icon: IconHome, exact: true },
  { labelKey: 'nav.search', to: '/search', icon: IconSearch },
  { labelKey: 'nav.films', to: '/films', icon: IconMovie },
  { labelKey: 'nav.series', to: '/series', icon: IconDeviceTv },
  { labelKey: 'nav.myList', to: '/mylist', icon: IconListDetails },
];

export function Sidebar() {
  const t = useT();
  return (
    <aside className="sticky top-0 flex h-screen flex-col gap-1 border-r border-border bg-[#0C0C0E] px-4.5 py-7">
      <div className="px-2 pb-4">
        <Logo size={26} />
      </div>
      <nav className="flex flex-col gap-0.5">
        {NAV.map((item) => (
          <Link
            key={item.to}
            to={item.to}
            className={itemCls}
            activeOptions={{ exact: item.exact ?? false }}
          >
            <item.icon size={18} />
            {t(item.labelKey)}
          </Link>
        ))}
        <RequestsLink />
      </nav>
      <div className="mt-auto flex flex-col gap-2.5">
        <InviteLink />
        <Link to="/connect" className={itemCls}>
          <IconDeviceDesktop size={18} />
          {t('nav.connectDevice')}
        </Link>
        <AdminLink />
        <UserChip />
        <div className="flex flex-col gap-2.5 px-2 pt-2">
          <span className="text-[11px] font-bold uppercase tracking-[.12em] text-dim">
            {t('nav.thisDevice')}
          </span>
          <CapabilityChip />
          <LanguageSwitch />
        </div>
      </div>
    </aside>
  );
}

/** "Mes demandes" only for accounts allowed to request media. */
function RequestsLink() {
  const t = useT();
  const { user } = useAuth();
  if (!user || !hasPermission(user, 'requests.create')) return null;
  return (
    <Link to="/requests" className={itemCls}>
      <IconInbox size={18} />
      {t('nav.requests')}
    </Link>
  );
}

/** Inline language picker two small pills (French / English) that bubble the
 * choice through `useSetLocale` (persisted + account-synced by LocaleProvider). */
function LanguageSwitch() {
  const t = useT();
  const locale = useLocale();
  const setLocale = useSetLocale();
  return (
    <div className="flex gap-1.5 rounded-md bg-white/4 p-1" aria-label={t('common.language')}>
      {LOCALES.map((l) => (
        <button
          key={l.code}
          type="button"
          onClick={() => setLocale(l.code)}
          aria-pressed={locale === l.code}
          className={`flex-1 rounded-[7px] px-2.5 py-1.5 text-[12px] font-semibold transition-colors ${
            locale === l.code
              ? 'bg-accent-soft text-accent'
              : 'text-muted hover:bg-white/4 hover:text-text'
          }`}
        >
          {t(l.labelKey)}
        </button>
      ))}
    </div>
  );
}

/** "Inviter un utilisateur" only for accounts with the `users.manage`
 * permission (registration is invite-only). */
function InviteLink() {
  const t = useT();
  const { user } = useAuth();
  if (!user || !hasPermission(user, 'users.manage')) return null;
  return (
    <Link to="/invite" className={itemCls}>
      <IconUserPlus size={18} />
      {t('nav.inviteUser')}
    </Link>
  );
}

/** "Serveur" links to the admin console for accounts with any management
 * capability (users / library / settings). */
function AdminLink() {
  const t = useT();
  const { user } = useAuth();
  const isAdmin =
    !!user &&
    (hasPermission(user, 'users.manage') ||
      hasPermission(user, 'library.manage') ||
      hasPermission(user, 'settings.manage') ||
      hasPermission(user, 'requests.manage'));
  if (!isAdmin) {
    return (
      <div className={`${itemCls} cursor-default opacity-50`}>
        <IconSettings size={18} />
        {t('nav.settings')}
      </div>
    );
  }
  return (
    <Link to="/admin" className={itemCls}>
      <IconSettings size={18} />
      {t('nav.server')}
    </Link>
  );
}

const MENU =
  'z-50 min-w-[204px] rounded-xl border border-white/[0.10] bg-[#16161C] p-1.5 shadow-[0_12px_32px_rgba(0,0,0,.45)]';

/** A row inside the account menu. */
function MenuItem({
  icon,
  label,
  onSelect,
  danger,
}: Readonly<{ icon: ReactNode; label: string; onSelect: () => void; danger?: boolean }>) {
  return (
    <DropdownMenu.Item
      onSelect={onSelect}
      className={`flex cursor-pointer items-center gap-3 rounded-lg px-3 py-2.5 text-[14px] font-semibold outline-none transition-colors data-highlighted:bg-white/8 ${
        danger ? 'text-danger' : 'text-text'
      }`}
    >
      {icon}
      {label}
    </DropdownMenu.Item>
  );
}

/** Current account chip avatar + name; clicking opens a menu (account settings,
 * switch profile, sign out). Renders nothing until a session is hydrated. */
function UserChip() {
  const t = useT();
  const navigate = useNavigate();
  const { user, logout } = useAuth();
  // Return to the current page after switching profile.
  const href = useRouterState({ select: (s) => s.location.href });
  if (!user) return null;
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button
          type="button"
          className="mt-2 flex items-center gap-3 rounded-[11px] p-2.5 text-left transition-colors hover:bg-white/4 focus:outline-none data-[state=open]:bg-white/4"
          title={t('nav.account')}
        >
          <UserAvatar
            name={user.username}
            avatarUrl={user.avatarUrl}
            seed={user.id}
            size={36}
            radius={9}
          />
          <div className="min-w-0">
            <div className="truncate text-[14px] font-semibold text-text">{user.username}</div>
            <div className="truncate text-[11px] font-medium text-dim">{t('nav.account')}</div>
          </div>
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content side="top" align="start" sideOffset={6} className={MENU}>
          <MenuItem
            icon={<IconUserCircle size={17} />}
            label={t('nav.accountSettings')}
            onSelect={() => void navigate({ to: '/account' })}
          />
          <MenuItem
            icon={<IconUsers size={17} />}
            label={t('nav.changeProfile')}
            onSelect={() => void navigate({ to: '/login', search: { redirect: href } })}
          />
          <DropdownMenu.Separator className="my-1 h-px bg-white/[0.07]" />
          <MenuItem
            icon={<IconLogout size={17} />}
            label={t('auth.logout')}
            onSelect={() => void logout()}
            danger
          />
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}
