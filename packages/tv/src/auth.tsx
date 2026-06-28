// TV per-user session context. The session state machine lives in the shared
// `useAuthSession` hook (@luma/ui); this adds the TV-specific bits: the client is
// passed in (null on the connect screen), the profile picker list is loaded while
// signed out, and `login` feeds a pre-fetched AuthResult straight into `apply`
// (the connect/login screens call the API themselves).

import { type AuthResult, type LumaClient, type PublicUser, type StoredSession } from '@luma/core';
import { useAuthSession } from '@luma/ui';
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from 'react';

interface Auth {
  /** The active session, or null when signed out. */
  session: StoredSession | null;
  /** The signed-in user (null when signed out). */
  user: StoredSession['user'] | null;
  /** Profiles to pick from on the login screen (loaded while signed out). */
  profiles: PublicUser[];
  /** Accounts already signed-in on this device — switchable without a password. */
  accounts: StoredSession[];
  /** Persist a successful auth result and sign in. */
  login: (res: AuthResult) => void;
  /** Switch to a remembered account instantly (no password re-entry). */
  activate: (s: StoredSession) => void;
  /** Back to the picker WITHOUT signing out (keeps remembered accounts). */
  switchProfile: () => void;
  /** Forget a remembered account on this device (real sign-out for it). */
  forget: (userId: string) => void;
  /** Fully sign out of the current account (invalidate + forget this device). */
  logout: () => void;
  /** Merge a patch into the active user, persisting it to the stored session. */
  updateUser: (patch: Partial<StoredSession['user']>) => void;
}

const AuthCtx = createContext<Auth | null>(null);

/**
 * Holds the per-user session and exposes it via `useAuth()` so screens never
 * prop-drill auth. Mounted inside <TvClientProvider> so the `profiles` route and
 * the home ProfileChip read it straight from the hook.
 */
export function AuthProvider({
  client,
  children,
}: Readonly<{
  client: LumaClient | null;
  children: ReactNode;
}>) {
  // Sync hydration so the first authed fetch (e.g. "Reprendre") already carries
  // the token — the TV has no SSR pass to stay consistent with.
  const auth = useAuthSession(client, { syncHydrate: true });
  const [profiles, setProfiles] = useState<PublicUser[]>([]);

  // Load the picker list while signed out.
  useEffect(() => {
    if (auth.session || !client) return;
    let cancelled = false;
    client
      .users()
      .then((u) => {
        if (!cancelled) setProfiles(u);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [client, auth.session]);

  const login = useCallback((res: AuthResult) => auth.apply(res), [auth]);

  const value = useMemo<Auth>(
    () => ({
      session: auth.session,
      user: auth.user,
      profiles,
      accounts: auth.accounts,
      login,
      activate: auth.activate,
      switchProfile: auth.switchProfile,
      forget: auth.forget,
      logout: auth.logout,
      updateUser: auth.updateUser,
    }),
    [auth, profiles, login],
  );
  return <AuthCtx.Provider value={value}>{children}</AuthCtx.Provider>;
}

export function useAuth(): Auth {
  const ctx = useContext(AuthCtx);
  if (!ctx) throw new Error('useAuth() must be used inside <AuthProvider>');
  return ctx;
}
