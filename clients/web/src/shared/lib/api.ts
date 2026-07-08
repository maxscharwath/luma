// LUMA API origin resolution.
//
// Production (the single-binary Synology package): the Rust server serves THIS
// SPA *and* the API on the same origin, so we call the page's own origin and
// there's nothing to configure. Dev: the web (vite :3000) and API (:4040) are
// separate origins, so a build-time `VITE_LUMA_SERVER` points at the API.
// `window.__LUMA_API__` (if injected) still wins, for embedding flexibility.
import {
  isTextSubtitle,
  LumaClient,
  loadSession,
  type MediaItem,
  type Show,
  sessionToken,
  setSessionToken,
} from '@luma/core';

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

/** Whether an account is active on this device (has a stored access token).
 * Route loaders use this to skip fetching the (now auth-gated) catalogue before
 * sign-in the router re-runs them once logged in (see the root invalidator). */
export function isAuthed(): boolean {
  return loadSession() != null;
}

// Silent refresh shared by every ad-hoc `lumaClient()`: on a 401 they exchange
// the stored access token for a fresh in-memory session token. Deduped so a burst
// of 401s (a page full of cards) triggers a single exchange.
let refreshInFlight: Promise<string | undefined> | null = null;
function refreshSession(): Promise<string | undefined> {
  if (refreshInFlight) return refreshInFlight;
  const active = loadSession();
  if (!active) return Promise.resolve(undefined);
  const c = new LumaClient({ baseUrl: apiBase() });
  refreshInFlight = c
    .exchangeToken(active.accessToken)
    .then((res) => {
      setSessionToken(res.token);
      return res.token as string | undefined;
    })
    .catch(() => {
      setSessionToken(undefined);
      return undefined;
    })
    .finally(() => {
      refreshInFlight = null;
    });
  return refreshInFlight;
}

export function lumaClient(): LumaClient {
  // The bearer is the in-memory session token (never persisted); a 401 refreshes
  // it from the stored access token. `sessionToken()` is null during the shell
  // prerender / before the boot exchange, which is fine those requests either
  // hit public endpoints or trigger a refresh.
  const c = new LumaClient({ baseUrl: apiBase(), authToken: sessionToken() });
  c.setRefreshHandler(refreshSession);
  return c;
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
  /** True for a generated subtitle (Whisper/translate), vs embedded. */
  downloaded?: boolean;
  /** Display label for a generated sub. */
  label?: string;
  /** The generated subtitle's id (for deletion); absent for embedded tracks. */
  subId?: string;
  /** Provider tag of a generated sub (`whisper`/`translate`), for the "IA" badge. */
  provider?: string;
}

/** A movie/episode with art + stream + subtitle URLs pre-resolved to absolute LUMA URLs. */
export interface MovieView extends MediaItem {
  poster: string;
  backdrop: string | null;
  stream: string;
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
    subs,
  };
}

export function toShowView(c: LumaClient, show: Show): ShowView {
  return { ...show, poster: c.showPosterFor(show), backdrop: c.backdropFor(show) };
}
