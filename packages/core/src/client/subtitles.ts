// Online subtitle search + download (provider-agnostic; OpenSubtitles today).

import type { RequestContext } from './base';

const JSON_HEADERS = { 'content-type': 'application/json' };

/** A provider search hit, before download. */
export interface RemoteSub {
  /** Provider-specific id to download (`remote_id` in the download request). */
  id: string;
  provider: string;
  language: string;
  label: string;
  /** Provider popularity count (download_count), for sorting. */
  downloads: number;
}

/** A downloaded + cached subtitle, with its WebVTT URL (relative to the server). */
export interface DownloadedSub {
  id: string;
  language: string | null;
  label: string;
  provider: string;
  /** WebVTT URL, e.g. `/api/items/:id/subtitles/dl/:id.vtt`. */
  url: string;
}

/** Search providers for this title, optionally filtered to `langs` (e.g. `['fr','en']`). */
export function searchSubtitles(ctx: RequestContext, id: string, langs: string[] = []): Promise<RemoteSub[]> {
  const q = langs.length ? `?lang=${encodeURIComponent(langs.join(','))}` : '';
  return ctx.json<RemoteSub[]>(`/items/${encodeURIComponent(id)}/subtitles/search${q}`);
}

/** Download a chosen search hit; the server caches it as WebVTT and records it. */
export function downloadSubtitle(
  ctx: RequestContext,
  id: string,
  hit: { provider: string; remoteId: string; language: string | null; label: string },
): Promise<DownloadedSub> {
  return ctx.json<DownloadedSub>(`/items/${encodeURIComponent(id)}/subtitles/download`, {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({
      provider: hit.provider,
      remote_id: hit.remoteId,
      language: hit.language,
      label: hit.label,
    }),
  });
}

/** This item's already-downloaded / generated online subtitles. */
export function downloadedSubtitles(ctx: RequestContext, id: string): Promise<DownloadedSub[]> {
  return ctx.json<DownloadedSub[]>(`/items/${encodeURIComponent(id)}/subtitles/downloaded`);
}

/** Which subtitle actions the configured providers enable (to hide empty buttons). */
export interface SubCapabilities {
  search: boolean;
  transcribe: boolean;
  translate: boolean;
}

export function subtitleCapabilities(ctx: RequestContext, id: string): Promise<SubCapabilities> {
  return ctx.json<SubCapabilities>(`/items/${encodeURIComponent(id)}/subtitles/capabilities`);
}

/** Generate a subtitle with an AI provider: transcribe the audio (Whisper) or
 * translate `sourceVtt` into `lang` (LLM). Slow; caches + returns the new track. */
export function generateSubtitle(
  ctx: RequestContext,
  id: string,
  req: { providerId?: string; lang: string; sourceVtt?: string; audioTrack?: number },
): Promise<DownloadedSub> {
  return ctx.json<DownloadedSub>(`/items/${encodeURIComponent(id)}/subtitles/generate`, {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(req),
  });
}
