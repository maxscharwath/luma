import { KromaApiError, KromaClient, type MessageKey } from '@kroma/core';
import { useT } from '@kroma/ui';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useAuth } from '#tv/app/providers/auth';
import { useConnection } from '#tv/app/providers/connection';
import { useEnv } from '#tv/app/providers/env';
import { useNav, useParams } from '#tv/app/router';
import { useFocusNav } from '#tv/app/useFocusNav';
import { AuthScreen, artUrl, Keypad, LockGlyph, ProfileAvatar } from '#tv/shared/ui';

/** PINs are a fixed 4 digits; the last digit auto-validates (no OK press). */
const PIN_LENGTH = 4;

/**
 * PIN entry. Three intents share one keypad:
 *  • `verify` unlock a remembered, PIN-protected profile from the picker. Uses
 *    that account's remembered token to call `pinVerify`, then activates it.
 *  • `set` set the active account's PIN (enter, then confirm).
 *  • `clear` remove the active account's PIN (enter the current one).
 *
 * The keypad has no OK button: entering the fourth digit submits automatically,
 * so a completed PIN validates or is rejected the instant it is typed.
 */
export function TvPin() {
  const nav = useNav();
  const t = useT();
  const { intent, account } = useParams('pin');
  const { client: activeClient } = useConnection();
  const { user: activeUser, activate, updateUser } = useAuth();

  // For `verify`, talk to the account's own server. The bearer is minted on
  // demand by exchanging the account's access token (see `submit`).
  const verifyClient = useMemo(
    () => (account?.serverUrl ? new KromaClient({ baseUrl: account.serverUrl }) : null),
    [account],
  );

  const [buffer, setBuffer] = useState('');
  const [first, setFirst] = useState<string | null>(null); // 'set' confirm step
  const [error, setError] = useState<MessageKey | ''>('');
  const [shake, setShake] = useState(0);
  const [busy, setBusy] = useState(false);
  const [cooldown, setCooldown] = useState(0);

  useFocusNav({ onBack: nav.back });

  // Lockout countdown (PIN verify rate-limit).
  useEffect(() => {
    if (cooldown <= 0) return;
    const id = setInterval(() => setCooldown((c) => Math.max(0, c - 1)), 1000);
    return () => clearInterval(id);
  }, [cooldown]);

  let headerUser: { name: string; seed: string; src?: string | null } | null = null;
  if (intent === 'verify' && account) {
    headerUser = {
      name: account.user.username,
      seed: account.user.id,
      src: artUrl(account.serverUrl ?? '', account.user.avatarUrl),
    };
  } else if (activeUser) {
    headerUser = {
      name: activeUser.username,
      seed: activeUser.id,
      src: activeClient?.resolveArt(activeUser.avatarUrl),
    };
  }

  let subtitle: MessageKey = 'pin.verifySubtitle';
  if (intent === 'set') subtitle = first == null ? 'pin.setSubtitle' : 'pin.confirmSubtitle';
  else if (intent === 'clear') subtitle = 'pin.clearSubtitle';

  const fail = (key: MessageKey) => {
    setError(key);
    setBuffer('');
    setShake((s) => s + 1);
  };

  const submit = async () => {
    if (busy || cooldown > 0) return;
    const pin = buffer;
    if (pin.length < PIN_LENGTH) return; // auto-submit only fires on a full PIN
    setError('');
    setBusy(true);
    try {
      if (intent === 'verify') {
        if (!verifyClient || !account) return;
        // Exchange the access token for a session bearer, passing the PIN so a
        // not-yet-pin-verified token (after a PIN change/reset) doesn't 401 before
        // we can check it. pinVerify is still the authoritative gate it also
        // rejects a wrong PIN when the token is already pin-verified server-side
        // (where the exchange would skip the check).
        const sess = await verifyClient.exchangeToken(account.accessToken, pin);
        verifyClient.setAuthToken(sess.token);
        await verifyClient.pinVerify(pin);
        activate(account); // clears the lock + signs in for this session
        nav.home(); // `pin` is allowed while signed in (set/clear), so move on explicitly
      } else if (intent === 'set') {
        if (first == null) {
          setFirst(pin);
          setBuffer('');
          return;
        }
        if (pin !== first) {
          setFirst(null);
          fail('pin.mismatch');
          return;
        }
        const res = await activeClient?.setPin(pin);
        if (!res) return; // no client (offline) don't fake success
        updateUser(res.user); // trust the server's returned user (hasPin: true)
        nav.back();
      } else {
        const res = await activeClient?.clearPin(pin);
        if (!res) return; // no client (offline) don't fake a disabled PIN
        updateUser(res.user); // trust the server's returned user (hasPin: false)
        nav.back();
      }
    } catch (e) {
      if (e instanceof KromaApiError && e.status === 429) {
        const secs = Number((e.body as { retryAfter?: number } | undefined)?.retryAfter ?? 30);
        setCooldown(secs);
        fail('auth.pinLocked');
      } else if (intent === 'verify' || intent === 'clear') {
        fail(intent === 'clear' ? 'auth.pinCurrentWrong' : 'auth.pinIncorrect');
      } else {
        fail('auth.pinInvalid');
      }
    } finally {
      setBusy(false);
    }
  };

  // Auto-validate the instant the PIN is complete, so no OK press is needed.
  // biome-ignore lint/correctness/useExhaustiveDependencies: fire only when the buffer fills; `submit` reads fresh state via closure.
  useEffect(() => {
    if (buffer.length === PIN_LENGTH) void submit();
  }, [buffer]);

  const addDigit = (d: string) => {
    if (busy || cooldown > 0) return;
    setError('');
    setBuffer((b) => (b.length < PIN_LENGTH ? b + d : b));
  };

  const removeDigit = () => {
    if (busy || cooldown > 0) return;
    setBuffer((b) => b.slice(0, -1));
  };

  // Desktop: type the PIN with the number-row / numpad (Delete edits). Digits and
  // Delete aren't remote keys, so they don't collide with useFocusNav's arrows /
  // Back; on a TV (on-screen keypad) this listener never attaches. A ref carries
  // the lock so the listener stays stable (no re-subscribe per keystroke).
  const { physicalKeyboard } = useEnv();
  const locked = useRef(false);
  locked.current = busy || cooldown > 0;
  useEffect(() => {
    if (!physicalKeyboard) return;
    const onKey = (e: KeyboardEvent) => {
      if (locked.current) return;
      if (/^[0-9]$/.test(e.key)) {
        setError('');
        setBuffer((b) => (b.length < PIN_LENGTH ? b + e.key : b));
      } else if (e.key === 'Delete') {
        setBuffer((b) => b.slice(0, -1));
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [physicalKeyboard]);

  return (
    <AuthScreen>
      {headerUser ? (
        <ProfileAvatar
          name={headerUser.name}
          seed={headerUser.seed}
          size={118}
          radius={30}
          src={headerUser.src}
        />
      ) : null}
      <h1 className="m-0 mt-6 mb-1 font-display text-[32px] font-semibold">{headerUser?.name}</h1>
      <div className="flex items-center gap-2 font-sans text-[15px] font-medium text-dim">
        <span className="text-accent">
          <LockGlyph size={14} />
        </span>
        {t(subtitle)}
      </div>

      <div
        key={shake}
        className={`mt-8 flex gap-4.5 ${shake ? 'animate-[tv-shake_0.4s_ease]' : ''} ${busy ? 'animate-pulse' : ''}`}
      >
        {Array.from({ length: PIN_LENGTH }).map((_, i) => (
          <span
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed-length dot row
            key={i}
            className="h-4.5 w-4.5 rounded-full border-2 transition-all"
            style={{
              background: i < buffer.length ? '#F4B642' : 'transparent',
              borderColor: i < buffer.length ? '#F4B642' : 'rgba(255,255,255,0.25)',
            }}
          />
        ))}
      </div>

      <div className="flex h-6 items-center gap-2">
        {busy ? (
          <>
            <span className="h-4 w-4 animate-spin rounded-full border-2 border-[rgba(255,255,255,0.25)] border-t-accent" />
            <span className="font-sans text-[14px] font-medium text-dim">{t('pin.verifying')}</span>
          </>
        ) : null}
        {!busy && error ? (
          <span className="font-sans text-[14px] font-semibold text-danger">
            {error === 'auth.pinLocked' && cooldown > 0
              ? t('pin.lockedRetry', { seconds: cooldown })
              : t(error)}
          </span>
        ) : null}
      </div>

      <div className="mt-2">
        <Keypad onDigit={addDigit} onDelete={removeDigit} />
      </div>

      <span className="mt-7 font-sans text-[14px] font-medium text-[rgba(244,243,240,0.38)]">
        {t('pin.backHint')}
      </span>
    </AuthScreen>
  );
}
