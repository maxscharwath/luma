import { normalizeServerUrl as norm, type StoredSession } from '@kroma/core';
import { useT } from '@kroma/ui';
import { Box, Chip, Focusable, Icon, Txt, useFocusNav } from '@kroma/ui/kit';
import { useMemo } from 'react';
import { useAuth } from '#tv/app/providers/auth';
import { useConnection } from '#tv/app/providers/connection';
import { useNav } from '#tv/app/router';
import { useServersHealth } from '#tv/app/useServersHealth';
import { StatusDot } from '#tv/features/accounts/ServerStatus';
import { AuthScreen, artUrl, hostOf, KromaMark, ProfileAvatar } from '#tv/shared/ui';

interface Tile {
  key: string;
  account: StoredSession;
  serverName: string;
}

/**
 * Profile picker the signed-out home. It shows ONLY the profiles paired on this
 * device (remembered accounts); it never lists the server's other accounts and
 * makes no request on open. A PIN-protected profile routes to the PIN screen, the
 * rest sign in instantly. "Ajouter un profil" opens the wizard to pair a new one.
 */
export function TvProfiles() {
  const nav = useNav();
  const t = useT();
  const { servers, activeServerName } = useConnection();
  const { accounts, activate, isUnlocked } = useAuth();

  const tiles = useMemo<Tile[]>(() => {
    const nameFor = (url: string) =>
      servers.find((s) => s.url === norm(url))?.name ||
      hostOf(norm(url)) ||
      (activeServerName ?? 'KROMA');
    return accounts.map((a) => ({
      key: `${norm(a.serverUrl)}|${a.user.id}`,
      account: a,
      serverName: nameFor(a.serverUrl ?? ''),
    }));
  }, [accounts, servers, activeServerName]);

  // Probe each distinct server behind the remembered profiles so a tile can show
  // whether its server is reachable BEFORE you pick it (public /api/health, no
  // auth needed while signed out).
  const serverUrls = useMemo(
    () => Array.from(new Set(accounts.map((a) => norm(a.serverUrl ?? '')).filter(Boolean))),
    [accounts],
  );
  const health = useServersHealth(serverUrls);

  useFocusNav({ onBack: nav.back, resetKey: tiles.length });

  const onSelect = (a: StoredSession, offline: boolean) => {
    // Signing in would only fail (catalogue fetch, progress sync) against a
    // server that isn't answering don't let the profile "connect" into a dead end.
    if (offline) return;
    const locked = a.user.hasPin && !isUnlocked(a);
    if (locked) nav.go('pin', { intent: 'verify', account: a });
    else activate(a);
  };

  return (
    <AuthScreen>
      <Box mb={28}>
        <KromaMark size={34} />
      </Box>
      <Txt
        variant="hero"
        style={{ fontSize: 50, lineHeight: 50, fontWeight: '600', marginBottom: 12 }}
      >
        {t('auth.whoWatching')}
      </Txt>
      <Txt style={{ fontSize: 17, fontWeight: '500', marginBottom: 44 }} color="textDim">
        {t('profiles.subtitle')}
      </Txt>

      {/* No own scroll or clip: the page (AuthScreen) scrolls, so the focus zoom
          and the amber ring are never cropped. Gutters keep the edge tiles'
          rings clear. */}
      <Box
        row
        wrap
        justify="center"
        align="flex-start"
        gap={28}
        w="100%"
        maxW={1100}
        px={24}
        py={16}
      >
        {tiles.map(({ key, account, serverName }) => {
          const up = health[norm(account.serverUrl ?? '')];
          const offline = up === false;
          return (
            <Box key={key} w={150} align="center" gap={12}>
              <Focusable
                onPress={() => onSelect(account, offline)}
                label={account.user.username}
                focusScale={1.07}
                style={{ borderRadius: 24 }}
              >
                <Box opacity={offline ? 0.4 : 1}>
                  <ProfileAvatar
                    name={account.user.username}
                    seed={account.user.id}
                    size={146}
                    radius={24}
                    src={artUrl(norm(account.serverUrl), account.user.avatarUrl)}
                    locked={account.user.hasPin}
                  />
                </Box>
              </Focusable>
              <Box align="center" gap={5}>
                <Txt style={{ fontSize: 18, fontWeight: '500' }} color="rgba(244, 243, 240, 0.82)">
                  {account.user.username}
                </Txt>
                <Box row align="center" gap={6}>
                  <StatusDot online={up} />
                  <Txt
                    style={{ fontSize: 12, fontWeight: '600' }}
                    color={offline ? 'danger' : 'rgba(244, 243, 240, 0.42)'}
                  >
                    {offline ? t('connection.offline') : serverName}
                  </Txt>
                </Box>
              </Box>
            </Box>
          );
        })}

        <Box w={150} align="center" gap={12}>
          <Focusable
            onPress={() => nav.go('addProfile')}
            label={t('profiles.addProfile')}
            focusScale={1.07}
            ring={false}
            style={ADD_TILE}
            focusedStyle={{ borderColor: '#F4B642' }}
          >
            {({ focused }) => (
              <Icon
                name="plus"
                size={46}
                stroke={1.6}
                color={focused ? 'accent' : 'rgba(255, 255, 255, 0.35)'}
              />
            )}
          </Focusable>
          <Txt style={{ fontSize: 18, fontWeight: '500' }} color="rgba(244, 243, 240, 0.5)">
            {t('profiles.addProfile')}
          </Txt>
        </Box>
      </Box>

      {/* Device settings (language, keyboard, desktop extras) must stay reachable
          while signed out: there is no profile menu yet. */}
      <Box mt={40}>
        <Chip
          variant="subtle"
          icon="settings"
          focusScale={1.04}
          label={t('profiles.deviceSettings')}
          onPress={() => nav.go('deviceSettings')}
          style={{ paddingHorizontal: 18, paddingVertical: 10, borderWidth: 1 }}
        />
      </Box>

      <Txt style={NAV_HINT} color="rgba(244, 243, 240, 0.4)">
        {t('profiles.navHint')}
      </Txt>
    </AuthScreen>
  );
}

const ADD_TILE = {
  width: 146,
  height: 146,
  alignItems: 'center' as const,
  justifyContent: 'center' as const,
  borderRadius: 24,
  borderWidth: 2,
  borderStyle: 'dashed' as const,
  borderColor: 'rgba(255, 255, 255, 0.18)',
};

const NAV_HINT = {
  fontSize: 14,
  fontWeight: '600' as const,
  letterSpacing: 0.42,
  marginTop: 24,
};
