// Auth + server session, on the same multi-server / multi-profile model as the
// TV client: any number of saved servers, any number of remembered accounts
// (device credentials in SecureStore), one active session. Two-token model:
// the long-lived accessToken is stored and exchanged on demand for a
// short-lived bearer kept in memory; a 401 mid-flight silently re-exchanges.

import { KromaClient, normalizeServerUrl, type User } from '@kroma/core';
import { createContext, type ReactNode, useCallback, useContext, useMemo, useState } from 'react';
import {
  deletePinBehindBiometrics,
  type MobileAccount,
  type ServerEntry,
  saveActive,
  setBiometricLockEnabled,
} from '../storage';
import { useBootRestore } from './boot';
import { sameAccount, useAccountStore, useServerStore } from './stores';

export interface AuthSession {
  status: 'booting' | 'signedOut' | 'signedIn';
  /** The server the connect/sign-in flow is pointed at (last used). */
  serverUrl: string | null;
  client: KromaClient | null;
  user: User | null;
  /** Saved servers, most recently used first. */
  servers: ServerEntry[];
  /** Every remembered account on this device (all servers). */
  accounts: MobileAccount[];
  connect(url: string): Promise<void>;
  /** Point the sign-in flow at another saved server. */
  selectServer(url: string): void;
  /** Refresh a saved server's display name (from a `/health` probe). */
  renameServer(url: string, name: string): void;
  login(identifier: string, password: string): Promise<void>;
  /** Enter a remembered account without a password (stored device token).
   * PIN-locked profiles need `pin` (401 body `pinRequired: true` otherwise). */
  switchAccount(account: MobileAccount, pin?: string): Promise<void>;
  /** Leave the session but KEEP the account remembered (profile switcher). */
  switchProfile(): void;
  /** Leave AND forget the active account. */
  signOut(): Promise<void>;
  forgetAccount(account: MobileAccount): void;
  forgetServer(url: string): void;
  setUser(user: User): void;
}

const Ctx = createContext<AuthSession | null>(null);

export function useSession(): AuthSession {
  const value = useContext(Ctx);
  if (!value) throw new Error('useSession outside SessionProvider');
  return value;
}

/** The signed-in client; screens behind the auth gate can rely on it. */
export function useClient(): KromaClient {
  const { client } = useSession();
  if (!client) throw new Error('useClient before sign-in');
  return client;
}

function makeClient(serverUrl: string): KromaClient {
  return new KromaClient({ baseUrl: serverUrl });
}

export function SessionProvider({ children }: Readonly<{ children: ReactNode }>) {
  const [status, setStatus] = useState<AuthSession['status']>('booting');
  const [serverUrl, setServerUrl] = useState<string | null>(null);
  const [user, setUserState] = useState<User | null>(null);
  const [client, setClient] = useState<KromaClient | null>(null);
  const accounts = useAccountStore();
  const servers = useServerStore();

  const enterSession = useCallback(
    (url: string, accessToken: string, token: string, freshUser: User) => {
      const next = makeClient(url);
      next.setAuthToken(token);
      next.setRefreshHandler(async () => {
        try {
          const { token: newToken, user: refreshed } = await next.exchangeToken(accessToken);
          next.setAuthToken(newToken);
          setUserState(refreshed);
          return newToken;
        } catch {
          return undefined;
        }
      });
      setClient(next);
      setServerUrl(url);
      setUserState(freshUser);
      setStatus('signedIn');
      const account: MobileAccount = { serverUrl: url, accessToken, user: freshUser };
      accounts.persist([
        account,
        ...accounts.ref.current.filter((a) => !sameAccount(a, url, freshUser.id)),
      ]);
      void saveActive({ serverUrl: url, userId: freshUser.id });
      servers.touch(url);
    },
    [accounts, servers],
  );

  const setSignedOut = useCallback(() => setStatus('signedOut'), []);
  useBootRestore({ accounts, servers, makeClient, enterSession, setServerUrl, setSignedOut });

  const connect = useCallback(
    async (url: string) => {
      // A bare host ("luma.stmx.ch") tries https first, then http, so users
      // never have to type the scheme. Each probe validates that this really
      // is a Kroma server before saving.
      const raw = url.trim();
      const candidates = /^https?:\/\//i.test(raw) ? [raw] : [`https://${raw}`, `http://${raw}`];
      let lastError: unknown;
      for (const candidate of candidates) {
        const normalized = normalizeServerUrl(candidate);
        const abort = new AbortController();
        const timer = setTimeout(() => abort.abort(), 5000);
        try {
          const health = await makeClient(normalized).health({ signal: abort.signal });
          setServerUrl(normalized);
          // `name` is LAN-only: a server reached over the internet answers
          // without one, and `touch` then keeps whatever label we already had.
          servers.touch(normalized, health.name);
          return;
        } catch (err) {
          lastError = err;
        } finally {
          clearTimeout(timer);
        }
      }
      throw lastError ?? new Error('unreachable');
    },
    [servers],
  );

  const selectServer = useCallback((url: string) => setServerUrl(url), []);

  const login = useCallback(
    async (identifier: string, password: string) => {
      if (!serverUrl) throw new Error('no server');
      const result = await makeClient(serverUrl).login(identifier, password);
      enterSession(serverUrl, result.accessToken, result.token, result.user);
    },
    [serverUrl, enterSession],
  );

  const switchAccount = useCallback(
    async (account: MobileAccount, pin?: string) => {
      const probe = makeClient(account.serverUrl);
      const { token, user: fresh } = await probe.exchangeToken(account.accessToken, pin);
      enterSession(account.serverUrl, account.accessToken, token, fresh);
    },
    [enterSession],
  );

  /** Drop every device-local secret tied to a profile. */
  const forgetSecrets = useCallback((url: string, userId: string) => {
    void deletePinBehindBiometrics(url, userId);
    void setBiometricLockEnabled(url, userId, false);
  }, []);

  const leave = useCallback(
    (forgetActive: boolean) => {
      const current = client;
      const activeUser = user;
      const active =
        activeUser && serverUrl
          ? accounts.ref.current.find((a) => sameAccount(a, serverUrl, activeUser.id))
          : undefined;
      setClient(null);
      setUserState(null);
      setStatus('signedOut');
      void saveActive(null);
      if (forgetActive && activeUser && serverUrl) {
        accounts.forget(serverUrl, activeUser.id);
        forgetSecrets(serverUrl, activeUser.id);
        if (current && active) void current.logout(active.accessToken).catch(() => undefined);
      } else if (current && active) {
        // Back to the profile picker: re-arm the PIN gate on this credential.
        void current.relock(active.accessToken).catch(() => undefined);
      }
    },
    [client, user, serverUrl, accounts, forgetSecrets],
  );

  const switchProfile = useCallback(() => leave(false), [leave]);
  const signOut = useCallback(async () => leave(true), [leave]);

  const forgetAccount = useCallback(
    (account: MobileAccount) => {
      accounts.forget(account.serverUrl, account.user.id);
      forgetSecrets(account.serverUrl, account.user.id);
    },
    [accounts, forgetSecrets],
  );

  const forgetServer = useCallback(
    (url: string) => {
      servers.remove(url);
      accounts.persist(accounts.ref.current.filter((a) => a.serverUrl !== url));
      setServerUrl((current) => (current === url ? null : current));
    },
    [accounts, servers],
  );

  const setUser = useCallback(
    (u: User) => {
      setUserState(u);
      if (!serverUrl) return;
      accounts.persist(
        accounts.ref.current.map((a) => (sameAccount(a, serverUrl, u.id) ? { ...a, user: u } : a)),
      );
    },
    [serverUrl, accounts],
  );

  const renameServer = servers.rename;
  const value = useMemo<AuthSession>(
    () => ({
      status,
      serverUrl,
      client,
      user,
      servers: servers.servers,
      accounts: accounts.accounts,
      connect,
      selectServer,
      renameServer,
      login,
      switchAccount,
      switchProfile,
      signOut,
      forgetAccount,
      forgetServer,
      setUser,
    }),
    [
      status,
      serverUrl,
      client,
      user,
      servers.servers,
      accounts.accounts,
      connect,
      selectServer,
      renameServer,
      login,
      switchAccount,
      switchProfile,
      signOut,
      forgetAccount,
      forgetServer,
      setUser,
    ],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
