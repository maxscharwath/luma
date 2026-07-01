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
  IconArchive,
  IconChevronRight,
  IconClockBolt,
  IconDatabase,
  IconLibrary,
  IconSettings,
  IconSitemap,
  IconSparkles,
  IconTransform,
  IconWorld,
  type TablerIcon,
} from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import { createContext, type ReactNode, useCallback, useContext, useEffect, useState } from 'react';
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
  /** Bumps on every server event depend on it to refetch live. */
  tick: number;
}

const AdminContext = createContext<AdminCtx | null>(null);

export function AdminProvider({ children }: Readonly<{ children: ReactNode }>) {
  const { client } = useAuth();
  const [serverInfo, setServerInfo] = useState<ServerInfo | null>(null);
  const [tick, setTick] = useState(0);

  const loadServer = useCallback(() => {
    client
      .adminServer()
      .then(setServerInfo)
      .catch(() => undefined);
  }, [client]);

  // Reload server info initially, every 15s (uptime), and on each event.
  useEffect(() => {
    loadServer();
    const iv = setInterval(loadServer, 15000);
    return () => clearInterval(iv);
  }, [loadServer, tick]);

  // Live event stream → tick. Skip the high-frequency per-line `job.log` and
  // `job.progress` frames: bumping `tick` re-runs every admin `usePoll`, and a
  // verbose job would otherwise storm the admin endpoints. The jobs page consumes
  // those two on its own stream for smooth progress; start/finish still tick.
  // Remaining events are COALESCED (one bump per window): an enrich pass emits
  // one `item.updated` per title, which would otherwise refetch every admin
  // panel hundreds of times in a row.
  useEffect(() => {
    let pending: ReturnType<typeof setTimeout> | null = null;
    const ev = new LumaEvents(apiBase(), {
      onEvent: (e) => {
        if (e.type === 'job.log' || e.type === 'job.progress') return;
        if (pending) return;
        pending = setTimeout(() => {
          pending = null;
          setTick((t) => t + 1);
        }, 1500);
      },
    });
    ev.connect();
    return () => {
      if (pending) clearTimeout(pending);
      ev.close();
    };
  }, []);

  return <AdminContext.Provider value={{ serverInfo, tick }}>{children}</AdminContext.Provider>;
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
  {
    to: '/admin/libraries',
    labelKey: 'admin.navLibraries',
    cap: 'library.manage',
    icon: IconLibrary,
  },
  {
    to: '/admin/transcoder',
    labelKey: 'admin.navTranscoder',
    cap: 'settings.manage',
    icon: IconTransform,
  },
  { to: '/admin/ai', labelKey: 'admin.navAi', cap: 'settings.manage', icon: IconSparkles },
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
