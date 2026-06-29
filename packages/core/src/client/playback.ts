// Playback progress / resume + live-session heartbeats.

import type { ContinueItem, MediaItem, PlaybackPing, ProgressEntry } from '../types';
import type { RequestContext } from './base';

const JSON_HEADERS = { 'content-type': 'application/json' };

/** All of the user's saved positions. */
export function progress(ctx: RequestContext): Promise<ProgressEntry[]> {
  return ctx.json<ProgressEntry[]>('/progress');
}

/** Saved position for a single item, or null if none. */
export function itemProgress(ctx: RequestContext, itemId: string): Promise<ProgressEntry | null> {
  return ctx.json<ProgressEntry | null>(`/progress/${encodeURIComponent(itemId)}`);
}

/** Resumable items, newest first (the "Reprendre la lecture" rail). */
export function continueWatching(ctx: RequestContext): Promise<ContinueItem[]> {
  return ctx.json<ContinueItem[]>('/continue');
}

/** Personalized "For You" picks from the user's watch history (Bearer). Empty
 * until they've watched something embeddable. */
export function forYou(ctx: RequestContext): Promise<MediaItem[]> {
  return ctx.json<MediaItem[]>('/for-you');
}

/** Save (upsert) the playback position for an item. */
export async function saveProgress(
  ctx: RequestContext,
  itemId: string,
  positionMs: number,
  durationMs?: number | null,
): Promise<void> {
  await ctx.json<void>(`/progress/${encodeURIComponent(itemId)}`, {
    method: 'PUT',
    headers: JSON_HEADERS,
    body: JSON.stringify({ positionMs: Math.round(positionMs), durationMs: durationMs ?? null }),
  });
}

/** Forget an item's position (finished / removed from Continue Watching). */
export async function deleteProgress(ctx: RequestContext, itemId: string): Promise<void> {
  await ctx.json<void>(`/progress/${encodeURIComponent(itemId)}`, { method: 'DELETE' });
}

/** Report playback state so the admin dashboard can show a live session. */
export async function pingPlayback(ctx: RequestContext, ping: PlaybackPing): Promise<void> {
  await ctx.json<void>('/playback/ping', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(ping),
  });
}

/** End a playback session (logs it to history immediately). */
export async function stopPlayback(ctx: RequestContext, sessionId: string): Promise<void> {
  await ctx.json<void>('/playback/stop', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ sessionId }),
  });
}
