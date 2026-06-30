// LUMA API origin resolution.
//
// Production (the single-binary Synology package): the Rust server serves THIS
// SPA *and* the API on the same origin, so we call the page's own origin and
// there's nothing to configure. Dev: the web (vite :3000) and API (:4040) are
// separate origins, so a build-time `VITE_LUMA_SERVER` points at the API.
// `window.__LUMA_API__` (if injected) still wins, for embedding flexibility.
import { isTextSubtitle, loadSession, LumaClient, type MediaItem, type Show } from '@luma/core';

declare global {
  interface Window {
    __LUMA_API__?: string;
  }
}

const DEFAULT_BASE = 'http://localhost:4040';

/** The LUMA server origin (no trailing slash). */
export function apiBase(): string {
  // 1) Explicit runtime override (rare).
  if (typeof window !== 'undefined' && window.__LUMA_API__) {
    return window.__LUMA_API__.replace(/\/+$/, '');
  }
  // 2) Build-time override set in dev/staging to point at a specific API.
  const envBase = import.meta.env?.VITE_LUMA_SERVER;
  if (envBase) return envBase.replace(/\/+$/, '');
  // 3) Dev (vite): same-origin the Vite dev server reverse-proxies `/api`
  //    (incl. the events WebSocket) to the Rust server, so the whole app lives
  //    on one port (`:3000`). Just call the page origin, like production. SSR /
  //    prerender (no window) falls back to the conventional local API.
  if (import.meta.env?.DEV) {
    return typeof window !== 'undefined'
      ? window.location.origin.replace(/\/+$/, '')
      : DEFAULT_BASE;
  }
  // 4) Production SPA: same origin as the page (the Rust server serves both).
  if (typeof window !== 'undefined') return window.location.origin.replace(/\/+$/, '');
  // 5) SSR / prerender fallback.
  const env = typeof process !== 'undefined' ? process.env.LUMA_SERVER_URL : undefined;
  return (env ?? DEFAULT_BASE).replace(/\/+$/, '');
}

export function lumaClient(): LumaClient {
  // Carry the active session token (if any) so route loaders which now run on
  // the client (SPA, no SSR) get per-user personalised catalogue DTOs, e.g. the
  // per-show progress on cards. `loadSession()` is storage-guarded (null on the
  // server), so this stays safe during the shell prerender.
  return new LumaClient({ baseUrl: apiBase(), authToken: loadSession()?.token });
}

/** Resolve a metadata image path (relative `/api/…` cached art, or an absolute
 * URL) against the LUMA origin. Works on both server and client. */
export function imageUrl(url: string | null | undefined): string | null {
  if (!url) return null;
  return /^https?:\/\//.test(url) ? url : `${apiBase()}${url}`;
}

/** A subtitle track with its on-demand WebVTT URL (text subs only). */
export interface SubtitleView {
  index: number;
  language: string | null;
  codec: string;
  /** Text-based subs (subrip/ass/mov_text) can be served as WebVTT; image subs
   * (PGS/VobSub) cannot `url` is null then. */
  url: string | null;
}

/** A movie/episode with art + stream + subtitle URLs pre-resolved to absolute LUMA URLs. */
export interface MovieView extends MediaItem {
  poster: string;
  backdrop: string | null;
  stream: string;
  /** HLS URL that copies the video and re-encodes audio to stereo AAC, for
   * browsers that can't decode the source audio codec (AC3/EAC3/DTS/TrueHD). */
  hlsAudio: string;
  subs: SubtitleView[];
}

/** A show with art pre-resolved. */
export interface ShowView extends Show {
  poster: string;
  backdrop: string | null;
}

export function toMovieView(c: LumaClient, item: MediaItem): MovieView {
  const subs: SubtitleView[] = item.subtitles.map((s, index) => ({
    index,
    language: s.language,
    codec: s.codec,
    url: isTextSubtitle(s.codec) ? c.subtitleUrl(item.id, index) : null,
  }));
  return {
    ...item,
    poster: c.posterFor(item),
    backdrop: c.backdropFor(item),
    stream: c.streamUrl(item.id),
    hlsAudio: c.hlsAudioUrl(item.id),
    subs,
  };
}

export function toShowView(c: LumaClient, show: Show): ShowView {
  return { ...show, poster: c.showPosterFor(show), backdrop: c.backdropFor(show) };
}
