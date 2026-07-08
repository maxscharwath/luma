// Multi-server per-user session for the TV. Unlike the web (single origin), the
// TV remembers profiles from several LUMA servers at once, so this owns:
//   • the active session (token + user + which server),
//   • every remembered account across all servers,
//   • each saved server's public profile list (for the picker), with liveness,
//   • a per-session "unlocked" set so a PIN-protected profile is gated once on
//     switch-in, not on every render.
// It reads the active client + server list from <ConnectionProvider> and feeds
// the chosen server back via `setActiveServer` when a profile is activated.

import {
  type AuthResult,
  clearSession,
  forgetAccount as forgetAccountStore,
  type LumaClient,
  loadAccounts,
  loadSession,
  normalizeServerUrl as norm,
  type StoredSession,
  saveSession,
  type User,
} from '@luma/core';
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';

const keyOf = (a: Pick<StoredSession, 'serverUrl' | 'user'>) => `${norm(a.serverUrl)}|${a.user.id}`;

interface Auth {
  /** The active session, or null when signed out. */
  session: StoredSession | null;
  /** The signed-in user (null when signed out). */
  user: User | null;
  /** Every remembered account, across all servers (the picker's only source). */
  accounts: StoredSession[];
  /** Pair a freshly Quick-Connected account on `serverUrl` and sign in. */
  login: (res: AuthResult, serverUrl: string) => void;
  /** Switch to a remembered account instantly (no password). */
  activate: (account: StoredSession) => void;
  /** Back to the picker WITHOUT signing out (re-arms every PIN lock). */
  switchProfile: () => void;
  /** Forget a remembered (serverUrl, user) profile on this device. */
  forget: (userId: string, serverUrl: string) => void;
  /** Fully sign out the active account (invalidate + forget on this device). */
  logout: () => Promise<void>;
  /** Merge a patch into the active user, persisting it. */
  updateUser: (patch: Partial<User>) => void;
  /** Has this account already cleared its PIN gate this session? */
  isUnlocked: (account: Pick<StoredSession, 'serverUrl' | 'user'>) => boolean;
}

const AuthCtx = createContext<Auth | null>(null);

export function AuthProvider({
  client,
  activeServerUrl,
  setActiveServer,
  onSignedInChange,
  children,
}: Readonly<{
  client: LumaClient | null;
  activeServerUrl: string | null;
  setActiveServer: (url: string) => void;
  /** Reports whether a session is active, so the host can gate the catalogue +
   * event stream (the signed-out picker makes no requests). */
  onSignedInChange: (signedIn: boolean) => void;
  children: ReactNode;
}>) {
  const [session, setSession] = useState<StoredSession | null>(() => loadSession());
  const [accounts, setAccounts] = useState<StoredSession[]>(() => loadAccounts());
  // Profiles that have already cleared their PIN this session (ref gating reads
  // it at switch-in time, it doesn't drive rendering).
  const unlocked = useRef<Set<string>>(new Set(session ? [keyOf(session)] : []));
  // The access token we've already exchanged for a bearer. The success path below
  // mints a new `session` object (re-running the effect); this ref stops it from
  // re-exchanging in an infinite loop.
  const exchangedRef = useRef<string | null>(null);
  // Coalesce concurrent silent refreshes (a poster grid full of 401s → one
  // exchange, not N — which would trip the brute-force guard and bounce to the
  // picker).
  const refreshingRef = useRef<Promise<string | undefined> | null>(null);

  // Exchange the active account's access token for a short-lived session bearer
  // and keep it on the client but only when the session belongs to the server
  // the client points at (a token for server A must never ride a request to
  // server B). Also installs the 401 silent-refresh handler. The exchange
  // returns the fresh user, so a change made elsewhere (language, hasPin) reaches
  // the TV without a separate `me()` call.
  useEffect(() => {
    if (!client) return;
    const match = session && norm(session.serverUrl) === norm(activeServerUrl);
    if (!match || !session) {
      client.setAuthToken(undefined);
      client.setRefreshHandler(undefined);
      exchangedRef.current = null;
      return;
    }
    client.setRefreshHandler(() => {
      if (refreshingRef.current) return refreshingRef.current;
      const s = loadSession();
      if (!s) return Promise.resolve(undefined);
      const p = client
        .exchangeToken(s.accessToken)
        .then((r) => r.token as string | undefined)
        .catch(() => undefined)
        .finally(() => {
          refreshingRef.current = null;
        });
      refreshingRef.current = p;
      return p;
    });

    // Exchange once per access token (the setSession below would otherwise loop).
    if (exchangedRef.current === session.accessToken) {
      return () => client.setRefreshHandler(undefined);
    }
    exchangedRef.current = session.accessToken;

    let cancelled = false;
    client
      .exchangeToken(session.accessToken)
      .then((res) => {
        if (cancelled) return;
        client.setAuthToken(res.token);
        setSession((cur) =>
          cur && cur.user.id === res.user.id ? { ...cur, user: res.user } : cur,
        );
        saveSession({ ...session, user: res.user });
      })
      .catch(() => {
        if (cancelled) return;
        // Can't resume (revoked/expired token, or PIN required after a reset):
        // drop to the picker instead of a zombie 'signed-in' state with no bearer.
        client.setAuthToken(undefined);
        exchangedRef.current = null;
        unlocked.current.clear();
        clearSession();
        setSession(null);
      });
    return () => {
      cancelled = true;
      client.setRefreshHandler(undefined);
    };
  }, [client, session, activeServerUrl]);

  // Surface sign-in state to the host (gates catalogue + events).
  useEffect(() => {
    onSignedInChange(Boolean(session));
  }, [session, onSignedInChange]);

  // Persist a (normalized) session, clear its PIN gate for this session, point the
  // client at its server, and sign in. Shared by login (fresh pair) and activate
  // (switch to a remembered profile).
  const enter = useCallback(
    (s: StoredSession) => {
      saveSession(s);
      unlocked.current.add(keyOf(s));
      setActiveServer(norm(s.serverUrl));
      setSession(s);
      setAccounts(loadAccounts());
    },
    [setActiveServer],
  );

  const login = useCallback(
    (res: AuthResult, serverUrl: string) => {
      // Set the just-minted bearer immediately, and mark this access token as
      // already-exchanged so the effect doesn't redundantly re-exchange it.
      client?.setAuthToken(res.token);
      exchangedRef.current = res.accessToken;
      enter({ serverUrl: norm(serverUrl), accessToken: res.accessToken, user: res.user });
    },
    [enter, client],
  );

  const activate = useCallback(
    (account: StoredSession) => {
      enter({ ...account, serverUrl: norm(account.serverUrl) });
    },
    [enter],
  );

  const switchProfile = useCallback(() => {
    client?.setAuthToken(undefined);
    clearSession();
    unlocked.current.clear(); // re-arm every PIN lock
    setSession(null);
  }, [client]);

  const forget = useCallback(
    (userId: string, serverUrl: string) => {
      forgetAccountStore(userId, serverUrl);
      setAccounts(loadAccounts());
      setSession((s) => {
        if (s?.user.id === userId && norm(s?.serverUrl) === norm(serverUrl)) {
          client?.setAuthToken(undefined);
          return null;
        }
        return s;
      });
    },
    [client],
  );

  const logout = useCallback(async () => {
    const active = session;
    try {
      await client?.logout(active?.accessToken);
    } catch {
      /* best-effort server-side revocation */
    }
    client?.setAuthToken(undefined);
    if (active?.serverUrl) forgetAccountStore(active.user.id, active.serverUrl);
    else clearSession();
    unlocked.current.clear();
    setAccounts(loadAccounts());
    setSession(null);
  }, [client, session]);

  const updateUser = useCallback(
    (patch: Partial<User>) => {
      if (!session) return;
      const next: StoredSession = { ...session, user: { ...session.user, ...patch } };
      // `saveSession` rewrites BOTH the active session and this profile's entry in
      // the remembered-accounts store, so re-reading it keeps the picker's lock
      // state (hasPin) in sync. Without refreshing `accounts`, disabling a PIN
      // left the profile still showing its lock and re-prompting on switch-in.
      saveSession(next);
      setSession(next);
      setAccounts(loadAccounts());
    },
    [session],
  );

  const isUnlocked = useCallback(
    (account: Pick<StoredSession, 'serverUrl' | 'user'>) => unlocked.current.has(keyOf(account)),
    [],
  );

  const value = useMemo<Auth>(
    () => ({
      session,
      user: session?.user ?? null,
      accounts,
      login,
      activate,
      switchProfile,
      forget,
      logout,
      updateUser,
      isUnlocked,
    }),
    [session, accounts, login, activate, switchProfile, forget, logout, updateUser, isUnlocked],
  );
  return <AuthCtx.Provider value={value}>{children}</AuthCtx.Provider>;
}

export function useAuth(): Auth {
  const ctx = useContext(AuthCtx);
  if (!ctx) throw new Error('useAuth() must be used inside <AuthProvider>');
  return ctx;
}
