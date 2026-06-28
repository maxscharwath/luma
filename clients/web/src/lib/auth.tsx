// Client-side authentication context. The catalogue is rendered publicly via
// SSR loaders; this layers a per-user session on top (login gate, profile,
// playback progress). The bearer token is held here + persisted to localStorage
// (see @luma/core `session`), and attached to a single authed `LumaClient` used
// for all per-user calls (progress, avatar upload).

import {
  type AuthResult,
  clearSession,
  forgetAccount,
  LumaClient,
  loadAccounts,
  loadSession,
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
  useState,
} from 'react';
import { apiBase } from '#web/lib/api';

interface AuthValue {
  /** Logged-in user, or null when signed out. */
  user: User | null;
  /** True once the session has been hydrated from storage (client-side). */
  ready: boolean;
  /** Authed API client (token attached while logged in). */
  client: LumaClient;
  /** Accounts already signed-in on this device — switchable without a password. */
  accounts: StoredSession[];
  login: (email: string, password: string) => Promise<void>;
  register: (
    email: string,
    username: string,
    password: string,
    avatar?: File | null,
    inviteToken?: string,
  ) => Promise<void>;
  /** Switch to a remembered account instantly (no password re-entry). */
  activate: (s: StoredSession) => void;
  /** Return to the "Qui regarde ?" picker WITHOUT signing out (keeps remembered
   * accounts, so switching back stays password-free). */
  switchProfile: () => void;
  /** Forget a remembered account on this device (real sign-out for it). */
  forget: (userId: string) => void;
  /** Fully sign out of the current account (invalidate + forget this device). */
  logout: () => Promise<void>;
  /** Merge a patch into the active user, persisting it to the stored session
   * (e.g. the language preference, so a reload keeps it). No-op when signed out. */
  updateUser: (patch: Partial<User>) => void;
}

const AuthContext = createContext<AuthValue | null>(null);

export function AuthProvider({ children }: Readonly<{ children: ReactNode }>) {
  // One authed client for the app's lifetime; the token is swapped in/out.
  const client = useMemo(() => new LumaClient({ baseUrl: apiBase() }), []);
  const [user, setUser] = useState<User | null>(null);
  const [accounts, setAccounts] = useState<StoredSession[]>([]);
  const [ready, setReady] = useState(false);

  // Hydrate the active session + the remembered-accounts list (client-only).
  useEffect(() => {
    const s = loadSession();
    if (s) {
      client.setAuthToken(s.token);
      setUser(s.user);
      // Pull fresh account state (language, avatar, permissions) so a change made
      // on another device propagates here. Best-effort: keep the stored user if
      // offline or the token is stale.
      client
        .me()
        .then(({ user: fresh }) => {
          setUser(fresh);
          saveSession({ token: s.token, user: fresh });
        })
        .catch(() => {});
    }
    setAccounts(loadAccounts());
    setReady(true);
  }, [client]);

  const apply = useCallback(
    (res: AuthResult) => {
      client.setAuthToken(res.token);
      setUser(res.user);
      saveSession({ token: res.token, user: res.user });
      setAccounts(loadAccounts());
    },
    [client],
  );

  const login = useCallback(
    async (email: string, password: string) => {
      apply(await client.login(email, password));
    },
    [client, apply],
  );

  const register = useCallback(
    async (
      email: string,
      username: string,
      password: string,
      avatar?: File | null,
      inviteToken?: string,
    ) => {
      const res = await client.register(email, username, password, inviteToken);
      apply(res);
      // Optional avatar upload — uses the just-issued token.
      if (avatar) {
        try {
          const { avatarUrl } = await client.uploadAvatar(avatar);
          const updated = { ...res.user, avatarUrl };
          setUser(updated);
          saveSession({ token: res.token, user: updated });
          setAccounts(loadAccounts());
        } catch {
          /* avatar is optional — keep the account without it */
        }
      }
    },
    [client, apply],
  );

  // Switch to a remembered account instantly — no password.
  const activate = useCallback(
    (s: StoredSession) => {
      client.setAuthToken(s.token);
      setUser(s.user);
      saveSession(s); // re-affirm active + bump recency
      setAccounts(loadAccounts());
    },
    [client],
  );

  // Back to the picker without signing out (token + remembered accounts kept).
  const switchProfile = useCallback(() => {
    client.setAuthToken(undefined);
    clearSession();
    setUser(null);
  }, [client]);

  const forget = useCallback(
    (userId: string) => {
      forgetAccount(userId);
      setAccounts(loadAccounts());
      setUser((u) => {
        if (u?.id === userId) {
          client.setAuthToken(undefined);
          return null;
        }
        return u;
      });
    },
    [client],
  );

  const logout = useCallback(async () => {
    const id = user?.id;
    try {
      await client.logout();
    } catch {
      /* best-effort server-side invalidation */
    }
    client.setAuthToken(undefined);
    if (id) forgetAccount(id);
    else clearSession();
    setAccounts(loadAccounts());
    setUser(null);
  }, [client, user]);

  const updateUser = useCallback((patch: Partial<User>) => {
    setUser((u) => {
      if (!u) return u;
      const next = { ...u, ...patch };
      const token = loadSession()?.token;
      if (token) saveSession({ token, user: next });
      return next;
    });
  }, []);

  const value = useMemo<AuthValue>(
    () => ({
      user,
      ready,
      client,
      accounts,
      login,
      register,
      activate,
      switchProfile,
      forget,
      logout,
      updateUser,
    }),
    [
      user,
      ready,
      client,
      accounts,
      login,
      register,
      activate,
      switchProfile,
      forget,
      logout,
      updateUser,
    ],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

/** Access the auth context. Throws if used outside `<AuthProvider>`. */
export function useAuth(): AuthValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error('useAuth must be used within <AuthProvider>');
  return ctx;
}
