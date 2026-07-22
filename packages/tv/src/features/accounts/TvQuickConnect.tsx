import type { AuthResult, KromaClient, MessageKey, QuickConnectInit } from '@kroma/core';
import { useT } from '@kroma/ui';
import { Box, Spinner, SvgXml, Txt, useFocusNav } from '@kroma/ui/kit';
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
      <Box mb={36}>
        <KromaMark size={40} />
      </Box>
      <Txt
        variant="hero"
        style={{ fontSize: 44, lineHeight: 44, fontWeight: '600', marginBottom: 16 }}
      >
        {t('connect.quickConnect')}
      </Txt>
      <Box
        row
        align="center"
        gap={10}
        mb={28}
        px={16}
        py={9}
        radius="pill"
        border="border"
        bg="rgba(255, 255, 255, 0.05)"
      >
        <Box w={8} h={8} radius="pill" bg="accent" />
        <Txt style={{ fontSize: 15, fontWeight: '600' }} color="rgba(244, 243, 240, 0.88)">
          {activeServerName ?? 'KROMA'}
        </Txt>
      </Box>

      {error ? (
        <Txt style={{ fontSize: 16 }} color="danger">
          {t(error)}
        </Txt>
      ) : null}

      {!error && info ? (
        <>
          {qr ? (
            <Box w={280} h={280} center radius={28} bg="#FFFFFF" p={20} shadow="pop">
              <SvgXml xml={qr} width="100%" height="100%" />
            </Box>
          ) : null}
          <Txt style={{ fontSize: 17, fontWeight: '500', marginTop: 20 }} color="textDim">
            {t('connect.scanQrConnected')}
          </Txt>
          <Txt
            style={{ fontSize: 17, fontWeight: '500', marginTop: 24, textAlign: 'center' }}
            color="textMuted"
          >
            {t('connect.orInAppPrefix')}
            <Txt style={{ fontSize: 17, fontWeight: '700' }}>{t('nav.connectDevice')}</Txt>
            {t('connect.orInAppSuffix')}
          </Txt>
          <Txt style={CODE} color="accent">
            {info.code}
          </Txt>
          <Box
            row
            align="center"
            gap={10}
            mt={28}
            px={18}
            py={10}
            radius="pill"
            border="rgba(70, 208, 141, 0.25)"
            bg="rgba(70, 208, 141, 0.1)"
          >
            <Box w={9} h={9} radius="pill" bg="success" />
            <Txt style={{ fontSize: 14, fontWeight: '600' }} color="success">
              {t('connect.waitingApproval')}
            </Txt>
          </Box>
        </>
      ) : null}
      {!error && !info ? <Spinner size={40} thickness={3} /> : null}

      <Txt style={{ fontSize: 15, fontWeight: '500', marginTop: 24 }} color="textDim">
        {t('connect.backToProfiles')}
      </Txt>
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

const CODE = {
  fontSize: 96,
  lineHeight: 96,
  fontWeight: '700' as const,
  letterSpacing: 8,
  marginTop: 20,
  fontVariant: ['tabular-nums' as const],
};
