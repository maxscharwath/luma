import {
  type AuthResult,
  LOCALES,
  type LumaClient,
  type MessageKey,
  type QuickConnectInit,
} from '@luma/core';
import { Button, Logo, useLocale, useSetLocale, useT } from '@luma/ui';
import { IconApps, IconLock, IconPlus } from '@tabler/icons-react';
import { type ReactNode, useEffect, useState } from 'react';
import { useAuth } from '#tv/auth';
import { useClient, useNav, useParams } from '#tv/router';
import { useFocusNav } from '#tv/useFocusNav';

// Same vivid gradient palette as the web profiles (LUMA.dc.html).
const GRADS = [
  'linear-gradient(135deg,#F4B642,#E8743B)',
  'linear-gradient(135deg,#3BC9DB,#3B82F6)',
  'linear-gradient(135deg,#A855F7,#6366F1)',
  'linear-gradient(135deg,#F472B6,#EC4899)',
  'linear-gradient(135deg,#34D399,#10B981)',
];

function gradFor(seed: string): string {
  let h = 0;
  for (let i = 0; i < seed.length; i += 1) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return GRADS[h % GRADS.length] as string;
}

function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '?';
  if (parts.length === 1) return parts[0]!.slice(0, 2).toUpperCase();
  return (parts[0]![0]! + parts[parts.length - 1]![0]!).toUpperCase();
}

/** Map a sign-in/registration failure to a catalog key: invalid credentials get
 * their own message; anything else falls back to the screen-specific key. */
function authError(e: unknown, fallback: MessageKey): MessageKey {
  return e instanceof Error && /401|identifiants|invalid/i.test(e.message)
    ? 'auth.invalidCredentials'
    : fallback;
}

/** Rounded-square profile avatar — the uploaded photo when there is one, else a
 * deterministic gradient with the user's initials (same look as the web picker). */
function ProfileAvatar({
  name,
  seed,
  size,
  src,
}: Readonly<{
  name: string;
  seed: string;
  size: number;
  src?: string | null;
}>) {
  const [failed, setFailed] = useState(false);
  const showImg = Boolean(src) && !failed;
  return (
    <div
      className="flex items-center justify-center overflow-hidden font-display font-bold text-white/90"
      style={{
        width: size,
        height: size,
        borderRadius: Math.round(size * 0.13),
        background: gradFor(seed),
        fontSize: Math.round(size * 0.38),
        boxShadow: '0 16px 40px rgba(0,0,0,.5)',
      }}
    >
      {showImg ? (
        <img
          src={src ?? undefined}
          alt=""
          onError={() => setFailed(true)}
          style={{ width: '100%', height: '100%', objectFit: 'cover' }}
        />
      ) : (
        initials(name)
      )}
    </div>
  );
}

const INPUT =
  'w-full rounded-md border border-border-strong bg-surface-2 px-5 py-4 text-center font-sans text-[18px] text-text';

/** Shared 10-foot auth backdrop + centred brand mark. Each auth screen
 * (picker / login / register / quick-connect) is its own router route and wraps
 * its body in this. */
function AuthShell({ children }: Readonly<{ children: ReactNode }>) {
  return (
    <div
      className="fixed inset-0 flex flex-col items-center justify-center px-16 text-center"
      style={{ background: 'radial-gradient(120% 90% at 50% 0%, #15131C, #0A0A0C 70%)' }}
    >
      <div className="mb-12">
        <Logo size={40} />
      </div>
      {children}
    </div>
  );
}

/**
 * 10-foot language switcher: one focusable chip per locale (FR / EN). Each chip
 * carries `data-focus` and activates on OK via its native `onClick` — exactly
 * like the profile avatars and the Quick Connect button — so the remote's
 * spatial navigation (useFocusNav) can reach and toggle it. Selecting a locale
 * calls useSetLocale(), which persists it and syncs it to the account.
 */
function LanguageSwitcher() {
  const t = useT();
  const locale = useLocale();
  const setLocale = useSetLocale();
  return (
    <div
      className="mt-8 inline-flex items-center gap-1 rounded-full border border-[rgba(255,255,255,0.16)] bg-[rgba(255,255,255,0.06)] p-1"
      aria-label={t('common.language')}
    >
      {LOCALES.map((l) => {
        const active = l.code === locale;
        return (
          <button
            key={l.code}
            data-focus=""
            type="button"
            aria-current={active}
            onClick={() => setLocale(l.code)}
            className={`cursor-pointer rounded-full border-none px-5 py-2 font-sans text-[15px] font-semibold outline-none transition-transform focus:scale-[1.06] ${
              active
                ? 'bg-accent text-accent-ink'
                : 'bg-transparent text-[rgba(244,243,240,0.7)] focus:text-accent'
            }`}
          >
            {t(l.labelKey)}
          </button>
        );
      })}
    </div>
  );
}

/**
 * Profile picker — the signed-out home of the auth flow (route `profiles`).
 * Selecting a remembered account signs in instantly (no password); any other
 * profile / "Ajouter" / "Connexion rapide" pushes the matching route, so Back
 * pops cleanly via the shared TV router instead of a local view state machine.
 */
export function TvProfiles() {
  const client = useClient();
  const nav = useNav();
  const t = useT();
  const { profiles, accounts, activate } = useAuth();
  useFocusNav({ onBack: nav.back });

  return (
    <AuthShell>
      <h1 className="m-0 mb-12 font-display text-[44px] font-semibold">{t('auth.whoWatching')}</h1>
      <div className="flex flex-wrap items-start justify-center gap-10">
        {profiles.map((p) => {
          const remembered = accounts.find((a) => a.user.id === p.id);
          // Only the avatar is focusable, so the single amber ring hugs it (the
          // name sits below, outside the focus box) — no double border.
          return (
            <div key={p.id} className="flex w-40 flex-col items-center gap-4">
              <div
                data-focus=""
                tabIndex={0}
                role="button"
                onClick={() => (remembered ? activate(remembered) : nav.go('login', { user: p }))}
                className="relative cursor-pointer rounded-[20px] outline-none transition-transform focus:scale-[1.07]"
              >
                <ProfileAvatar
                  name={p.username}
                  seed={p.id}
                  size={150}
                  src={client.resolveArt(p.avatarUrl)}
                />
                {remembered ? null : (
                  <span className="absolute bottom-2 right-2 flex h-8 w-8 items-center justify-center rounded-full bg-[rgba(10,10,12,0.78)] text-[rgba(244,243,240,0.85)]">
                    <IconLock size={16} stroke={2} />
                  </span>
                )}
              </div>
              <span className="font-sans text-[20px] font-medium text-[rgba(244,243,240,0.82)]">
                {p.username}
              </span>
            </div>
          );
        })}
        <div className="flex w-40 flex-col items-center gap-4">
          <div
            data-focus=""
            tabIndex={0}
            role="button"
            onClick={() => nav.go('register')}
            className="flex h-37.5 w-37.5 cursor-pointer items-center justify-center rounded-[20px] border-2 border-dashed border-[rgba(255,255,255,0.2)] text-[rgba(255,255,255,0.4)] outline-none transition-transform focus:scale-[1.07]"
          >
            <IconPlus size={44} stroke={1.6} />
          </div>
          <span className="font-sans text-[20px] font-medium text-[rgba(244,243,240,0.5)]">
            {t('profiles.add')}
          </span>
        </div>
      </div>
      <button
        data-focus=""
        type="button"
        onClick={() => nav.go('quick')}
        className="mt-12 inline-flex cursor-pointer items-center gap-2.5 rounded-full border border-[rgba(255,255,255,0.2)] bg-transparent px-6 py-3 font-sans text-[16px] font-semibold text-[rgba(244,243,240,0.78)] outline-none transition-transform focus:scale-[1.05] focus:border-accent focus:text-accent"
      >
        <IconApps size={20} stroke={1.7} />
        {t('connect.quickConnect')}
      </button>
      <LanguageSwitcher />
      <p className="mt-5 font-sans text-[15px] font-semibold text-dim">{t('profiles.chooseHint')}</p>
    </AuthShell>
  );
}

/** Password entry for a chosen profile (route `login`). */
export function TvLogin() {
  const client = useClient();
  const nav = useNav();
  const t = useT();
  const { login } = useAuth();
  const { user } = useParams('login');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<MessageKey | ''>('');
  useFocusNav({ onBack: nav.back });

  const submit = async () => {
    if (!password) return;
    setBusy(true);
    setError('');
    try {
      login(await client.login(user.username, password));
    } catch (e) {
      setError(authError(e, 'auth.loginFailed'));
    } finally {
      setBusy(false);
    }
  };

  return (
    <AuthShell>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
        className="flex w-full max-w-115 flex-col items-center gap-5"
      >
        <ProfileAvatar
          name={user.username}
          seed={user.id}
          size={104}
          src={client.resolveArt(user.avatarUrl)}
        />
        <h1 className="m-0 font-display text-[30px] font-bold">{user.username}</h1>
        <input
          data-focus=""
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          placeholder={t('auth.password')}
          className={INPUT}
        />
        {error ? <p className="m-0 font-sans text-[15px] text-danger">{t(error)}</p> : null}
        <Button type="submit" data-focus="">
          {busy ? t('auth.loggingIn') : t('auth.login')}
        </Button>
      </form>
    </AuthShell>
  );
}

/** New-account creation (route `register`). */
export function TvRegister() {
  const client = useClient();
  const nav = useNav();
  const t = useT();
  const { login } = useAuth();
  const [email, setEmail] = useState('');
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<MessageKey | ''>('');
  useFocusNav({ onBack: nav.back });

  const valid = email.includes('@') && username.trim().length > 0 && password.length >= 4;
  const submit = async () => {
    if (!valid) return;
    setBusy(true);
    setError('');
    try {
      login(await client.register(email.trim(), username.trim(), password));
    } catch (e) {
      setError(authError(e, 'auth.registerFailed'));
    } finally {
      setBusy(false);
    }
  };

  return (
    <AuthShell>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
        className="flex w-full max-w-115 flex-col items-center gap-4"
      >
        <h1 className="m-0 mb-2 font-display text-[30px] font-bold">{t('auth.newAccount')}</h1>
        <input
          data-focus=""
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          placeholder={t('auth.email')}
          className={INPUT}
        />
        <input
          data-focus=""
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          placeholder={t('auth.username')}
          className={INPUT}
        />
        <input
          data-focus=""
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          placeholder={t('auth.passwordHint')}
          className={INPUT}
        />
        {error ? <p className="m-0 font-sans text-[15px] text-danger">{t(error)}</p> : null}
        <Button type="submit" data-focus="">
          {busy ? t('auth.creating') : t('auth.createAccount')}
        </Button>
      </form>
    </AuthShell>
  );
}

/**
 * Quick Connect (route `quick`): the TV shows a short code (and a QR when the
 * server knows the web URL); an already-signed-in user approves it from the web
 * app, and the TV logs in on its next poll — no password typed on the remote.
 */
export function TvQuickConnect() {
  const client = useClient();
  const nav = useNav();
  const t = useT();
  const { login } = useAuth();
  const [info, setInfo] = useState<QuickConnectInit | null>(null);
  const [qr, setQr] = useState<string | null>(null);
  const [error, setError] = useState<MessageKey | ''>('');
  useFocusNav({ onBack: nav.back });

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let secret = '';

    const onAuthenticated = (res: AuthResult) => login(res);

    const poll = async () => {
      if (cancelled) return;
      try {
        const res = await client.quickConnectPoll(secret);
        if (cancelled) return;
        if (res.status === 'authorized') {
          onAuthenticated({ token: res.token, user: res.user });
          return;
        }
        if (res.status === 'expired') {
          void begin();
          return;
        }
      } catch {
        /* transient — keep polling */
      }
      timer = setTimeout(poll, 2500);
    };

    const begin = async () => {
      try {
        const init = await client.quickConnectInitiate();
        if (cancelled) return;
        secret = init.secret;
        setInfo(init);
        setQr(null);
        // Build the approval URL (server's authorizeUrl if LUMA_WEB_URL is set,
        // else derive the LUMA web app from the API origin) and render a QR.
        const url = connectUrl(client, init.code, init.authorizeUrl);
        if (url) {
          void import('qrcode-generator')
            .then((mod) => {
              if (cancelled) return;
              const make = mod.default;
              const qrc = make(0, 'M');
              qrc.addData(url);
              qrc.make();
              setQr(qrc.createSvgTag({ cellSize: 6, margin: 1, scalable: true }));
            })
            .catch(() => undefined);
        }
        timer = setTimeout(poll, 2500);
      } catch {
        if (!cancelled) setError('connect.quickConnectUnavailable');
      }
    };

    void begin();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [client, login]);

  return (
    <AuthShell>
      <div className="flex w-full max-w-190 flex-col items-center gap-7 text-center">
        <h1 className="m-0 font-display text-[34px] font-bold">{t('connect.quickConnect')}</h1>

        {error ? <p className="font-sans text-[16px] text-danger">{t(error)}</p> : null}
        {!error && info ? (
          <div className="flex flex-col items-center gap-6">
            {qr ? (
              <div className="flex flex-col items-center gap-3">
                {/* eslint-disable-next-line react/no-danger */}
                <div
                  className="h-55 w-55 rounded-2xl bg-white p-3 [&>svg]:h-full [&>svg]:w-full"
                  dangerouslySetInnerHTML={{ __html: qr }}
                />
                <span className="font-sans text-[14px] font-semibold text-dim">
                  {t('connect.scanQr')}
                </span>
              </div>
            ) : null}

            <p className="m-0 font-sans text-[16px] text-muted">
              {t('connect.orInAppPrefix')}
              <b className="text-text">{t('nav.connectDevice')}</b>
              {t('connect.orInAppSuffix')}
            </p>
            <div className="font-display text-[96px] font-bold leading-none tracking-[0.2em] text-accent tabular-nums">
              {info.code}
            </div>
          </div>
        ) : null}
        {!error && !info ? (
          <div className="h-10 w-10 rounded-full border-[3px] border-[rgba(255,255,255,0.2)] border-t-accent animate-[tvp-spin_0.9s_linear_infinite]" />
        ) : null}

        <p className="font-sans text-[14px] font-semibold text-dim">
          {t('connect.backToProfiles')}
        </p>
      </div>
    </AuthShell>
  );
}

/** Resolve the web `/connect?code=` URL for the QR: the server's `authorizeUrl`
 * (set when `LUMA_WEB_URL` is configured) wins; otherwise derive it from the API
 * origin — the LUMA web app runs alongside the API on port 3000 by convention. */
function connectUrl(client: LumaClient, code: string, serverUrl?: string | null): string {
  if (serverUrl) return serverUrl;
  try {
    const u = new URL(client.baseUrl);
    u.port = '3000';
    u.pathname = '/connect';
    u.search = `?code=${code}`;
    return u.toString();
  } catch {
    return '';
  }
}
