// Accounts, sessions, invitations, profile/PIN and Quick Connect device pairing.

import {
  AuthConfig,
  AuthResult,
  type Invite,
  type InviteCreated,
  PasskeyInfo,
  type Permission,
  PublicUser,
  type QuickConnectInit,
  type QuickConnectStatus,
  SessionInfo,
  SessionResult,
  User,
  validate,
} from '../schemas';
import type { RequestContext } from './base';

const JSON_HEADERS = { 'content-type': 'application/json' };

/** Create an account and open a session. After the first (owner) account,
 * `inviteToken` is required registration is invite-only. Does NOT set the
 * token; the caller persists it (then calls {@link setAuthToken}). */
export function register(
  ctx: RequestContext,
  email: string,
  username: string,
  password: string,
  inviteToken?: string,
): Promise<AuthResult> {
  return ctx
    .json<AuthResult>('/auth/register', {
      method: 'POST',
      headers: JSON_HEADERS,
      body: JSON.stringify({ email, username, password, inviteToken }),
    })
    .then((r) => validate(AuthResult, r));
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

/** Check an invite token's validity (public used by the join page). */
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
export function login(
  ctx: RequestContext,
  identifier: string,
  password: string,
): Promise<AuthResult> {
  return ctx
    .json<AuthResult>('/auth/login', {
      method: 'POST',
      headers: JSON_HEADERS,
      body: JSON.stringify({ email: identifier, password }),
    })
    .then((r) => validate(AuthResult, r));
}

/** Exchange the long-lived access token for a short-lived session token. Pass
 * `pin` when switching into a PIN-locked profile (required on the first exchange;
 * silent refreshes omit it). Throws `LumaApiError` 401 when the PIN is needed or
 * the access token is invalid/expired. */
export function exchangeToken(
  ctx: RequestContext,
  accessToken: string,
  pin?: string,
): Promise<{ token: string; user: User }> {
  return ctx
    .json<{ token: string; user: User }>('/auth/token', {
      method: 'POST',
      headers: JSON_HEADERS,
      body: JSON.stringify({ accessToken, pin }),
    })
    .then((r) => validate(SessionResult, r));
}

/** Re-lock an access token (clear its PIN-verified flag) so the next exchange
 * re-prompts for the PIN. Called when returning to the profile picker. */
export async function relock(ctx: RequestContext, accessToken: string): Promise<void> {
  await ctx.json<void>('/auth/relock', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ accessToken }),
  });
}

/** Invalidate the current session server-side and revoke the device's access
 * token (a full disconnect), then clear the tokens locally. */
export async function logout(ctx: RequestContext, accessToken?: string): Promise<void> {
  await ctx.json<void>('/auth/logout', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ accessToken }),
  });
}

/** The currently-authenticated user (requires a token). */
export function me(ctx: RequestContext): Promise<{ user: User }> {
  return ctx.json<{ user: User }>('/auth/me').then((r) => {
    User.parse(r.user);
    return r;
  });
}

/** A partial patch of the signed-in account's own profile. Omitted keys are left
 * unchanged; a `null` clears the field (only the language prefs are clearable
 * server-side username/email must be non-empty). All fields persist server-side
 * and sync across the account's devices. */
export interface AccountPatch {
  /** New display name (non-empty). */
  username?: string;
  /** New email (valid, unused; stored lower-cased). */
  email?: string;
  /** Preferred UI locale (`"fr"` | `"en"`), or `null` to clear. */
  language?: string | null;
  /** Preferred audio-track language (ISO code), or `null` to clear. */
  audioLanguage?: string | null;
  /** Preferred subtitle-track language (ISO code or `"off"`), or `null` to clear. */
  subtitleLanguage?: string | null;
}

/** Update the signed-in account's own profile → the updated `{ user }`. Sends
 * only the fields present in `patch` (see {@link AccountPatch}). */
export function updateAccount(ctx: RequestContext, patch: AccountPatch): Promise<{ user: User }> {
  return ctx.json<{ user: User }>('/auth/me', {
    method: 'PATCH',
    headers: JSON_HEADERS,
    body: JSON.stringify(patch),
  });
}

/** Persist the signed-in user's preferred UI locale (synced across their
 * devices) → the updated `{ user }`. Pass `null` to clear it. */
export function updateLanguage(
  ctx: RequestContext,
  language: string | null,
): Promise<{ user: User }> {
  return updateAccount(ctx, { language });
}

/** Change the signed-in account's password after verifying the current one.
 * Resolves on 204; throws `LumaApiError` on 401 (wrong current) / 400 (too
 * short). There is no email-based reset (LAN self-hosted, no mail service). */
export async function changePassword(
  ctx: RequestContext,
  current: string,
  next: string,
): Promise<void> {
  await ctx.json<void>('/auth/me/password', {
    method: 'PATCH',
    headers: JSON_HEADERS,
    body: JSON.stringify({ current, next }),
  });
}

/** Public login-gate config read before any credential: whether the profile
 * roster is public and whether any account exists yet. Lets the client decide
 * between the picker, a plain email/password form, and first-run registration. */
export function authConfig(ctx: RequestContext): Promise<AuthConfig> {
  return ctx.json<AuthConfig>('/auth/config').then((r) => validate(AuthConfig, r));
}

/** Public profile list for the "Qui regarde ?" picker (no emails). Empty when
 * the `publicUserList` setting is off (see {@link authConfig}). */
export function users(ctx: RequestContext): Promise<PublicUser[]> {
  return ctx.json<PublicUser[]>('/users').then((r) => validate(PublicUser.array(), r));
}

/** Verify a profile-lock PIN with the remembered token (TV switch-in). Resolves
 * on 204; throws `LumaApiError` on 401 (wrong) / 429 (locked out the error's
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
export function setPin(
  ctx: RequestContext,
  pin: string,
  current?: string,
): Promise<{ user: User }> {
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

/** The signed-in account's active devices (sessions), newest first, with the
 * current device flagged. Backed by the long-lived per-device access tokens. */
export function sessions(ctx: RequestContext): Promise<SessionInfo[]> {
  return ctx.json<SessionInfo[]>('/auth/me/sessions').then((r) => validate(SessionInfo.array(), r));
}

/** Revoke one of the account's own devices by its id (from {@link sessions}),
 * signing it out. Resolves on 204; throws `LumaApiError` 404 if unknown. */
export async function revokeSession(ctx: RequestContext, id: string): Promise<void> {
  await ctx.json<void>(`/auth/me/sessions/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

// ----- passkeys (WebAuthn) ----------------------------------------------------

/** Opaque WebAuthn ceremony payloads. Their shape is defined by the platform
 * (`navigator.credentials`), not by us the web layer converts the binary
 * fields to/from `ArrayBuffer` around these. */
export type WebAuthnOptions = { publicKey: Record<string, unknown> };
export type WebAuthnCredential = Record<string, unknown>;

/** Begin registering a passkey → `{ ceremonyId, options }`. `options` feeds
 * `navigator.credentials.create`; echo `ceremonyId` back to finish. (Bearer.) */
export function passkeyRegisterStart(
  ctx: RequestContext,
): Promise<{ ceremonyId: string; options: WebAuthnOptions }> {
  return ctx.json('/auth/me/passkeys/register/start', { method: 'POST' });
}

/** Finish registering a passkey with the browser's credential → the stored
 * {@link PasskeyInfo}. (Bearer.) */
export function passkeyRegisterFinish(
  ctx: RequestContext,
  body: { ceremonyId: string; name: string; credential: WebAuthnCredential },
): Promise<PasskeyInfo> {
  return ctx
    .json<PasskeyInfo>('/auth/me/passkeys/register/finish', {
      method: 'POST',
      headers: JSON_HEADERS,
      body: JSON.stringify(body),
    })
    .then((r) => validate(PasskeyInfo, r));
}

/** The account's registered passkeys, newest first. (Bearer.) */
export function passkeys(ctx: RequestContext): Promise<PasskeyInfo[]> {
  return ctx.json<PasskeyInfo[]>('/auth/me/passkeys').then((r) => validate(PasskeyInfo.array(), r));
}

/** Remove one of the account's passkeys by id. (Bearer.) */
export async function deletePasskey(ctx: RequestContext, id: string): Promise<void> {
  await ctx.json<void>(`/auth/me/passkeys/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

/** Begin usernameless (discoverable) passwordless sign-in → `{ ceremonyId,
 * options }`. `options` feeds `navigator.credentials.get`; the browser lets the
 * user pick which account. Public. */
export function passkeyAuthStart(
  ctx: RequestContext,
): Promise<{ ceremonyId: string; options: WebAuthnOptions }> {
  return ctx.json('/auth/passkeys/authenticate/start', { method: 'POST' });
}

/** Finish passwordless sign-in with the browser's assertion → `{ token,
 * accessToken, user }` (same shape as password login). Public. */
export function passkeyAuthFinish(
  ctx: RequestContext,
  body: { ceremonyId: string; credential: WebAuthnCredential },
): Promise<AuthResult> {
  return ctx
    .json<AuthResult>('/auth/passkeys/authenticate/finish', {
      method: 'POST',
      headers: JSON_HEADERS,
      body: JSON.stringify(body),
    })
    .then((r) => validate(AuthResult, r));
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
  return ctx.json<QuickConnectStatus>(
    `/auth/quickconnect/poll?secret=${encodeURIComponent(secret)}`,
  );
}

/** Approve a device's Quick Connect code (requires the approver's token). */
export async function quickConnectAuthorize(ctx: RequestContext, code: string): Promise<void> {
  await ctx.json<void>('/auth/quickconnect/authorize', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ code }),
  });
}
