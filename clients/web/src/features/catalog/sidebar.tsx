import { hasPermission, type MessageKey } from '@luma/core';
import { useT } from '@luma/ui';
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
import { useQuery } from '@tanstack/react-query';
import { Link, useNavigate, useRouterState } from '@tanstack/react-router';
import type { ReactNode } from 'react';
import { CapabilityChip } from '#web/features/accounts/capability-chip';
import { UserAvatar } from '#web/features/accounts/user-avatar';
import { useAuth } from '#web/shared/lib/auth';
import { serverQueries } from '#web/shared/lib/queries';
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
    <aside className="sticky top-0 flex h-screen flex-col self-start border-r border-border bg-[#0C0C0E]">
      {/* Fixed header: brand */}
      <div className="shrink-0 px-4.5 pb-2 pt-7">
        <div className="px-2 pb-2">
          <Logo size={26} />
        </div>
      </div>
      {/* Scroll region: primary nav at the top, account/device block pinned to
          the bottom via mt-auto. It reads as a fixed footer on a normal window,
          and scrolls (rather than clipping) when the viewport is too short. */}
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-4.5 pb-7 pt-1">
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
        {/* Footer block: invite / device / admin / account / device prefs */}
        <div className="mt-auto flex flex-col gap-2.5 pt-6">
          <InviteLink />
          <Link to="/connect" className={itemCls}>
            <IconDeviceDesktop size={18} />
            {t('nav.connectDevice')}
          </Link>
          <AdminLink />
          <UserChip />
          <div className="flex flex-col gap-2 px-2 pt-1">
            <div className="flex flex-wrap items-center justify-between gap-x-2 gap-y-1.5">
              <span className="text-[10px] font-bold uppercase tracking-[.1em] text-dim">
                {t('nav.thisDevice')}
              </span>
              <CapabilityChip />
            </div>
            <VersionInfo />
          </div>
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

/** Client + server versions on one compact line. The client version is injected
 * at build time (`__APP_VERSION__`); the server version comes from the public
 * `/api/health` endpoint (falls back to `…` until it resolves). Hover for labels. */
function VersionInfo() {
  const t = useT();
  const { data: health } = useQuery(serverQueries.health());
  return (
    <div className="flex items-center gap-1.5 px-0.5 text-[10px] font-medium text-dim">
      <span>
        {t('nav.versionClient')} <span className="tabular-nums">v{__APP_VERSION__}</span>
      </span>
      <span className="opacity-40">·</span>
      <span>
        {t('nav.versionServer')}{' '}
        <span className="tabular-nums">{health ? `v${health.version}` : '…'}</span>
      </span>
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
