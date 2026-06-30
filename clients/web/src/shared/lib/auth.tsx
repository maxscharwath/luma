// Client-side authentication context. The catalogue is rendered publicly via
// SSR loaders; this layers a per-user session on top (login gate, profile,
// playback progress). The session state machine lives in the shared
// `useAuthSession` hook (@luma/ui); this adds the web-specific bits: a single
// authed `LumaClient`, email/password login, and invite-gated registration with
// an optional avatar upload.

import { LumaClient, type StoredSession, type User } from '@luma/core';
import { useAuthSession } from '@luma/ui';
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useMemo,
} from 'react';
import { apiBase } from '#web/shared/lib/api';

interface AuthValue {
  /** Logged-in user, or null when signed out. */
  user: User | null;
  /** True once the session has been hydrated from storage (client-side). */
  ready: boolean;
  /** Authed API client (token attached while logged in). */
  client: LumaClient;
  /** Accounts already signed-in on this device switchable without a password. */
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
  /** Merge a patch into the active user, persisting it to the stored session. */
  updateUser: (patch: Partial<User>) => void;
}

const AuthContext = createContext<AuthValue | null>(null);

export function AuthProvider({ children }: Readonly<{ children: ReactNode }>) {
  // One authed client for the app's lifetime; the token is swapped in/out.
  const client = useMemo(() => new LumaClient({ baseUrl: apiBase() }), []);
  // Web is SSR-rendered, so hydrate the session in an effect (not synchronously).
  const auth = useAuthSession(client);

  const login = useCallback(
    async (email: string, password: string) => {
      auth.apply(await client.login(email, password));
    },
    [client, auth],
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
      auth.apply(res);
      // Optional avatar upload uses the just-issued token, then patches the user.
      if (avatar) {
        try {
          const { avatarUrl } = await client.uploadAvatar(avatar);
          auth.updateUser({ avatarUrl });
        } catch {
          /* avatar is optional keep the account without it */
        }
      }
    },
    [client, auth],
  );

  const value = useMemo<AuthValue>(
    () => ({
      user: auth.user,
      ready: auth.ready,
      client,
      accounts: auth.accounts,
      login,
      register,
      activate: auth.activate,
      switchProfile: auth.switchProfile,
      forget: auth.forget,
      logout: auth.logout,
      updateUser: auth.updateUser,
    }),
    [auth, client, login, register],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

/** Access the auth context. Throws if used outside `<AuthProvider>`. */
export function useAuth(): AuthValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error('useAuth must be used within <AuthProvider>');
  return ctx;
}
