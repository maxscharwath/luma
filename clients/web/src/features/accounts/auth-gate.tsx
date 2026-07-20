// The login gate. Rendered as a full-screen overlay by the root layout whenever
// no session is active, so the catalogue underneath is never usable until a real
// account is chosen. Visual design follows KROMA.dc.html's "Qui regarde ?" screen
// (rounded-square gradient avatars, Bricolage headings) while keeping real
// account semantics: selecting a profile asks for its password, and new accounts
// are created with email + username + password + an optional uploaded avatar.

import {
  apiErrorText,
  KromaApiError,
  type PublicUser,
  type StoredSession,
  UserId,
} from '@kroma/core';
import { type ActivateResult, Logo, useT } from '@kroma/ui';
import { IconLock, IconPlus } from '@tabler/icons-react';
import { type ReactNode, useEffect, useState } from 'react';
import { LoginForm, RegisterForm } from '#web/features/accounts/auth-forms';
import { UserAvatar } from '#web/features/accounts/user-avatar';
import { useAuth } from '#web/shared/lib/auth';
import { passkeysSupported } from '#web/shared/lib/webauthn';
import { Otp } from '#web/shared/ui';

type Mode =
  // `expired` marks a re-login after a remembered profile's session lapsed, so
  // the form can explain (calmly) why they're being asked to sign in again.
  | { kind: 'pick' }
  | { kind: 'login'; user: PublicUser | null; expired?: boolean }
  | { kind: 'register' }
  | { kind: 'pin'; account: StoredSession };

export const RADIAL = 'radial-gradient(120% 90% at 50% 0%, #15131C, #0A0A0C 70%)';

/** The full-screen login gate: the "Qui regarde ?" picker / sign-in / register
 * flow. Rendered as page content by the layouts that require a session (the
 * `_app` shell and the admin console) in place of their content while signed
 * out so the URL is preserved and revealed once a session exists and by the
 * dedicated `/login` route. */
export function LoginGate() {
  const { ready } = useAuth();
  return (
    <div
      className="flex min-h-screen w-full flex-col items-center justify-center overflow-y-auto px-6 py-12"
      style={{ background: RADIAL }}
    >
      <Brand />
      {ready ? <GateBody /> : <Spinner />}
    </div>
  );
}

export function Brand() {
  return (
    <div className="mb-12">
      <Logo size={24} />
    </div>
  );
}

export function Spinner() {
  // The brand loading state: the chromatic wheel at its 2.6s spinner pace.
  return <Logo markOnly size={40} spin="loading" />;
}

/** Full-screen loader shown by authenticated layouts while the session hydrates
 * or a redirect to /login is in flight (see {@link useRequireAuth}). */
export function GateLoading() {
  return (
    <div
      className="flex min-h-screen w-full items-center justify-center"
      style={{ background: RADIAL }}
    >
      <Spinner />
    </div>
  );
}

export function GateBody() {
  const t = useT();
  const { client, accounts, login, loginPasskey, register, activate, forget } = useAuth();
  const [profiles, setProfiles] = useState<PublicUser[]>([]);
  const [mode, setMode] = useState<Mode>({ kind: 'pick' });
  // Whether the profile picker is available (the `publicUserList` setting). When
  // off there is no roster to return to, so sign-in becomes the root screen.
  const [canPick, setCanPick] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Read the public login-gate config, then decide what to show:
  //  - no accounts yet        → first-run owner registration
  //  - roster hidden          → picker of *remembered local accounts* if any,
  //                             else plain email/password sign-in
  //  - roster public          → load the profiles for the picker
  // The picker always renders remembered accounts too (see below), so switching
  // profiles works TV-style even with a private roster.
  useEffect(() => {
    let cancelled = false;
    client
      .authConfig()
      .then((cfg) => {
        if (cancelled) return;
        setCanPick(cfg.publicUserList);
        if (!cfg.hasAccounts) {
          setMode({ kind: 'register' });
          return;
        }
        if (!cfg.publicUserList) {
          setMode(accounts.length > 0 ? { kind: 'pick' } : { kind: 'login', user: null });
          return;
        }
        client
          .users()
          .then((u) => {
            if (!cancelled) setProfiles(u);
          })
          .catch(() => undefined);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [client, accounts]);

  function fail(e: unknown, fallback: string) {
    setError(
      e instanceof Error && /401|invalid|identifiants/i.test(e.message)
        ? t('auth.invalidCredentials')
        : fallback,
    );
  }

  async function doLogin(identifier: string, password: string) {
    setBusy(true);
    setError(null);
    try {
      await login(identifier, password);
    } catch (e) {
      // 429 → brute-force lockout: surface the server's localized cooldown text.
      if (e instanceof KromaApiError && e.status === 429) {
        setError(apiErrorText(e, t('auth.loginLocked')));
      } else {
        fail(e, t('auth.loginFailed'));
      }
    } finally {
      setBusy(false);
    }
  }

  async function doPasskeyLogin() {
    setBusy(true);
    setError(null);
    try {
      await loginPasskey();
    } catch (e) {
      // A user dismissing the browser prompt (DOMException) isn't a real error.
      if (e instanceof DOMException && (e.name === 'NotAllowedError' || e.name === 'AbortError')) {
        return;
      }
      console.error('passkey sign-in failed', e);
      fail(e, t('auth.passkeyLoginFailed'));
    } finally {
      setBusy(false);
    }
  }

  async function doRegister(
    email: string,
    username: string,
    password: string,
    avatar: File | null,
  ) {
    setBusy(true);
    setError(null);
    try {
      await register(email, username, password, avatar);
    } catch (e) {
      setError(
        e instanceof Error && /409|déjà|exist/i.test(e.message)
          ? t('auth.emailTaken')
          : t('auth.registerFailed'),
      );
    } finally {
      setBusy(false);
    }
  }

  if (mode.kind === 'login') {
    return (
      <LoginForm
        profile={mode.user}
        busy={busy}
        error={error}
        notice={mode.expired ? t('auth.sessionExpiredHint') : null}
        canGoBack={canPick}
        canUsePasskey={passkeysSupported()}
        onBack={() => {
          setError(null);
          setMode({ kind: 'pick' });
        }}
        onSubmit={doLogin}
        onPasskey={doPasskeyLogin}
      />
    );
  }

  if (mode.kind === 'register') {
    return (
      <RegisterForm
        busy={busy}
        error={error}
        canGoBack={profiles.length > 0}
        onBack={() => {
          setError(null);
          setMode({ kind: 'pick' });
        }}
        onSubmit={doRegister}
      />
    );
  }

  if (mode.kind === 'pin') {
    const account = mode.account;
    return (
      <PinEntry
        account={account}
        onBack={() => {
          setError(null);
          setMode({ kind: 'pick' });
        }}
        onSubmit={(pin) => activate(account, pin)}
        onExpired={() => {
          setError(null);
          setMode({ kind: 'login', user: account.user, expired: true });
        }}
      />
    );
  }

  // --- picker ---
  // Remembered local accounts first (one-tap switch, no password), then any
  // public-roster profiles not already remembered (a tap asks for their
  // password). With a private roster `profiles` is empty, so only the remembered
  // accounts show enough to switch profiles TV-style without exposing everyone.
  const tiles: {
    id: string;
    username: string;
    avatarUrl: string | null;
    remembered: StoredSession | null;
    /** Show a lock when entering needs a credential: a PIN-protected remembered
     * profile, or any not-yet-remembered one (which needs its password). */
    locked: boolean;
  }[] = [
    ...accounts.map((a) => ({
      id: a.user.id,
      username: a.user.username,
      avatarUrl: a.user.avatarUrl ?? null,
      remembered: a,
      locked: a.user.hasPin,
    })),
    ...profiles
      .filter((p) => !accounts.some((a) => a.user.id === p.id))
      .map((p) => ({
        id: p.id,
        username: p.username,
        avatarUrl: p.avatarUrl ?? null,
        remembered: null,
        locked: true,
      })),
  ];
  return (
    <div className="flex w-full max-w-4xl flex-col items-center">
      <h1 className="text-center font-display text-[clamp(44px,8vw,76px)] font-bold tracking-[-.02em]">
        {t('auth.whoWatching')}
      </h1>
      <p className="mt-3 mb-12 max-w-xl text-center text-[15px] text-muted">
        {t('auth.whoWatchingHint')}
      </p>

      <div className="flex w-full max-w-[1100px] flex-wrap content-start items-start justify-center gap-x-7 gap-y-9 px-6 py-4">
        {tiles.map((p) => (
          <div key={p.id} className="flex w-[150px] flex-col items-center gap-3">
            <button
              type="button"
              onClick={async () => {
                setError(null);
                const acc = p.remembered;
                if (!acc) {
                  setMode({
                    kind: 'login',
                    user: {
                      id: UserId.of(p.id),
                      username: p.username,
                      avatarUrl: p.avatarUrl,
                      hasPin: false,
                    },
                  });
                  return;
                }
                // Probe with a no-PIN exchange to let the SERVER decide what's
                // needed rather than trusting the cached `hasPin`. A dead token
                // can't be rescued by a PIN, so this lets us skip the PIN screen
                // and go straight to re-login instead of trapping the user there.
                const r = await activate(acc);
                if (r.ok) return; // token still live (+ pin-verified) → signed in
                if (r.needsPin) {
                  // Live token, PIN required (or wrong-cached state) collect it.
                  setMode({ kind: 'pin', account: acc });
                  return;
                }
                // The remembered session is dead (expired/revoked). Send them to
                // this profile's sign-in form (pre-filled) with a calm "session
                // expired" note no dead-end PIN prompt or error on the picker.
                setError(null);
                setMode({ kind: 'login', user: acc.user, expired: true });
              }}
              className="group flex flex-col items-center gap-3.5 focus:outline-none"
            >
              {/* Plain rounded avatar + amber ring on hover/focus (the TV app's
                  focused-tile look), with the amber PIN lock badge in the corner. */}
              <div className="relative w-fit transition-transform duration-200 group-hover:scale-[1.06] group-focus-visible:scale-[1.06]">
                {/* Shadow/ring live on the avatar element itself so they trace its
                    exact rounded box (a wrapper div would cast a boxy shadow). */}
                <UserAvatar
                  name={p.username}
                  avatarUrl={p.avatarUrl}
                  seed={p.id}
                  size={146}
                  radius={24}
                  className="shadow-[0_10px_25px_-8px_rgba(0,0,0,0.6)] transition-shadow duration-200 group-hover:shadow-[0_0_0_4px_var(--kroma-accent),0_10px_25px_-8px_rgba(0,0,0,0.6)] group-focus-visible:shadow-[0_0_0_4px_var(--kroma-accent),0_10px_25px_-8px_rgba(0,0,0,0.6)]"
                />
                {p.locked ? (
                  <span
                    className="absolute right-2 bottom-2 flex h-[29px] w-[29px] items-center justify-center rounded-full bg-[rgba(10,10,12,0.8)] text-accent"
                    title={t('auth.passwordRequired')}
                  >
                    <IconLock size={16} stroke={2} />
                  </span>
                ) : null}
              </div>
              <span className="text-[18px] font-medium text-text/82">{p.username}</span>
            </button>
            {p.remembered ? (
              <button
                type="button"
                onClick={() => forget(p.id)}
                className="text-[12px] font-medium text-dim transition-colors hover:text-text"
              >
                {t('auth.logout')}
              </button>
            ) : null}
          </div>
        ))}

        <div className="flex w-[150px] flex-col items-center gap-3">
          <button
            type="button"
            onClick={() => {
              setError(null);
              setMode({ kind: 'login', user: null });
            }}
            className="group flex flex-col items-center gap-3.5 focus:outline-none"
          >
            <div className="flex h-[146px] w-[146px] items-center justify-center rounded-[24px] border-2 border-dashed border-white/18 text-white/35 transition-transform duration-200 group-hover:scale-[1.06] group-hover:border-accent group-hover:text-accent group-focus-visible:scale-[1.06] group-focus-visible:border-accent group-focus-visible:text-accent">
              <IconPlus size={46} stroke={1.6} />
            </div>
            <span className="text-[18px] font-medium text-text/50">{t('auth.addProfile')}</span>
          </button>
        </div>
      </div>

      {error ? <p className="mt-8 text-[13px] font-medium text-danger">{error}</p> : null}
    </div>
  );
}

/** 4-digit PIN entry shown when switching into a PIN-locked remembered profile.
 * Auto-submits on the fourth digit and mirrors the TV app's feedback: a
 * "verifying" spinner, a shake + message on a wrong PIN, and a live cooldown
 * countdown when the server rate-limits (429). */
function PinEntry({
  account,
  onBack,
  onSubmit,
  onExpired,
}: Readonly<{
  account: StoredSession;
  onBack: () => void;
  onSubmit: (pin: string) => Promise<ActivateResult>;
  /** Called when the exchange fails because the token is dead (not a wrong PIN),
   * so the parent can route to a full re-login instead of looping here. */
  onExpired: () => void;
}>) {
  const t = useT();
  const [pin, setPin] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [shake, setShake] = useState(0);
  const [cooldown, setCooldown] = useState(0);

  // Lockout countdown (429).
  useEffect(() => {
    if (cooldown <= 0) return;
    const id = setInterval(() => setCooldown((c) => Math.max(0, c - 1)), 1000);
    return () => clearInterval(id);
  }, [cooldown]);

  const locked = busy || cooldown > 0;

  const submit = async (value: string) => {
    if (locked) return;
    setBusy(true);
    setError(null);
    const r = await onSubmit(value);
    setBusy(false);
    if (r.ok) return; // the gate unmounts on success
    // The token died (expired/revoked) rather than a wrong PIN a PIN can't fix
    // it, so leave the PIN screen for a full re-login.
    if (!r.needsPin) {
      onExpired();
      return;
    }
    setPin('');
    if (r.retryAfter) setCooldown(r.retryAfter);
    setError(r.error || t('auth.pinIncorrect'));
    setShake((s) => s + 1);
  };

  // Status line content: spinner while verifying, else the error / cooldown.
  let status: ReactNode = null;
  if (busy) {
    status = (
      <>
        <span className="h-4 w-4 animate-spin rounded-full border-2 border-white/20 border-t-accent" />
        <span className="text-[13px] font-medium text-muted">{t('pin.verifying')}</span>
      </>
    );
  } else if (error) {
    status = (
      <span className="text-[13px] font-medium text-danger">
        {cooldown > 0 ? t('pin.lockedRetry', { seconds: cooldown }) : error}
      </span>
    );
  }

  return (
    <div className="flex w-full max-w-90 flex-col items-center gap-6">
      <UserAvatar
        name={account.user.username}
        avatarUrl={account.user.avatarUrl}
        seed={account.user.id}
        size={96}
      />
      <h1 className="font-display text-[24px] font-semibold">{account.user.username}</h1>
      <p className="text-[14px] text-muted">{t('pin.verifySubtitle')}</p>

      {/* key={shake} remounts the row so the shake animation replays each error. */}
      <div key={shake} className={shake ? 'animate-[otp-shake_0.4s_ease]' : ''}>
        <Otp
          value={pin}
          onChange={(v) => {
            setError(null);
            setPin(v);
          }}
          onComplete={(value) => void submit(value)}
          mask
          autoFocus
          disabled={locked}
          ariaLabel={t('account.currentPin')}
        />
      </div>

      {/* Status line: spinner while verifying, else the error / cooldown. */}
      <div className="flex h-5 items-center gap-2">{status}</div>

      <button
        type="button"
        onClick={onBack}
        className="text-[14px] font-medium text-muted hover:text-text"
      >
        ← {t('common.back')}
      </button>
    </div>
  );
}
