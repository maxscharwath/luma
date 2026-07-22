import type { AuthResult, KromaClient, MessageKey, QuickConnectInit } from '@kroma/core';
import { useT } from '@kroma/ui';
import { useFocusNav } from '@kroma/ui/kit';
import { useEffect, useState } from 'react';
import { useAuth } from '#tv/app/providers/auth';
import { useConnection } from '#tv/app/providers/connection';
import { useNav } from '#tv/app/router';
import { AuthScreen, KromaMark } from '#tv/shared/ui';

/** Regenerate the code this many seconds before the server-side TTL lapses. */
const EXPIRY_MARGIN_SEC = 5;

/**
 * Quick Connect (route `quick`) against the active server: shows a code + QR; an
 * already-signed-in user approves it from the web/mobile app and the TV pairs the
 * profile on its next poll no password typed on the remote.
 */
export function TvQuickConnect() {
  const nav = useNav();
  const t = useT();
  const { client, activeServerUrl, activeServerName } = useConnection();
  const { login } = useAuth();
  const [info, setInfo] = useState<QuickConnectInit | null>(null);
  const [qr, setQr] = useState<string | null>(null);
  const [error, setError] = useState<MessageKey | ''>('');
  useFocusNav({ onBack: nav.back });

  useEffect(() => {
    if (!client || !activeServerUrl) return;
    let cancelled = false;
    let pollTimer: ReturnType<typeof setTimeout> | undefined;
    let expireTimer: ReturnType<typeof setTimeout> | undefined;
    let secret = '';

    const clearTimers = () => {
      if (pollTimer) clearTimeout(pollTimer);
      if (expireTimer) clearTimeout(expireTimer);
      pollTimer = undefined;
      expireTimer = undefined;
    };

    const onAuthenticated = (res: AuthResult) => login(res, activeServerUrl);

    const poll = async () => {
      if (cancelled) return;
      try {
        const res = await client.quickConnectPoll(secret);
        if (cancelled) return;
        if (res.status === 'authorized') {
          clearTimers();
          onAuthenticated({ token: res.token, accessToken: res.accessToken, user: res.user });
          return;
        }
        if (res.status === 'expired') {
          void begin();
          return;
        }
      } catch {
        /* transient keep polling */
      }
      pollTimer = setTimeout(poll, 2500);
    };

    const begin = async () => {
      clearTimers();
      try {
        // On a rotation, hand the server the code we're leaving so it revokes it
        // instead of letting it linger approvable until its own TTL lapses.
        const init = await client.quickConnectInitiate(secret || undefined);
        if (cancelled) return;
        secret = init.secret;
        setInfo(init);
        setQr(null);
        const url = connectUrl(client, init.code, init.authorizeUrl);
        if (url) {
          void import('qrcode-generator')
            .then((mod) => {
              if (cancelled) return;
              const qrc = mod.default(0, 'M');
              qrc.addData(url);
              qrc.make();
              setQr(qrc.createSvgTag({ cellSize: 6, margin: 1, scalable: true }));
            })
            .catch(() => undefined);
        }
        // Proactively mint a fresh code a touch before the server TTL lapses, so
        // the code on screen is always valid to approve (independent of the poll
        // loop, which only learns of expiry after the server has already reaped).
        const marginSec = Math.min(EXPIRY_MARGIN_SEC, Math.floor(init.expiresInSec / 2));
        const renewMs = Math.max(1000, (init.expiresInSec - marginSec) * 1000);
        expireTimer = setTimeout(() => void begin(), renewMs);
        pollTimer = setTimeout(poll, 2500);
      } catch {
        if (!cancelled) setError('connect.quickConnectUnavailable');
      }
    };

    void begin();
    return () => {
      cancelled = true;
      clearTimers();
    };
  }, [client, activeServerUrl, login]);

  return (
    <AuthScreen>
      <div className="mb-9">
        <KromaMark size={40} />
      </div>
      <h1 className="m-0 mb-4 font-display text-[44px] font-semibold leading-none">
        {t('connect.quickConnect')}
      </h1>
      <div className="mb-7 inline-flex items-center gap-2.5 rounded-full border border-border bg-[rgba(255,255,255,0.05)] px-4 py-2.25">
        <span className="h-2 w-2 rounded-full bg-accent" />
        <span className="font-sans text-[15px] font-semibold text-[rgba(244,243,240,0.88)]">
          {activeServerName ?? 'KROMA'}
        </span>
      </div>

      {error ? <p className="font-sans text-[16px] text-danger">{t(error)}</p> : null}

      {!error && info ? (
        <>
          {qr ? (
            <div
              className="flex h-[280px] w-[280px] items-center justify-center rounded-[28px] bg-white p-5 shadow-pop [&>svg]:h-full [&>svg]:w-full"
              // biome-ignore lint/security/noDangerouslySetInnerHtml: app-generated QR SVG built by qrcode-generator from a trusted server URL + server-issued code, never user input.
              dangerouslySetInnerHTML={{ __html: qr }}
            />
          ) : null}
          <div className="mt-5 font-sans text-[17px] font-medium text-dim">
            {t('connect.scanQrConnected')}
          </div>
          <div className="mt-6 font-sans text-[17px] font-medium text-muted">
            {t('connect.orInAppPrefix')}
            <b className="text-text">{t('nav.connectDevice')}</b>
            {t('connect.orInAppSuffix')}
          </div>
          <div className="mt-5 flex gap-7 font-display text-[96px] font-bold leading-none text-accent tabular-nums">
            {info.code}
          </div>
          <div className="mt-7 inline-flex items-center gap-2.5 rounded-full border border-[rgba(70,208,141,0.25)] bg-[rgba(70,208,141,0.1)] px-4.5 py-2.5">
            <span className="h-2.25 w-2.25 rounded-full bg-success animate-[tv-breathe_1.6s_ease-in-out_infinite]" />
            <span className="font-sans text-[14px] font-semibold text-success">
              {t('connect.waitingApproval')}
            </span>
          </div>
        </>
      ) : null}
      {!error && !info ? (
        <div className="h-10 w-10 rounded-full border-[3px] border-[rgba(255,255,255,0.2)] border-t-accent animate-[tvp-spin_0.9s_linear_infinite]" />
      ) : null}

      <p className="mt-6 font-sans text-[15px] font-medium text-dim">
        {t('connect.backToProfiles')}
      </p>
    </AuthScreen>
  );
}

/**
 * Resolve the web `/connect?code=` URL for the QR. The server-advertised URL
 * wins; otherwise fall back to the API origin, which also serves the web SPA
 * in production (single-binary installs).
 */
function connectUrl(client: KromaClient, code: string, serverUrl?: string | null): string {
  if (serverUrl) return serverUrl;
  try {
    const u = new URL(client.baseUrl);
    u.pathname = '/connect';
    u.search = `?code=${code}`;
    return u.toString();
  } catch {
    return '';
  }
}
