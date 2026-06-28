// Client-side session persistence. Two things live in `localStorage`:
//   • the ACTIVE session (token + user) so a reload / relaunch stays signed in;
//   • the list of accounts that have signed in on THIS device, so switching back
//     to one of them is instant (no password) — Netflix-style. Guarded by
//     `typeof` so it's a no-op during SSR / non-DOM runtimes.

import type { User } from './types';

const KEY = 'luma.session'; // active session
const ACCOUNTS_KEY = 'luma.accounts'; // remembered sessions on this device
const LOCALE_KEY = 'luma.locale'; // device-level UI locale override

export interface StoredSession {
  token: string;
  user: User;
}

function storage(): Storage | null {
  try {
    return typeof localStorage !== 'undefined' ? localStorage : null;
  } catch {
    // Access to localStorage can throw (privacy mode / sandboxed iframe).
    return null;
  }
}

function readJson<T>(key: string, fallback: T): T {
  const raw = storage()?.getItem(key);
  if (!raw) return fallback;
  try {
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

function writeJson(key: string, value: unknown): void {
  try {
    storage()?.setItem(key, JSON.stringify(value));
  } catch {
    /* quota / disabled storage — non-fatal */
  }
}

/** The active session, or null when signed out. */
export function loadSession(): StoredSession | null {
  const s = readJson<StoredSession | null>(KEY, null);
  return s?.token && s.user ? s : null;
}

/** Set the active session AND remember the account on this device. */
export function saveSession(session: StoredSession): void {
  writeJson(KEY, session);
  const accounts = loadAccounts().filter((a) => a.user.id !== session.user.id);
  accounts.unshift(session);
  writeJson(ACCOUNTS_KEY, accounts);
}

/** Clear only the ACTIVE session (e.g. "switch profile"). Remembered accounts
 * stay, so switching back to one is still password-free. */
export function clearSession(): void {
  try {
    storage()?.removeItem(KEY);
  } catch {
    /* ignore */
  }
}

/** Accounts that have signed in on this device (most-recent first). */
export function loadAccounts(): StoredSession[] {
  return readJson<StoredSession[]>(ACCOUNTS_KEY, []).filter((a) => a?.token && a?.user);
}

/** The device-level UI locale override (what the user last picked on THIS
 * device), or null. Used before sign-in and as a fallback when the account has
 * no preference. */
export function loadLocalePref(): string | null {
  try {
    return storage()?.getItem(LOCALE_KEY) ?? null;
  } catch {
    return null;
  }
}

/** Persist (or clear, with `null`) the device-level UI locale override. */
export function saveLocalePref(locale: string | null): void {
  try {
    if (locale) storage()?.setItem(LOCALE_KEY, locale);
    else storage()?.removeItem(LOCALE_KEY);
  } catch {
    /* quota / disabled storage — non-fatal */
  }
}

/** Forget one remembered account (full sign-out for it); also clears the active
 * session when it was the one being forgotten. */
export function forgetAccount(userId: string): void {
  writeJson(
    ACCOUNTS_KEY,
    loadAccounts().filter((a) => a.user.id !== userId),
  );
  if (loadSession()?.user.id === userId) clearSession();
}
