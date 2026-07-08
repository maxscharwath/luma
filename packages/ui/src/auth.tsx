// The shared per-user session state machine behind each client's <AuthProvider>.
//
// Token model: localStorage holds only a long-lived ACCESS token per remembered
// account (never a bearer). On boot we silently exchange the active account's
// access token for a short-lived SESSION token, kept in memory (see
// `setSessionToken` in @luma/core) and set as the client's bearer. A 401 during
// use triggers a silent re-exchange (the `refreshHandler`). Switching INTO a
// PIN-locked profile requires the PIN on the exchange; returning to the picker
// re-locks it (`relock`) so the next switch-in re-prompts.

import {
  type AuthResult,
  apiErrorText,
  clearSession,
  forgetAccount,
  LumaApiError,
  type LumaClient,
  loadAccounts,
  loadSession,
  type StoredSession,
  saveSession,
  setSessionToken,
  type User,
} from '@luma/core';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

/** Outcome of an {@link AuthSession.activate}. `needsPin` asks the UI to collect
 * the profile PIN and call `activate` again with it; `retryAfter` (seconds) is
 * set when the PIN was rate-limited (429) so the UI can show a cooldown. */
export type ActivateResult =
  | { ok: true }
  | { ok: false; needsPin: boolean; error?: string; retryAfter?: number };

export interface AuthSession {
  /** The active account (access token + user), or null when signed out. */
  session: StoredSession | null;
  /** The signed-in user (null when signed out). */
  user: User | null;
  /** Accounts remembered on this device switchable via {@link activate}. */
  accounts: StoredSession[];
  /** True once storage hydration + the boot token exchange have run. */
  ready: boolean;
  /** Persist a login/register result (access token + first session) and sign in. */
  apply: (res: AuthResult) => void;
  /** Switch into a remembered account by exchanging its access token for a
   * session. Pass `pin` for a PIN-locked profile (required on switch-in). */
  activate: (s: StoredSession, pin?: string) => Promise<ActivateResult>;
  /** Back to the picker WITHOUT forgetting the account: drop the in-memory
   * session and re-lock the access token so the next switch-in re-prompts. */
  switchProfile: () => void;
  /** Forget a remembered account on this device (revokes it when it's active). */
  forget: (userId: string) => void;
  /** Fully sign out of the current account (revoke tokens + forget this device). */
  logout: () => Promise<void>;
  /** Merge a patch into the active user, persisting it to the stored session. */
  updateUser: (patch: Partial<User>) => void;
}

export function useAuthSession(client: LumaClient | null): AuthSession {
  const [session, setSession] = useState<StoredSession | null>(null);
  const [accounts, setAccounts] = useState<StoredSession[]>([]);
  const [ready, setReady] = useState(false);

  /** Adopt a freshly-minted session token: memory + client bearer + stored user. */
  const adopt = useCallback(
    (stored: StoredSession, token: string, user: User) => {
      setSessionToken(token);
      client?.setAuthToken(token);
      const next: StoredSession = { ...stored, user };
      saveSession(next);
      setSession(next);
      setAccounts(loadAccounts());
    },
    [client],
  );

  // Silent-refresh handler: on a 401, re-exchange the active access token (no
  // PIN) for a new session. Deduped so concurrent 401s share one exchange.
  const refreshing = useRef<Promise<string | undefined> | null>(null);
  useEffect(() => {
    if (!client) return;
    client.setRefreshHandler(() => {
      if (refreshing.current) return refreshing.current;
      const active = loadSession();
      if (!active) return Promise.resolve(undefined);
      const p = client
        .exchangeToken(active.accessToken)
        .then((res) => {
          setSessionToken(res.token);
          client.setAuthToken(res.token);
          setSession((cur) => (cur ? { ...cur, user: res.user } : cur));
          saveSession({ ...active, user: res.user });
          return res.token as string | undefined;
        })
        .catch(() => {
          // The access token is dead (revoked/expired, or a PIN was added
          // elsewhere so no-PIN exchange 401s). Drop the session so the login
          // gate reappears instead of a zombie 'signed-in' state that 401s every
          // request forever. The account stays remembered for a re-login/PIN.
          setSessionToken(undefined);
          client.setAuthToken(undefined);
          clearSession();
          setSession(null);
          return undefined;
        })
        .finally(() => {
          refreshing.current = null;
        });
      refreshing.current = p;
      return p;
    });
    return () => client.setRefreshHandler(undefined);
  }, [client]);

  // Boot: hydrate remembered accounts + silently exchange the active access token
  // for a session. On failure (PIN needed / expired) drop to the picker but keep
  // the account remembered. Runs once per client.
  // biome-ignore lint/correctness/useExhaustiveDependencies: boot once per client.
  useEffect(() => {
    setAccounts(loadAccounts());
    const active = loadSession();
    if (!client || !active) {
      setReady(true);
      return;
    }
    let cancelled = false;
    client
      .exchangeToken(active.accessToken)
      .then((res) => {
        if (cancelled) return;
        adopt(active, res.token, res.user);
      })
      .catch(() => {
        if (cancelled) return;
        // Can't silently resume (PIN required / token invalid): show the picker.
        setSessionToken(undefined);
        client.setAuthToken(undefined);
        clearSession();
        setSession(null);
      })
      .finally(() => {
        if (!cancelled) setReady(true);
      });
    return () => {
      cancelled = true;
    };
  }, [client]);

  const apply = useCallback(
    (res: AuthResult) => {
      adopt({ accessToken: res.accessToken, user: res.user }, res.token, res.user);
    },
    [adopt],
  );

  const activate = useCallback(
    async (s: StoredSession, pin?: string): Promise<ActivateResult> => {
      if (!client) return { ok: false, needsPin: false };
      try {
        const res = await client.exchangeToken(s.accessToken, pin);
        adopt(s, res.token, res.user);
        return { ok: true };
      } catch (e) {
        if (e instanceof LumaApiError) {
          // Rate-limited (429): too many wrong PINs. Keep the PIN screen and pass
          // the cooldown so the UI can count it down.
          if (e.status === 429) {
            const retryAfter = Number(
              (e.body as { retryAfter?: number } | undefined)?.retryAfter ?? 30,
            );
            return { ok: false, needsPin: true, retryAfter, error: apiErrorText(e, '') };
          }
          // A PIN-locked profile 401s until the correct PIN is supplied ask the
          // UI to collect it. Trust the server's `pinRequired` flag (so a PIN
          // added on another device is handled even if our cached `hasPin` is
          // stale), falling back to that cached flag. Any other 401 is a dead
          // access token.
          const pinRequired =
            (e.body as { pinRequired?: boolean } | undefined)?.pinRequired === true;
          if (e.status === 401 && (pinRequired || s.user.hasPin)) {
            return { ok: false, needsPin: true, error: apiErrorText(e, '') };
          }
        }
        return { ok: false, needsPin: false, error: apiErrorText(e, '') };
      }
    },
    [client, adopt],
  );

  const switchProfile = useCallback(() => {
    const active = loadSession();
    if (active && client) client.relock(active.accessToken).catch(() => {});
    setSessionToken(undefined);
    client?.setAuthToken(undefined);
    clearSession();
    setSession(null);
  }, [client]);

  const forget = useCallback(
    (userId: string) => {
      const active = loadSession();
      // Revoke server-side only when it's the active account (the logout call
      // also drops the current bearer, which we're discarding anyway).
      if (active?.user.id === userId && client) client.logout(active.accessToken).catch(() => {});
      forgetAccount(userId);
      setAccounts(loadAccounts());
      setSession((s) => {
        if (s?.user.id === userId) {
          setSessionToken(undefined);
          client?.setAuthToken(undefined);
          return null;
        }
        return s;
      });
    },
    [client],
  );

  const logout = useCallback(async () => {
    const active = loadSession();
    try {
      await client?.logout(active?.accessToken);
    } catch {
      /* best-effort server-side revocation */
    }
    setSessionToken(undefined);
    client?.setAuthToken(undefined);
    if (active) forgetAccount(active.user.id);
    else clearSession();
    setAccounts(loadAccounts());
    setSession(null);
  }, [client]);

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
