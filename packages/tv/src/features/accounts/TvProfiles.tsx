import { normalizeServerUrl as norm, type StoredSession } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconPlus } from '@tabler/icons-react';
import { useMemo } from 'react';
import { useAuth } from '#tv/app/providers/auth';
import { useConnection } from '#tv/app/providers/connection';
import { useNav } from '#tv/app/router';
import { useServersHealth } from '#tv/app/useServersHealth';
import { StatusDot } from '#tv/features/accounts/ServerStatus';
import { artUrl, AuthScreen, hostOf, KromaMark, ProfileAvatar } from '#tv/shared/ui';
import { useFocusNav } from '#tv/app/useFocusNav';

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
      <div className="mb-7">
        <KromaMark size={34} />
      </div>
      <h1 className="m-0 mb-3 font-display text-[50px] font-semibold leading-none">
        {t('auth.whoWatching')}
      </h1>
      <p className="m-0 mb-11 font-sans text-[17px] font-medium text-dim">
        {t('profiles.subtitle')}
      </p>

      {/* No own scroll/clip the page (AuthScreen) scrolls, so focus zoom + the
          amber ring/glow are never cropped. Gutters keep edge tiles' rings clear. */}
      <div className="flex w-full max-w-[1100px] flex-wrap content-start items-start justify-center gap-x-7 gap-y-9 px-6 py-4">
        {tiles.map(({ key, account, serverName }) => {
          const up = health[norm(account.serverUrl ?? '')];
          const offline = up === false;
          return (
            <div key={key} className="flex w-[150px] flex-col items-center gap-3">
              <button
                data-focus=""
                type="button"
                onClick={() => onSelect(account, offline)}
                aria-disabled={offline}
                className={`relative rounded-3xl border-none bg-transparent p-0 outline-none transition-transform focus:scale-[1.07] ${
                  offline ? 'cursor-not-allowed' : 'cursor-pointer'
                }`}
              >
                <div className={offline ? 'opacity-40 grayscale' : ''}>
                  <ProfileAvatar
                    name={account.user.username}
                    seed={account.user.id}
                    size={146}
                    radius={24}
                    src={artUrl(norm(account.serverUrl), account.user.avatarUrl)}
                    locked={account.user.hasPin}
                  />
                </div>
              </button>
              <div className="flex flex-col items-center gap-1.25">
                <span className="font-sans text-[18px] font-medium text-[rgba(244,243,240,0.82)]">
                  {account.user.username}
                </span>
                <span
                  className={`inline-flex items-center gap-1.5 font-sans text-[12px] font-semibold ${
                    offline ? 'text-danger' : 'text-[rgba(244,243,240,0.42)]'
                  }`}
                >
                  <StatusDot online={up} />
                  {offline ? t('connection.offline') : serverName}
                </span>
              </div>
            </div>
          );
        })}

        <div className="flex w-[150px] flex-col items-center gap-3">
          <button
            data-focus=""
            type="button"
            onClick={() => nav.go('addProfile')}
            className="flex h-[146px] w-[146px] cursor-pointer items-center justify-center rounded-3xl border-2 border-dashed border-[rgba(255,255,255,0.18)] bg-transparent text-[rgba(255,255,255,0.35)] outline-none transition-transform focus:scale-[1.07] focus:border-accent focus:text-accent"
          >
            <IconPlus size={46} stroke={1.6} />
          </button>
          <span className="font-sans text-[18px] font-medium text-[rgba(244,243,240,0.5)]">
            {t('profiles.addProfile')}
          </span>
        </div>
      </div>

      <div className="mt-9 flex items-center gap-4 font-sans text-[14px] font-semibold tracking-[0.03em] text-[rgba(244,243,240,0.4)]">
        {t('profiles.navHint')}
      </div>
    </AuthScreen>
  );
}
