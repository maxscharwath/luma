import { LumaApiError, LumaClient, type MessageKey } from '@luma/core';
import { useT } from '@luma/ui';
import { useEffect, useMemo, useState } from 'react';
import { useAuth } from '#tv/app/providers/auth';
import { useConnection } from '#tv/app/providers/connection';
import { useNav, useParams } from '#tv/app/router';
import { artUrl, AuthScreen, Keypad, LockGlyph, ProfileAvatar } from '#tv/shared/ui';
import { useFocusNav } from '#tv/app/useFocusNav';

/**
 * PIN entry. Three intents share one keypad:
 *  • `verify` unlock a remembered, PIN-protected profile from the picker. Uses
 *    that account's remembered token to call `pinVerify`, then activates it.
 *  • `set` set the active account's PIN (enter, then confirm).
 *  • `clear` remove the active account's PIN (enter the current one).
 */
export function TvPin() {
  const nav = useNav();
  const t = useT();
  const { intent, account } = useParams('pin');
  const { client: activeClient } = useConnection();
  const { user: activeUser, activate, updateUser } = useAuth();

  // For `verify`, talk to the account's own server with its remembered token.
  const verifyClient = useMemo(
    () =>
      account?.serverUrl
        ? new LumaClient({ baseUrl: account.serverUrl, authToken: account.token })
        : null,
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

  const headerUser =
    intent === 'verify' && account
      ? {
          name: account.user.username,
          seed: account.user.id,
          src: artUrl(account.serverUrl ?? '', account.user.avatarUrl),
        }
      : activeUser
        ? {
            name: activeUser.username,
            seed: activeUser.id,
            src: activeClient?.resolveArt(activeUser.avatarUrl),
          }
        : null;

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
    if (pin.length < 4) {
      fail('auth.pinInvalid');
      return;
    }
    setError('');
    setBusy(true);
    try {
      if (intent === 'verify') {
        if (!verifyClient || !account) return;
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
        await activeClient?.setPin(pin);
        updateUser({ hasPin: true });
        nav.back();
      } else {
        await activeClient?.clearPin(pin);
        updateUser({ hasPin: false });
        nav.back();
      }
    } catch (e) {
      if (e instanceof LumaApiError && e.status === 429) {
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

  const dots = Math.max(4, buffer.length);

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
        className={`mt-8 flex gap-4.5 ${shake ? 'animate-[tv-shake_0.4s_ease]' : ''}`}
      >
        {Array.from({ length: dots }).map((_, i) => (
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

      <div className="flex h-6 items-center">
        {error ? (
          <span className="font-sans text-[14px] font-semibold text-danger">
            {error === 'auth.pinLocked' && cooldown > 0
              ? t('pin.lockedRetry', { seconds: cooldown })
              : t(error)}
          </span>
        ) : null}
      </div>

      <div className="mt-2">
        <Keypad
          onDigit={(d) => cooldown === 0 && setBuffer((b) => (b.length < 6 ? b + d : b))}
          onDelete={() => setBuffer((b) => b.slice(0, -1))}
          onSubmit={() => void submit()}
        />
      </div>

      <span className="mt-7 font-sans text-[14px] font-medium text-[rgba(244,243,240,0.38)]">
        {t('pin.backHint')}
      </span>
    </AuthScreen>
  );
}
