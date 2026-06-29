// Catalogue reads, art/stream URL builders and subtitles.

import type {
  Activity,
  Health,
  Library,
  MediaItem,
  Metadata,
  ScanResult,
  SearchResponse,
  Section,
  Show,
  ShowDetail,
} from '../types';
import { LumaApiError, libraryQuery, type RequestContext } from './base';

export function health(ctx: RequestContext): Promise<Health> {
  return ctx.json<Health>('/health');
}

export function libraries(ctx: RequestContext): Promise<Library[]> {
  return ctx.json<Library[]>('/libraries');
}

/** All playable items (movies + episodes). */
export function items(ctx: RequestContext, libraryId?: string): Promise<MediaItem[]> {
  return ctx.json<MediaItem[]>(`/items${libraryQuery(libraryId)}`);
}

/** Movies only (excludes episodes). */
export function movies(ctx: RequestContext, libraryId?: string): Promise<MediaItem[]> {
  return ctx.json<MediaItem[]>(`/movies${libraryQuery(libraryId)}`);
}

/** TV shows (aggregates). */
export function shows(ctx: RequestContext, libraryId?: string): Promise<Show[]> {
  return ctx.json<Show[]>(`/shows${libraryQuery(libraryId)}`);
}

/** One show with its seasons + episodes. */
export function show(ctx: RequestContext, id: string): Promise<ShowDetail> {
  return ctx.json<ShowDetail>(`/shows/${encodeURIComponent(id)}`);
}

export function item(ctx: RequestContext, id: string): Promise<MediaItem> {
  return ctx.json<MediaItem>(`/items/${encodeURIComponent(id)}`);
}

/** "More like this": content-embedding neighbours of a title (public). */
export function similar(ctx: RequestContext, id: string): Promise<MediaItem[]> {
  return ctx.json<MediaItem[]>(`/items/${encodeURIComponent(id)}/similar`);
}

/** Zero-shot themed row: titles matching a free-text phrase (e.g. "christmas
 * movie", "action"), ranked by content-embedding similarity (public). */
export function themed(ctx: RequestContext, query: string): Promise<MediaItem[]> {
  return ctx.json<MediaItem[]>(`/themed?q=${encodeURIComponent(query)}`);
}

/** The generated home screen (Bearer): an ordered, server-assembled list of
 * section rails (For You, "because you watched …", themed, trending, recently
 * added) — already localized + de-duplicated. Clients render it generically. */
export function home(ctx: RequestContext): Promise<Section[]> {
  return ctx.json<Section[]>('/home');
}

/** Full-text catalogue search (movies, shows, episodes). Server-side
 * field-weighted, typo-tolerant ranking — well suited to imperfect voice
 * transcripts. `limit` caps results (the server clamps to 60). */
export function search(
  ctx: RequestContext,
  query: string,
  opts?: { libraryId?: string; limit?: number },
): Promise<SearchResponse> {
  const params = new URLSearchParams({ q: query });
  if (opts?.limit) params.set('limit', String(opts.limit));
  if (opts?.libraryId) params.set('library', opts.libraryId);
  return ctx.json<SearchResponse>(`/search?${params.toString()}`);
}

export function scan(ctx: RequestContext): Promise<ScanResult> {
  return ctx.json<ScanResult>('/scan', { method: 'POST' });
}

/** Live scan/enrichment status snapshot. */
export function status(ctx: RequestContext): Promise<Activity> {
  return ctx.json<Activity>('/status');
}

/** URL of the server's recent log lines (text/plain). */
export function logsUrl(ctx: RequestContext, tail = 200): string {
  return `${ctx.baseUrl}/api/logs?tail=${tail}`;
}

/** Fetch the last `tail` lines of the server log as plain text. */
export async function logs(ctx: RequestContext, tail = 200): Promise<string> {
  const res = await ctx.fetchFn(logsUrl(ctx, tail));
  if (!res.ok) throw new LumaApiError(res.status, `GET /logs failed (${res.status})`);
  return res.text();
}

/** Direct-play stream URL for a `<video>` src. Range requests are served by the server. */
export function streamUrl(ctx: RequestContext, id: string): string {
  return `${ctx.baseUrl}/api/items/${encodeURIComponent(id)}/stream`;
}

/** HLS playlist URL for a per-track audio remux. The server always copies the
 * video stream untouched and either stream-copies the selected audio track
 * (`copy`, preserving surround) or re-encodes it to stereo AAC (`copy=false`,
 * for runtimes that can't decode the source codec). `audioIndex` is the
 * audio-relative track index. The default `a0c0` reproduces the legacy
 * audio-transcode fallback. See {@link planAudio}. Needs hls.js outside Safari. */
export function hlsAudioUrl(ctx: RequestContext, id: string, audioIndex = 0, copy = false): string {
  const variant = `a${audioIndex}c${copy ? 1 : 0}`;
  return `${ctx.baseUrl}/api/items/${encodeURIComponent(id)}/hls/${variant}/index.m3u8`;
}

/** HLS *master* playlist that carries the video once plus EVERY audio track as
 * an alternate rendition. The player switches language IN PLACE (no reload, the
 * video never moves) — the stable way to change audio. Video + audio are
 * stream-copied, so the runtime must natively decode them (see
 * {@link canSeamlessAudioSwitch}). Needs hls.js outside Safari/TV. */
export function hlsMasterUrl(ctx: RequestContext, id: string, aac = false, startSec = 0): string {
  // `aac` transcodes every rendition to stereo AAC (for runtimes that can't
  // decode the source codec, e.g. AC3/EAC3 on Chrome); else stream-copy
  // (surround preserved, for TV/Safari). `startSec` (-ss) starts the remux at
  // that position so resume/seek to any offset is instantly available — baked
  // into the path (not a query) so the player's relative segment URLs match the
  // session. The stream's own timeline restarts at 0; callers add startSec back.
  const variant = `master.${aac ? 'aac' : 'copy'}.${Math.max(0, Math.round(startSec * 1000))}`;
  return `${ctx.baseUrl}/api/items/${encodeURIComponent(id)}/hls/${variant}/index.m3u8`;
}

/** Generated SVG poster URL for a movie/episode. */
export function posterUrl(ctx: RequestContext, id: string): string {
  return `${ctx.baseUrl}/api/items/${encodeURIComponent(id)}/poster`;
}

/** Generated SVG poster URL for a show. */
export function showPosterUrl(ctx: RequestContext, id: string): string {
  return `${ctx.baseUrl}/api/shows/${encodeURIComponent(id)}/poster`;
}

/** Resolve a metadata image URL against the server origin. Cached WebP art is
 * stored as a relative path (`/api/images/…`); TMDB fallbacks are absolute. */
export function resolveArt(ctx: RequestContext, url?: string | null): string | null {
  if (!url) return null;
  return /^https?:\/\//.test(url) ? url : `${ctx.baseUrl}${url}`;
}

/** Best poster for a movie/episode: real cached TMDB art if resolved, else the
 * generated SVG placeholder. */
export function posterFor(ctx: RequestContext, x: Pick<MediaItem, 'id' | 'metadata'>): string {
  return resolveArt(ctx, x.metadata?.posterUrl) ?? posterUrl(ctx, x.id);
}

/** Best poster for a show: real cached TMDB art if resolved, else the SVG. */
export function showPosterFor(ctx: RequestContext, x: Pick<Show, 'id' | 'metadata'>): string {
  return resolveArt(ctx, x.metadata?.posterUrl) ?? showPosterUrl(ctx, x.id);
}

/** Cover/backdrop art for a movie or show, or `null` when none was resolved. */
export function backdropFor(ctx: RequestContext, x: { metadata?: Metadata | null }): string | null {
  return resolveArt(ctx, x.metadata?.backdropUrl);
}

/** WebVTT URL for the n-th embedded subtitle track of an item. The server
 * extracts text subtitles on demand (`GET /api/items/:id/subtitles/:n.vtt`). */
export function subtitleUrl(ctx: RequestContext, id: string, index: number): string {
  return `${ctx.baseUrl}/api/items/${encodeURIComponent(id)}/subtitles/${index}.vtt`;
}
