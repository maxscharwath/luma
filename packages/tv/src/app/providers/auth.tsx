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

  // Keep the active client's bearer token in sync with the active session but
  // only when the session belongs to the server the client points at (a token for
  // server A must never ride a request to server B).
  useEffect(() => {
    const match = session && norm(session.serverUrl) === norm(activeServerUrl);
    client?.setAuthToken(match ? session.token : undefined);
  }, [client, session, activeServerUrl]);

  // Surface sign-in state to the host (gates catalogue + events).
  useEffect(() => {
    onSignedInChange(Boolean(session));
  }, [session, onSignedInChange]);

  // Refresh the signed-in account from its server so a change made elsewhere
  // (language, hasPin) reaches the TV. Runs once per client.
  // biome-ignore lint/correctness/useExhaustiveDependencies: refresh once per client.
  useEffect(() => {
    if (!client) return;
    const s = loadSession();
    if (!s) return;
    let cancelled = false;
    client
      .me()
      .then(({ user }) => {
        if (cancelled) return;
        setSession((cur) => (cur && cur.user.id === user.id ? { ...cur, user } : cur));
        saveSession({ ...s, user });
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [client]);

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
      enter({ serverUrl: norm(serverUrl), token: res.token, user: res.user });
    },
    [enter],
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
      await client?.logout();
    } catch {
      /* best-effort server-side invalidation */
    }
    client?.setAuthToken(undefined);
    if (active?.serverUrl) forgetAccountStore(active.user.id, active.serverUrl);
    else clearSession();
    unlocked.current.clear();
    setAccounts(loadAccounts());
    setSession(null);
  }, [client, session]);

  const updateUser = useCallback((patch: Partial<User>) => {
    setSession((s) => {
      if (!s) return s;
      const next: StoredSession = { ...s, user: { ...s.user, ...patch } };
      saveSession(next);
      return next;
    });
  }, []);

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
