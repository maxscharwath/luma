// KROMA API origin resolution.
//
// Production (the single-binary Synology package): the Rust server serves THIS
// SPA *and* the API on the same origin, so we call the page's own origin and
// there's nothing to configure. Dev: the web (vite :3000) and API (:4040) are
// separate origins, so a build-time `VITE_KROMA_SERVER` points at the API.
// `window.__KROMA_API__` (if injected) still wins, for embedding flexibility.
import {
  isTextSubtitle,
  KromaClient,
  loadSession,
  type MediaItem,
  type Show,
  sessionToken,
  setSessionToken,
  sharedTokenExchange,
} from '@kroma/core';

declare global {
  interface Window {
    __KROMA_API__?: string;
  }
}

const DEFAULT_BASE = 'http://localhost:4040';

/** The KROMA server origin (no trailing slash). */
export function apiBase(): string {
  // 1) Explicit runtime override (rare).
  if (typeof window !== 'undefined' && window.__KROMA_API__) {
    return window.__KROMA_API__.replace(/\/+$/, '');
  }
  // 2) Build-time override set in dev/staging to point at a specific API.
  const envBase = import.meta.env?.VITE_KROMA_SERVER;
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
  const env = typeof process !== 'undefined' ? process.env.KROMA_SERVER_URL : undefined;
  return (env ?? DEFAULT_BASE).replace(/\/+$/, '');
}

/** Whether an account is active on this device (has a stored access token).
 * Route loaders use this to skip fetching the (now auth-gated) catalogue before
 * sign-in the router re-runs them once logged in (see the root invalidator). */
export function isAuthed(): boolean {
  return loadSession() != null;
}

// Exchange the stored access token for a fresh in-memory session bearer. Shared
// app-wide via `sharedTokenExchange`, so a reload's boot exchange (auth provider),
// this refresh, and any concurrent 401s coalesce into ONE POST /auth/token.
function exchangeStoredSession(): Promise<string | undefined> {
  const active = loadSession();
  if (!active) return Promise.resolve(undefined);
  return sharedTokenExchange(() =>
    new KromaClient({ baseUrl: apiBase() }).exchangeToken(active.accessToken),
  )
    .then((res) => {
      setSessionToken(res.token);
      return res.token as string | undefined;
    })
    .catch(() => {
      setSessionToken(undefined);
      return undefined;
    });
}

// Silent refresh shared by every ad-hoc `kromaClient()`: on a 401 they exchange
// the stored access token for a fresh bearer.
function refreshSession(): Promise<string | undefined> {
  return exchangeStoredSession();
}

/** Ensure an in-memory session bearer exists, running the boot token exchange if
 * it hasn't happened yet. Route loaders `await` this so their first authed
 * request carries a bearer instead of racing the boot exchange and 401-then-
 * retrying on every reload. A no-op once a bearer is in memory or when signed
 * out (no stored session to exchange). */
export function ensureSession(): Promise<void> {
  if (sessionToken()) return Promise.resolve();
  return exchangeStoredSession().then(() => undefined);
}

export function kromaClient(): KromaClient {
  // The bearer is the in-memory session token (never persisted); a 401 refreshes
  // it from the stored access token. `sessionToken()` is null during the shell
  // prerender / before the boot exchange, which is fine those requests either
  // hit public endpoints or trigger a refresh.
  const c = new KromaClient({ baseUrl: apiBase(), authToken: sessionToken() });
  c.setRefreshHandler(refreshSession);
  return c;
}

/** Resolve a metadata image path (relative `/api/…` cached art, or an absolute
 * URL) against the KROMA origin. Works on both server and client. */
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

/** A movie/episode with art + stream + subtitle URLs pre-resolved to absolute KROMA URLs. */
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

export function toMovieView(c: KromaClient, item: MediaItem): MovieView {
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

export function toShowView(c: KromaClient, show: Show): ShowView {
  return { ...show, poster: c.showPosterFor(show), backdrop: c.backdropFor(show) };
}
