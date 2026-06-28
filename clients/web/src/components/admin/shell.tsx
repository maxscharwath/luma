// Admin console shell: the left sidebar (identity + nav + live status card), the
// page header, and the data/event context (server info + a tick that bumps on
// server events so pages can refresh live).

import {
  hasPermission,
  LumaEvents,
  type MessageKey,
  type Permission,
  type ServerInfo,
  type User,
} from '@luma/core';
import { Logo, useT } from '@luma/ui';
import {
  IconChevronRight,
  IconDatabase,
  IconLibrary,
  IconPlus,
  IconSettings,
  IconTransform,
  IconWorld,
  type TablerIcon,
} from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from 'react';
import { formatUptime } from '#web/lib/adminFormat';
import { apiBase } from '#web/lib/api';
import { useAuth } from '#web/lib/auth';

// ----- data + events context --------------------------------------------------

interface AdminCtx {
  serverInfo: ServerInfo | null;
  /** Bumps on every server event — depend on it to refetch live. */
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

  // Live event stream → tick.
  useEffect(() => {
    const ev = new LumaEvents(apiBase(), { onEvent: () => setTick((t) => t + 1) });
    ev.connect();
    return () => ev.close();
  }, []);

  return <AdminContext.Provider value={{ serverInfo, tick }}>{children}</AdminContext.Provider>;
}

export function useAdmin(): AdminCtx {
  const ctx = useContext(AdminContext);
  if (!ctx) throw new Error('useAdmin must be used within <AdminProvider>');
  return ctx;
}

/** Poll `fn` every `intervalMs` (and immediately). Re-runs when `deps` change. */
export function usePoll<T>(
  fn: () => Promise<T>,
  intervalMs: number,
  deps: unknown[],
): { data: T | null; reload: () => void } {
  const [data, setData] = useState<T | null>(null);
  const fnRef = useRef(fn);
  fnRef.current = fn;
  const reload = useCallback(() => {
    fnRef
      .current()
      .then(setData)
      .catch(() => undefined);
  }, []);
  useEffect(() => {
    let active = true;
    const run = () =>
      fnRef
        .current()
        .then((d) => active && setData(d))
        .catch(() => undefined);
    run();
    const iv = setInterval(run, intervalMs);
    return () => {
      active = false;
      clearInterval(iv);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return { data, reload };
}

/** A busy-tracked async action for modal save/delete handlers. `run(fn, onError?)`
 * flips `busy` while `fn` runs and, on failure, sets `error` to `onError(e)` (when
 * provided) — collapsing the repeated setBusy/try/catch/finally boilerplate. */
export function useAsyncAction(): {
  busy: boolean;
  error: string | null;
  run: (fn: () => Promise<void>, onError?: (e: unknown) => string) => Promise<void>;
} {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = useCallback(async (fn: () => Promise<void>, onError?: (e: unknown) => string) => {
    setBusy(true);
    setError(null);
    try {
      await fn();
    } catch (e) {
      if (onError) setError(onError(e));
    } finally {
      setBusy(false);
    }
  }, []);
  return { busy, error, run };
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
  { to: '/admin/storage', labelKey: 'admin.navStorage', cap: null, icon: IconDatabase },
];

/** True if the user holds any management capability (unlocks the console). */
export function isAnyAdmin(user: Pick<User, 'permissions'> | null | undefined): boolean {
  return (
    !!user &&
    (hasPermission(user, 'users.manage') ||
      hasPermission(user, 'library.manage') ||
      hasPermission(user, 'settings.manage'))
  );
}

/** Whether the current user satisfies `cap` (or is any admin when `cap` is null). */
export function useCap(cap?: Permission | null): boolean {
  const { user } = useAuth();
  if (!user) return false;
  return cap ? hasPermission(user, cap) : isAnyAdmin(user);
}

/** Full-section "access denied" panel for pages the user can't reach. */
export function Denied() {
  const t = useT();
  return (
    <div className="flex min-h-[60vh] items-center justify-center px-6">
      <div className="rounded-2xl border border-border bg-surface-1 px-8 py-10 text-center shadow-card">
        <div className="font-display text-[18px] font-bold">{t('admin.accessDenied')}</div>
        <p className="mt-2 text-[14px] text-dim">{t('admin.sectionDenied')}</p>
      </div>
    </div>
  );
}

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

// ----- layout + header --------------------------------------------------------

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

export function PageHeader({
  title,
  suffix,
  subtitle,
  action,
  realtime,
}: Readonly<{
  title: string;
  suffix?: string;
  subtitle?: string;
  action?: ReactNode;
  realtime?: boolean;
}>) {
  const t = useT();
  return (
    <div className="mb-2 flex items-center justify-between gap-6">
      <div className="min-w-0">
        <h1 className="font-display text-[34px] font-bold leading-[1.05] tracking-[-.02em]">
          {title} {suffix ? <span className="font-normal text-text/40">{suffix}</span> : null}
        </h1>
        {subtitle ? (
          <p className="mt-2 max-w-140 text-[14.5px] font-medium text-text/50">{subtitle}</p>
        ) : null}
      </div>
      {realtime ? (
        <div className="flex shrink-0 items-center gap-2.5 rounded-full border border-border bg-[#121216] px-4 py-2">
          <span className="h-1.75 w-1.75 animate-[luma-breathe_2s_ease-in-out_infinite] rounded-full bg-accent" />
          <span className="text-[13px] font-semibold text-text/70">
            {t('admin.realtimeActivity')}
          </span>
        </div>
      ) : null}
      {action}
    </div>
  );
}

/** The amber primary action button used in headers ("Inviter", "Ajouter", …). */
export function HeaderAction({
  label,
  onClick,
  plus = true,
}: Readonly<{
  label: string;
  onClick?: () => void;
  plus?: boolean;
}>) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="inline-flex shrink-0 items-center gap-2 rounded-md bg-accent px-4.5 py-2.75 text-[14px] font-bold text-accent-ink transition-colors hover:bg-accent-hover"
    >
      {plus ? <IconPlus size={16} stroke={2.6} /> : null}
      {label}
    </button>
  );
}
