// Catalogue reads, art/stream URL builders and subtitles.

import type {
  Activity,
  Health,
  Library,
  MediaItem,
  Metadata,
  PersonResponse,
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
 * added) already localized + de-duplicated. Clients render it generically. */
export function home(ctx: RequestContext): Promise<Section[]> {
  return ctx.json<Section[]>('/home');
}

/** AI-curated suggestions for one title's detail page ("Suggestions IA"), Bearer.
 * Generated lazily by the LLM connector and cached server-side, so the first call
 * for a title returns `null` (generating) poll until a {@link Section} arrives
 * (its `items` may be empty when the model found nothing worth showing). */
export function aiSuggest(ctx: RequestContext, id: string): Promise<Section | null> {
  return ctx.json<Section | null>(`/items/${encodeURIComponent(id)}/ai-suggest`);
}

/** Full-text catalogue search (movies, shows, episodes). Server-side
 * field-weighted, typo-tolerant ranking well suited to imperfect voice
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

/** Every movie + show one person is credited in (cast or key crew). Server-side
 * exact (case-insensitive) match over the catalogue metadata distinct from the
 * fuzzy {@link search} ordered best-known work first. */
export function personCredits(
  ctx: RequestContext,
  name: string,
  opts?: { libraryId?: string },
): Promise<PersonResponse> {
  const params = new URLSearchParams({ name });
  if (opts?.libraryId) params.set('library', opts.libraryId);
  return ctx.json<PersonResponse>(`/people?${params.toString()}`);
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

/** HLS *master* playlist for one continuous remux: the video once plus EVERY
 * audio track as an alternate rendition (one per `item.audioTracks` entry, so
 * rendition T maps to audio-relative index T), so language switches happen IN
 * PLACE (no reload, the picture never moves). `startSec` (input `-ss`) anchors
 * the remux at a resume / far seek so it is available in ~1s even over a network
 * mount; hls.js reports time relative to that anchor, so the client adds it back
 * for the absolute position. `aac=true` transcodes every rendition to stereo AAC
 * for runtimes that can't decode the source codec via MSE (AC3/EAC3/DTS on
 * Chrome/webOS); `aac=false` stream-copies them (surround preserved, for
 * native-decode clients). Needs hls.js outside Safari/TV. */
export function hlsMasterUrl(ctx: RequestContext, id: string, aac = false, startSec = 0, audio = 0): string {
  // One muxed program per (item, mode, ANCHOR, AUDIO). The anchor (input `-ss`)
  // and the audio-relative track index are both in the PATH, so each seek
  // position and each language gets its own session with its own child URLs - no
  // collision, no stale-cache replay. The chosen audio is MUXED into the stream
  // (hls.js alternate-audio switching was unreliable), so language switch reloads
  // with a different `audio`. hls.js reports time relative to the anchor; the
  // client adds it back via the X-Hls-Start header.
  const anchor = Math.max(0, Math.round(startSec));
  const a = Math.max(0, Math.round(audio));
  return `${ctx.baseUrl}/api/items/${encodeURIComponent(id)}/hls/${aac ? 'aac' : 'copy'}/${anchor}/${a}/index.m3u8`;
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

/** Plex-style theme song for a movie or show, or `null` when none was resolved.
 * Only TV shows carry one (a cached `/api/themes/<tvdb>.mp3`); movies are null. */
export function themeFor(ctx: RequestContext, x: { metadata?: Metadata | null }): string | null {
  return resolveArt(ctx, x.metadata?.themeUrl);
}

/** WebVTT URL for the n-th embedded subtitle track of an item. The server
 * extracts text subtitles on demand (`GET /api/items/:id/subtitles/:n.vtt`). */
export function subtitleUrl(ctx: RequestContext, id: string, index: number): string {
  return `${ctx.baseUrl}/api/items/${encodeURIComponent(id)}/subtitles/${index}.vtt`;
}
