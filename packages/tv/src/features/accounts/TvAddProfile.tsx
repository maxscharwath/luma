import { normalizeServerUrl as norm } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconChevronRight, IconPlus, IconServer2 } from '@tabler/icons-react';
import { type ReactNode, useEffect, useMemo } from 'react';
import { useConnection } from '#tv/app/providers/connection';
import { useNav } from '#tv/app/router';
import { useServersHealth } from '#tv/app/useServersHealth';
import { StatusDot } from '#tv/features/accounts/ServerStatus';
import { AuthScreen, hostOf } from '#tv/shared/ui';
import { useFocusNav } from '#tv/app/useFocusNav';

interface Row {
  key: string;
  icon: ReactNode;
  iconAccent?: boolean;
  title: string;
  sub: string;
  /** Server origin for the live status dot, or undefined for the manual row. */
  url?: string;
  onSelect: () => void;
}

/** "host" or "host · port N" for a server URL. */
function addrOf(url: string): string {
  try {
    const u = new URL(url);
    return u.port ? `${u.hostname} · port ${u.port}` : u.hostname;
  } catch {
    return url;
  }
}

/**
 * Add-profile wizard, step 1 choose a server. One "Serveurs disponibles" list
 * (LAN-discovered + saved, with a discovery spinner) followed by "Ajouter
 * manuellement". Picking any of them points the client at it and advances to
 * Quick Connect. The wizard never offers a password or registration.
 */
export function TvAddProfile() {
  const nav = useNav();
  const t = useT();
  const { servers, discovered, discovering, discover, addServer } = useConnection();
  useFocusNav({ onBack: nav.back, resetKey: discovered.length + servers.length });

  // Kick off (or refresh) LAN discovery when the wizard opens.
  // biome-ignore lint/correctness/useExhaustiveDependencies: run once on open.
  useEffect(() => discover(), []);

  const pick = (url: string, name?: string | null) => {
    addServer(url, name);
    nav.go('quick');
  };

  // A single "Serveurs disponibles" section: discovered servers first (tagged
  // "nouveau" when not yet saved), then any saved-but-not-discovered, then the
  // manual-entry row exactly the design's layout.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `pick` and `nav` are stable enough here; rows rebuild on the reactive inputs (discovered/servers/t) only.
  const rows = useMemo<Row[]>(() => {
    const localUrls = discovered.map(norm);
    const out: Row[] = [];
    for (const url of localUrls) {
      const saved = servers.find((s) => s.url === url);
      out.push({
        key: `srv-${url}`,
        icon: <IconServer2 size={24} stroke={1.7} />,
        title: saved?.name || (hostOf(url) ?? url),
        sub: saved ? addrOf(url) : `${addrOf(url)} · ${t('addProfile.new')}`,
        url,
        onSelect: () => pick(url, saved?.name),
      });
    }
    for (const s of servers.filter((sv) => !localUrls.includes(sv.url))) {
      out.push({
        key: `srv-${s.url}`,
        icon: <IconServer2 size={24} stroke={1.7} />,
        title: s.name || (hostOf(s.url) ?? s.url),
        sub: addrOf(s.url),
        url: s.url,
        onSelect: () => pick(s.url, s.name),
      });
    }
    out.push({
      key: 'manual',
      icon: <IconPlus size={24} stroke={1.7} />,
      iconAccent: true,
      title: t('addProfile.addManually'),
      sub: t('addProfile.addManuallySub'),
      onSelect: () => nav.go('connect'),
    });
    return out;
  }, [discovered, servers, t]);

  // Probe each listed server so a row shows whether it actually answers (a saved
  // server can be offline; a freshly discovered one is reachable but confirmed here).
  const health = useServersHealth(rows.map((r) => r.url).filter((u): u is string => !!u));

  return (
    <AuthScreen>
      <div className="w-full max-w-[720px]">
        <h1 className="m-0 mb-1.5 text-center font-display text-[40px] font-semibold">
          {t('addProfile.title')}
        </h1>
        <p className="m-0 mb-9 text-center font-sans text-[16px] font-medium text-dim">
          {t('addProfile.subtitle')}
        </p>

        <div className="mb-3 flex items-center gap-2.5">
          <span className="font-sans text-[12px] font-bold uppercase tracking-[0.16em] text-[rgba(244,243,240,0.42)]">
            {t('addProfile.availableServers')}
          </span>
          {discovering ? (
            <span className="h-3.25 w-3.25 rounded-full border-2 border-[rgba(244,180,66,0.3)] border-t-accent animate-[tvp-spin_0.8s_linear_infinite]" />
          ) : null}
        </div>
        <div className="flex flex-col gap-3">
          {rows.map((r) => (
            <button
              key={r.key}
              data-focus=""
              type="button"
              onClick={r.onSelect}
              className="flex items-center gap-4 rounded-[15px] border border-border bg-[rgba(255,255,255,0.03)] px-5 py-4 text-left outline-none transition-transform focus:scale-[1.02] focus:border-accent"
            >
              <span
                className={`flex h-11.5 w-11.5 flex-none items-center justify-center rounded-xl ${
                  r.iconAccent
                    ? 'bg-accent-soft text-accent'
                    : 'bg-[rgba(255,255,255,0.06)] text-muted'
                }`}
              >
                {r.icon}
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate font-sans text-[19px] font-bold text-text">
                  {r.title}
                </span>
                <span className="block truncate font-sans text-[14px] font-medium text-dim">
                  {r.sub}
                </span>
              </span>
              {r.url ? <StatusDot online={health[r.url]} /> : null}
              <IconChevronRight size={22} className="flex-none text-dim" />
            </button>
          ))}
        </div>

        <div className="mt-7 text-center font-sans text-[14px] font-medium text-[rgba(244,243,240,0.4)]">
          {t('addProfile.navHint')}
        </div>
      </div>
    </AuthScreen>
  );
}
