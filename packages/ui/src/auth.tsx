// The shared per-user session state machine behind each client's <AuthProvider>.
// It holds the active session + the remembered-accounts list, keeps the API
// client's bearer token in sync, refreshes the account from the server on mount,
// and exposes the sign-in/out/switch actions. Each client wraps this in its own
// thin context: the web app adds email/password login + register; the TV app adds
// the "Qui regarde ?" profile picker and feeds a pre-fetched AuthResult straight
// into `apply`.

import {
  type AuthResult,
  clearSession,
  forgetAccount,
  loadAccounts,
  loadSession,
  type LumaClient,
  saveSession,
  type StoredSession,
  type User,
} from '@luma/core';
import { useCallback, useEffect, useMemo, useState } from 'react';

export interface AuthSession {
  /** The active session, or null when signed out. */
  session: StoredSession | null;
  /** The signed-in user (null when signed out). */
  user: User | null;
  /** Accounts already signed-in on this device — switchable without a password. */
  accounts: StoredSession[];
  /** True once storage hydration has run (client-side). */
  ready: boolean;
  /** Persist a successful auth result and sign in. */
  apply: (res: AuthResult) => void;
  /** Switch to a remembered account instantly (no password re-entry). */
  activate: (s: StoredSession) => void;
  /** Back to the picker WITHOUT signing out (keeps remembered accounts). */
  switchProfile: () => void;
  /** Forget a remembered account on this device (real sign-out for it). */
  forget: (userId: string) => void;
  /** Fully sign out of the current account (invalidate + forget this device). */
  logout: () => Promise<void>;
  /** Merge a patch into the active user, persisting it to the stored session. */
  updateUser: (patch: Partial<User>) => void;
}

export interface AuthSessionOptions {
  /** Read the stored session during the INITIAL render, so the very first authed
   * fetch already carries the token (what the TV needs for "Reprendre"). Leave
   * false on SSR clients (web) so server/client renders stay consistent and the
   * session is applied in an effect instead, gated by `ready`. */
  syncHydrate?: boolean;
}

export function useAuthSession(
  client: LumaClient | null,
  { syncHydrate = false }: AuthSessionOptions = {},
): AuthSession {
  const [session, setSession] = useState<StoredSession | null>(() => {
    if (!syncHydrate) return null;
    const s = loadSession();
    // Apply the token during init (before children render) so the first authed
    // fetch already carries it.
    if (s) client?.setAuthToken(s.token);
    return s;
  });
  const [accounts, setAccounts] = useState<StoredSession[]>(() =>
    syncHydrate ? loadAccounts() : [],
  );
  const [ready, setReady] = useState(syncHydrate);

  // Keep the bearer token in sync across client / session changes.
  useEffect(() => {
    client?.setAuthToken(session?.token);
  }, [client, session]);

  // For effect-hydrated (SSR-safe) clients: hydrate the active session + accounts
  // now. Then, for both, refresh the signed-in account from the server so a
  // preference changed on another device (e.g. language) reaches here. Reads
  // storage directly (not the `session` dep) so it runs once per client.
  // biome-ignore lint/correctness/useExhaustiveDependencies: hydrate once per client, not per session change.
  useEffect(() => {
    if (!syncHydrate) {
      const s = loadSession();
      if (s) client?.setAuthToken(s.token);
      setSession(s);
      setAccounts(loadAccounts());
      setReady(true);
    }
    if (!client) return;
    const s = loadSession();
    if (!s) return;
    let cancelled = false;
    client
      .me()
      .then(({ user }) => {
        if (cancelled) return;
        setSession((cur) => (cur && cur.user.id === user.id ? { ...cur, user } : cur));
        saveSession({ token: s.token, user });
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [client]);

  const apply = useCallback(
    (res: AuthResult) => {
      const s: StoredSession = { token: res.token, user: res.user };
      saveSession(s);
      client?.setAuthToken(res.token);
      setSession(s);
      setAccounts(loadAccounts());
    },
    [client],
  );

  const activate = useCallback(
    (s: StoredSession) => {
      saveSession(s); // re-affirm active + bump recency
      client?.setAuthToken(s.token);
      setSession(s);
      setAccounts(loadAccounts());
    },
    [client],
  );

  const switchProfile = useCallback(() => {
    client?.setAuthToken(undefined);
    clearSession();
    setSession(null);
  }, [client]);

  const forget = useCallback(
    (userId: string) => {
      forgetAccount(userId);
      setAccounts(loadAccounts());
      setSession((s) => {
        if (s?.user.id === userId) {
          client?.setAuthToken(undefined);
          return null;
        }
        return s;
      });
    },
    [client],
  );

  const logout = useCallback(async () => {
    const id = session?.user.id;
    try {
      await client?.logout();
    } catch {
      /* best-effort server-side invalidation */
    }
    client?.setAuthToken(undefined);
    if (id) forgetAccount(id);
    else clearSession();
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

  return useMemo<AuthSession>(
    () => ({
      session,
      user: session?.user ?? null,
      accounts,
      ready,
      apply,
      activate,
      switchProfile,
      forget,
      logout,
      updateUser,
    }),
    [session, accounts, ready, apply, activate, switchProfile, forget, logout, updateUser],
  );
}
