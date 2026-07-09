// Admin console shell: the left sidebar (identity + nav + live status card) and
// the data/event context (server info + a tick that bumps on server events so
// pages can refresh live).

import {
  hasPermission,
  LumaEvents,
  type MessageKey,
  type Permission,
  type ServerInfo,
} from '@luma/core';
import { Logo, useT } from '@luma/ui';
import {
  IconAntenna,
  IconArchive,
  IconChevronRight,
  IconClockBolt,
  IconCloud,
  IconDatabase,
  IconDownload,
  IconFileText,
  IconLibrary,
  IconMagnet,
  IconSettings,
  IconSitemap,
  IconSparkles,
  IconTransform,
  IconWorld,
  type TablerIcon,
} from '@tabler/icons-react';
import { useQueryClient } from '@tanstack/react-query';
import { Link } from '@tanstack/react-router';
import { createContext, type ReactNode, useContext, useEffect } from 'react';
import { usePoll } from '#web/features/admin/hooks';
import { formatUptime } from '#web/shared/lib/adminFormat';
import { apiBase } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';

export { HeaderAction, PageHeader } from '#web/features/admin/header';
// Data hooks + capability helpers and the page header live in sibling modules;
// re-exported here so call sites keep importing them from this shell module.
export { Denied, isAnyAdmin, useAsyncAction, useCap, usePoll } from '#web/features/admin/hooks';

// ----- data + events context --------------------------------------------------

interface AdminCtx {
  serverInfo: ServerInfo | null;
}

const AdminContext = createContext<AdminCtx | null>(null);

export function AdminProvider({ children }: Readonly<{ children: ReactNode }>) {
  const { client } = useAuth();
  const queryClient = useQueryClient();
  // Server info (uptime etc.) as a plain admin poll it refetches on the 15s tick
  // and whenever the event stream below invalidates the `['admin']` namespace.
  const { data: serverInfo } = usePoll(['admin', 'server'], () => client.adminServer(), 15000);

  // Live event stream → invalidate every admin query (they share the `['admin']`
  // key prefix). Skip the high-frequency per-line `job.log` / `job.progress` /
  // `download.progress` frames (the jobs page streams those itself for smooth
  // progress); a verbose job would otherwise storm the admin endpoints. Remaining
  // events are COALESCED (one refresh per window): an enrich pass emits one
  // `item.updated` per title, which would otherwise refetch every panel hundreds
  // of times in a row.
  useEffect(() => {
    let pending: ReturnType<typeof setTimeout> | null = null;
    const ev = new LumaEvents(apiBase(), {
      onEvent: (e) => {
        if (e.type === 'job.log' || e.type === 'job.progress' || e.type === 'download.progress')
          return;
        if (pending) return;
        pending = setTimeout(() => {
          pending = null;
          void queryClient.invalidateQueries({ queryKey: ['admin'] });
        }, 1500);
      },
    });
    ev.connect();
    return () => {
      if (pending) clearTimeout(pending);
      ev.close();
    };
  }, [queryClient]);

  return <AdminContext.Provider value={{ serverInfo }}>{children}</AdminContext.Provider>;
}

export function useAdmin(): AdminCtx {
  const ctx = useContext(AdminContext);
  if (!ctx) throw new Error('useAdmin must be used within <AdminProvider>');
  return ctx;
}

// ----- sidebar ----------------------------------------------------------------

// `cap: null` → visible to any admin (the read-only dashboard panels); otherwise
// the item is shown only to users holding that specific capability, mirroring the
// server-side guards in `api/admin.rs`.
const NAV_GESTION: {
  to: string;
  labelKey: MessageKey;
  exact: boolean;
  cap: Permission | null;
}[] = [
  { to: '/admin', labelKey: 'admin.dashboard', exact: true, cap: null },
  { to: '/admin/users', labelKey: 'admin.navUsers', exact: false, cap: 'users.manage' },
  { to: '/admin/requests', labelKey: 'admin.navRequests', exact: false, cap: 'requests.manage' },
];

const NAV_REGLAGES: {
  to: string;
  labelKey: MessageKey;
  cap: Permission | null;
  icon: TablerIcon;
}[] = [
  {
    to: '/admin/general',
    labelKey: 'admin.navGeneral',
    cap: 'settings.manage',
    icon: IconSettings,
  },
  { to: '/admin/network', labelKey: 'admin.navNetwork', cap: 'settings.manage', icon: IconWorld },
  { to: '/admin/remote', labelKey: 'admin.navRemote', cap: 'settings.manage', icon: IconCloud },
  {
    to: '/admin/libraries',
    labelKey: 'admin.navLibraries',
    cap: 'library.manage',
    icon: IconLibrary,
  },
  {
    to: '/admin/naming',
    labelKey: 'admin.navNaming',
    cap: 'library.manage',
    icon: IconFileText,
  },
  {
    to: '/admin/transcoder',
    labelKey: 'admin.navTranscoder',
    cap: 'settings.manage',
    icon: IconTransform,
  },
  { to: '/admin/ai', labelKey: 'admin.navAi', cap: 'settings.manage', icon: IconSparkles },
  {
    to: '/admin/acquisition',
    labelKey: 'admin.navAcquisition',
    cap: 'settings.manage',
    icon: IconMagnet,
  },
  {
    to: '/admin/indexers',
    labelKey: 'admin.navIndexers',
    cap: 'settings.manage',
    icon: IconAntenna,
  },
  {
    to: '/admin/downloads',
    labelKey: 'admin.navDownloads',
    cap: null,
    icon: IconDownload,
  },
  { to: '/admin/jobs', labelKey: 'admin.navJobs', cap: 'settings.manage', icon: IconClockBolt },
  {
    to: '/admin/pipeline',
    labelKey: 'admin.navPipeline',
    cap: 'settings.manage',
    icon: IconSitemap,
  },
  { to: '/admin/storage', labelKey: 'admin.navStorage', cap: null, icon: IconDatabase },
  { to: '/admin/backup', labelKey: 'admin.navBackup', cap: 'settings.manage', icon: IconArchive },
];

const linkCls =
  'flex items-center gap-3 rounded-md px-3.5 py-2.5 text-[14px] font-semibold text-muted no-underline transition-colors hover:bg-white/4 hover:text-text aria-[current=page]:bg-accent-soft aria-[current=page]:text-accent';

function AdminSidebar() {
  const t = useT();
  const { serverInfo } = useAdmin();
  const { user } = useAuth();
  const visible = (cap: Permission | null) => !cap || (!!user && hasPermission(user, cap));
  const gestion = NAV_GESTION.filter((n) => visible(n.cap));
  const reglages = NAV_REGLAGES.filter((n) => visible(n.cap));
  return (
    <aside className="sticky top-0 flex h-screen w-64 shrink-0 flex-col overflow-y-auto border-r border-border bg-[#0C0C0E] px-3.5 py-6">
      <div className="mb-5 flex items-center gap-2.5 px-2.5">
        <Logo markOnly size={25} />
        <span className="font-display text-[20px] font-extrabold leading-none tracking-[.16em]">
          LUMA
        </span>
        <span className="rounded-[5px] bg-accent px-1.5 py-0.75 text-[8.5px] font-bold tracking-[.13em] text-accent-ink">
          {t('admin.badge')}
        </span>
      </div>

      <Link
        to="/"
        className="mb-2 flex items-center justify-between rounded-[11px] border border-border-strong bg-surface-2 px-3.5 py-2.5 no-underline"
      >
        <span className="inline-flex items-center gap-2.5 text-[14px] font-bold text-accent">
          <Logo markOnly size={17} />
          {serverInfo?.name ?? 'LUMA'}
        </span>
        <IconChevronRight size={17} stroke={1.8} color="#46D08D" />
      </Link>

      {gestion.length > 0 ? (
        <SidebarGroup label={t('admin.groupManagement')}>
          {gestion.map((n) => (
            <Link
              key={n.to}
              to={n.to}
              className={linkCls}
              activeOptions={{ exact: n.exact ?? false }}
            >
              <span className="h-1.25 w-1.25 rounded-full bg-current opacity-60" />
              {t(n.labelKey)}
            </Link>
          ))}
        </SidebarGroup>
      ) : null}

      {reglages.length > 0 ? (
        <SidebarGroup label={t('admin.groupSettings')}>
          {reglages.map((n) => (
            <Link key={n.to} to={n.to} className={linkCls} activeOptions={{ exact: false }}>
              <n.icon size={18} stroke={1.7} />
              {t(n.labelKey)}
            </Link>
          ))}
        </SidebarGroup>
      ) : null}

      <ServerStatusCard />
    </aside>
  );
}

function SidebarGroup({ label, children }: Readonly<{ label: string; children: ReactNode }>) {
  return (
    <>
      <div className="px-3 pb-2 pt-4.5 text-[10px] font-bold uppercase tracking-[.16em] text-dim">
        {label}
      </div>
      {children}
    </>
  );
}

function ServerStatusCard() {
  const t = useT();
  const { serverInfo } = useAdmin();
  return (
    <div className="mt-auto rounded-xl border border-border bg-[#121216] p-3.5">
      <div className="mb-2 flex items-center gap-2.5">
        <span className="h-2 w-2 animate-[luma-breathe_2s_ease-in-out_infinite] rounded-full bg-success" />
        <span className="text-[13px] font-bold text-success">{t('admin.online')}</span>
      </div>
      <div className="text-[12.5px] font-semibold text-text">
        {serverInfo ? `${serverInfo.hostname} · v${serverInfo.version}` : '…'}
      </div>
      <div className="mt-0.75 text-[11px] font-medium text-dim">
        {serverInfo ? t('admin.uptime', { uptime: formatUptime(serverInfo.uptimeSec) }) : ''}
      </div>
    </div>
  );
}

// ----- layout -----------------------------------------------------------------

export function AdminLayout({ children }: Readonly<{ children: ReactNode }>) {
  return (
    <AdminProvider>
      <div className="flex min-h-screen w-full bg-bg text-text">
        <AdminSidebar />
        <main className="min-w-0 flex-1 px-11 pb-16 pt-7.5">
          <div className="max-w-375">{children}</div>
        </main>
      </div>
    </AdminProvider>
  );
}
