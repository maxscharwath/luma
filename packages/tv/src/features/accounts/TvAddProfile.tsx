import { normalizeServerUrl as norm } from '@kroma/core';
import { useT } from '@kroma/ui';
import { Box, Focusable, Icon, type IconName, Spinner, Txt, useFocusNav } from '@kroma/ui/kit';
import { useEffect, useMemo } from 'react';
import { useConnection } from '#tv/app/providers/connection';
import { useNav } from '#tv/app/router';
import { useServersHealth } from '#tv/app/useServersHealth';
import { StatusDot } from '#tv/features/accounts/ServerStatus';
import { AuthScreen, hostOf } from '#tv/shared/ui';

interface Row {
  key: string;
  icon: IconName;
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
    const localUrls = discovered.map((u) => norm(u));
    const out: Row[] = [];
    for (const url of localUrls) {
      const saved = servers.find((s) => s.url === url);
      out.push({
        key: `srv-${url}`,
        icon: 'server-2',
        title: saved?.name || (hostOf(url) ?? url),
        sub: saved ? addrOf(url) : `${addrOf(url)} · ${t('addProfile.new')}`,
        url,
        onSelect: () => pick(url, saved?.name),
      });
    }
    for (const s of servers.filter((sv) => !localUrls.includes(sv.url))) {
      out.push({
        key: `srv-${s.url}`,
        icon: 'server-2',
        title: s.name || (hostOf(s.url) ?? s.url),
        sub: addrOf(s.url),
        url: s.url,
        onSelect: () => pick(s.url, s.name),
      });
    }
    out.push({
      key: 'manual',
      icon: 'plus',
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
      <Box w="100%" maxW={720}>
        <Txt
          variant="h1"
          style={{ fontSize: 40, fontWeight: '600', textAlign: 'center', marginBottom: 6 }}
        >
          {t('addProfile.title')}
        </Txt>
        <Txt
          style={{ fontSize: 16, fontWeight: '500', textAlign: 'center', marginBottom: 36 }}
          color="textDim"
        >
          {t('addProfile.subtitle')}
        </Txt>

        <Box row align="center" gap={10} mb={12}>
          <Txt style={SECTION} color="rgba(244, 243, 240, 0.42)">
            {t('addProfile.availableServers')}
          </Txt>
          {discovering ? <Spinner size={13} thickness={2} /> : null}
        </Box>
        <Box gap={12}>
          {rows.map((r) => (
            <Focusable
              key={r.key}
              onPress={r.onSelect}
              label={r.title}
              focusScale={1.02}
              ring={false}
              style={ROW}
              focusedStyle={{ borderColor: '#F4B642' }}
            >
              <Box
                w={46}
                h={46}
                shrink={0}
                center
                radius="xl"
                bg={r.iconAccent ? 'accentSoft' : 'rgba(255, 255, 255, 0.06)'}
              >
                <Icon
                  name={r.icon}
                  size={24}
                  stroke={1.7}
                  color={r.iconAccent ? 'accent' : 'textMuted'}
                />
              </Box>
              <Box flex style={{ minWidth: 0 }}>
                <Txt lines={1} style={{ fontSize: 19, fontWeight: '700' }}>
                  {r.title}
                </Txt>
                <Txt lines={1} style={{ fontSize: 14, fontWeight: '500' }} color="textDim">
                  {r.sub}
                </Txt>
              </Box>
              {r.url ? <StatusDot online={health[r.url]} /> : null}
              <Icon name="chevron-right" size={22} color="textDim" />
            </Focusable>
          ))}
        </Box>

        <Txt
          style={{ fontSize: 14, fontWeight: '500', textAlign: 'center', marginTop: 28 }}
          color="rgba(244, 243, 240, 0.4)"
        >
          {t('addProfile.navHint')}
        </Txt>
      </Box>
    </AuthScreen>
  );
}

const SECTION = {
  fontSize: 12,
  fontWeight: '700' as const,
  letterSpacing: 1.92,
  textTransform: 'uppercase' as const,
};

const ROW = {
  flexDirection: 'row' as const,
  alignItems: 'center' as const,
  gap: 16,
  borderRadius: 15,
  borderWidth: 1,
  borderColor: 'rgba(255, 255, 255, 0.08)',
  backgroundColor: 'rgba(255, 255, 255, 0.03)',
  paddingHorizontal: 20,
  paddingVertical: 16,
};
