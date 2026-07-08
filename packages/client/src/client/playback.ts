// Playback progress / resume + live-session heartbeats.

import type { ContinueItem, MediaItem, PlaybackPing, ProgressEntry, UpNext } from '../types';
import type { RequestContext } from './base';

const JSON_HEADERS = { 'content-type': 'application/json' };

/** All of the user's saved positions. */
export function progress(ctx: RequestContext): Promise<ProgressEntry[]> {
  return ctx.json<ProgressEntry[]>('/progress');
}

/** The episode to play to CONTINUE a show (resume in-progress, else next
 * unwatched, else first) + a `resume` flag for the button label. `null` when the
 * show has no episodes. */
export function upNext(ctx: RequestContext, showId: string): Promise<UpNext | null> {
  return ctx.json<UpNext | null>(`/shows/${encodeURIComponent(showId)}/up-next`);
}

/** The next episode after `itemId` in its show (sequence order), or `null` for a
 * movie / the last episode. Drives player autoplay. */
export function nextEpisode(ctx: RequestContext, itemId: string): Promise<MediaItem | null> {
  return ctx.json<MediaItem | null>(`/items/${encodeURIComponent(itemId)}/next`);
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

/** Item ids the user has marked (or finished) as watched. Clients hydrate this
 * into a set once and badge cards from it. */
export function watched(ctx: RequestContext): Promise<string[]> {
  return ctx.json<string[]>('/watched');
}

/** Mark an item as watched (also clears its resume position). */
export async function markWatched(ctx: RequestContext, itemId: string): Promise<void> {
  await ctx.json<void>(`/watched/${encodeURIComponent(itemId)}`, { method: 'PUT' });
}

/** Clear an item's watched flag. */
export async function unmarkWatched(ctx: RequestContext, itemId: string): Promise<void> {
  await ctx.json<void>(`/watched/${encodeURIComponent(itemId)}`, { method: 'DELETE' });
}

/** Item/show ids in the user's "Ma liste" (newest first). Hydrated into a set. */
export function myList(ctx: RequestContext): Promise<string[]> {
  return ctx.json<string[]>('/my-list');
}

/** Add a title to the user's list. */
export async function addToList(ctx: RequestContext, itemId: string): Promise<void> {
  await ctx.json<void>(`/my-list/${encodeURIComponent(itemId)}`, { method: 'PUT' });
}

/** Remove a title from the user's list. */
export async function removeFromList(ctx: RequestContext, itemId: string): Promise<void> {
  await ctx.json<void>(`/my-list/${encodeURIComponent(itemId)}`, { method: 'DELETE' });
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
