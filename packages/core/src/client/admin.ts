// Admin console: server identity, live sessions, metrics/storage, users,
// settings and watch stats.

import type {
  AdminOverview,
  AdminUsers,
  HistoryStats,
  MetricsSnapshot,
  Permission,
  PlaybackSession,
  ServerInfo,
  SettingsView,
  StorageInfo,
  TopUser,
} from '../types';
import type { RequestContext } from './base';

const JSON_HEADERS = { 'content-type': 'application/json' };

/** Server identity + uptime (requires an admin capability). */
export function adminServer(ctx: RequestContext): Promise<ServerInfo> {
  return ctx.json<ServerInfo>('/admin/server');
}

/** Live playback sessions for the dashboard. */
export function adminSessions(ctx: RequestContext): Promise<{ sessions: PlaybackSession[] }> {
  return ctx.json<{ sessions: PlaybackSession[] }>('/admin/sessions');
}

/** Terminate a live playback session; the owning client stops and shows
 * `message` (empty → the client's localized default). */
export async function terminateSession(ctx: RequestContext, id: string, message?: string): Promise<void> {
  await ctx.json<void>(`/admin/sessions/${encodeURIComponent(id)}/stop`, {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ message: message ?? '' }),
  });
}

/** CPU / RAM / bandwidth snapshot + history (poll for live charts). */
export function adminMetrics(ctx: RequestContext): Promise<MetricsSnapshot> {
  return ctx.json<MetricsSnapshot>('/admin/metrics');
}

/** Volumes, totals and cache usage. */
export function adminStorage(ctx: RequestContext): Promise<StorageInfo> {
  return ctx.json<StorageInfo>('/admin/storage');
}

/** Wipe transcode + image caches (requires `settings.manage`). */
export function clearCache(ctx: RequestContext): Promise<{ freedBytes: number }> {
  return ctx.json<{ freedBytes: number }>('/admin/cache/clear', { method: 'POST' });
}

/** Full member list (requires `users.manage`). */
export function adminUsers(ctx: RequestContext): Promise<AdminUsers> {
  return ctx.json<AdminUsers>('/admin/users');
}

/** Update a user's permissions and/or username. */
export async function updateUser(
  ctx: RequestContext,
  id: string,
  patch: { permissions?: Permission[]; username?: string },
): Promise<void> {
  await ctx.json<void>(`/admin/users/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    headers: JSON_HEADERS,
    body: JSON.stringify(patch),
  });
}

/** Delete a user account. */
export async function deleteUser(ctx: RequestContext, id: string): Promise<void> {
  await ctx.json<void>(`/admin/users/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

/** Grouped settings schema + current values for one view. */
export function adminSettings(ctx: RequestContext, view: string): Promise<SettingsView> {
  return ctx.json<SettingsView>(`/admin/settings?view=${encodeURIComponent(view)}`);
}

/** Persist a settings patch → the keys actually written. */
export function updateSettings(
  ctx: RequestContext,
  patch: Record<string, unknown>,
): Promise<{ updated: string[] }> {
  return ctx.json<{ updated: string[] }>('/admin/settings', {
    method: 'PUT',
    headers: JSON_HEADERS,
    body: JSON.stringify(patch),
  });
}

/** Per-user watch aggregates over the last `days` (default 7). */
export function topUsers(ctx: RequestContext, days = 7): Promise<{ users: TopUser[] }> {
  return ctx.json<{ users: TopUser[] }>(`/admin/stats/top-users?days=${days}`);
}

/** Weekly films-vs-TV watch buckets over the last `days` (default 28). */
export function playHistory(ctx: RequestContext, days = 28): Promise<HistoryStats> {
  return ctx.json<HistoryStats>(`/admin/stats/history?days=${days}`);
}

/** Top-line counts for the users page. */
export function adminOverview(ctx: RequestContext): Promise<AdminOverview> {
  return ctx.json<AdminOverview>('/admin/stats/overview');
}
