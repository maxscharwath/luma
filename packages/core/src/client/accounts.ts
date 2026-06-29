// Accounts, sessions, invitations, profile/PIN and Quick Connect device pairing.

import type {
  AuthResult,
  Invite,
  InviteCreated,
  Permission,
  PublicUser,
  QuickConnectInit,
  QuickConnectStatus,
  User,
} from '../types';
import type { RequestContext } from './base';

const JSON_HEADERS = { 'content-type': 'application/json' };

/** Create an account and open a session. After the first (owner) account,
 * `inviteToken` is required — registration is invite-only. Does NOT set the
 * token; the caller persists it (then calls {@link setAuthToken}). */
export function register(
  ctx: RequestContext,
  email: string,
  username: string,
  password: string,
  inviteToken?: string,
): Promise<AuthResult> {
  return ctx.json<AuthResult>('/auth/register', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ email, username, password, inviteToken }),
  });
}

/** Mint a registration invite (requires `users.manage`). */
export function createInvite(
  ctx: RequestContext,
  opts?: { permissions?: Permission[]; expiresInDays?: number },
): Promise<InviteCreated> {
  return ctx.json<InviteCreated>('/invites', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(opts ?? {}),
  });
}

/** Pending invites (requires `users.manage`). */
export function invites(ctx: RequestContext): Promise<Invite[]> {
  return ctx.json<Invite[]>('/invites');
}

/** Check an invite token's validity (public — used by the join page). */
export function checkInvite(
  ctx: RequestContext,
  token: string,
): Promise<{ valid: boolean; expiresAt?: number }> {
  return ctx.json<{ valid: boolean; expiresAt?: number }>(`/invites/${encodeURIComponent(token)}`);
}

/** Revoke an invite (requires `users.manage`). */
export async function revokeInvite(ctx: RequestContext, token: string): Promise<void> {
  await ctx.json<void>(`/invites/${encodeURIComponent(token)}`, { method: 'DELETE' });
}

/** Log in with email-or-username + password → `{ token, user }`. */
export function login(ctx: RequestContext, identifier: string, password: string): Promise<AuthResult> {
  return ctx.json<AuthResult>('/auth/login', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ email: identifier, password }),
  });
}

/** Invalidate the current session server-side (then clear the token locally). */
export async function logout(ctx: RequestContext): Promise<void> {
  await ctx.json<void>('/auth/logout', { method: 'POST' });
}

/** The currently-authenticated user (requires a token). */
export function me(ctx: RequestContext): Promise<{ user: User }> {
  return ctx.json<{ user: User }>('/auth/me');
}

/** Persist the signed-in user's preferred UI locale (synced across their
 * devices) → the updated `{ user }`. Pass `null` to clear it. */
export function updateLanguage(ctx: RequestContext, language: string | null): Promise<{ user: User }> {
  return ctx.json<{ user: User }>('/auth/me', {
    method: 'PATCH',
    headers: JSON_HEADERS,
    body: JSON.stringify({ language }),
  });
}

/** Public profile list for the "Qui regarde ?" picker (no emails). */
export function users(ctx: RequestContext): Promise<PublicUser[]> {
  return ctx.json<PublicUser[]>('/users');
}

/** Verify a profile-lock PIN with the remembered token (TV switch-in). Resolves
 * on 204; throws `LumaApiError` on 401 (wrong) / 429 (locked out — the error's
 * `retryAfter` seconds are surfaced as a cooldown). */
export function pinVerify(ctx: RequestContext, pin: string): Promise<void> {
  return ctx.json<void>('/auth/pin/verify', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ pin }),
  });
}

/** Set or rotate the signed-in user's PIN → the updated `{ user }`. `current`
 * is required when one is already set. */
export function setPin(ctx: RequestContext, pin: string, current?: string): Promise<{ user: User }> {
  return ctx.json<{ user: User }>('/auth/me/pin', {
    method: 'PATCH',
    headers: JSON_HEADERS,
    body: JSON.stringify({ pin, current }),
  });
}

/** Clear the signed-in user's PIN (verifying `current`) → the updated `{ user }`. */
export function clearPin(ctx: RequestContext, current: string): Promise<{ user: User }> {
  return ctx.json<{ user: User }>('/auth/me/pin', {
    method: 'DELETE',
    headers: JSON_HEADERS,
    body: JSON.stringify({ current }),
  });
}

/** Upload the current user's avatar (raw image bytes) → its cached WebP URL. */
export function uploadAvatar(ctx: RequestContext, file: Blob): Promise<{ avatarUrl: string }> {
  return ctx.json<{ avatarUrl: string }>('/users/avatar', {
    method: 'POST',
    headers: { 'content-type': file.type || 'application/octet-stream' },
    body: file,
  });
}

/** Start a Quick Connect request → a code to display + a secret to poll with. */
export function quickConnectInitiate(ctx: RequestContext): Promise<QuickConnectInit> {
  return ctx.json<QuickConnectInit>('/auth/quickconnect/initiate', { method: 'POST' });
}

/** Poll a Quick Connect request by its secret. */
export function quickConnectPoll(ctx: RequestContext, secret: string): Promise<QuickConnectStatus> {
  return ctx.json<QuickConnectStatus>(`/auth/quickconnect/poll?secret=${encodeURIComponent(secret)}`);
}

/** Approve a device's Quick Connect code (requires the approver's token). */
export async function quickConnectAuthorize(ctx: RequestContext, code: string): Promise<void> {
  await ctx.json<void>('/auth/quickconnect/authorize', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ code }),
  });
}
