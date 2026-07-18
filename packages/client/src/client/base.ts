// Request/URL/error plumbing shared by every KromaClient domain module. The
// per-domain request implementations live in sibling files (media, accounts,
// playback, library, admin) as thin functions over a {@link RequestContext};
// `KromaClient` (in ../api) is the public facade that wires them together.

export interface KromaClientOptions {
  /** Base server origin, e.g. "http://nas.local:4040". No trailing slash. */
  baseUrl: string;
  fetch?: typeof globalThis.fetch;
  /** Bearer token for per-user endpoints (progress, profile). Optional the
   * catalogue is public. Can be set later with {@link KromaClient.setAuthToken}. */
  authToken?: string;
  /** Active UI locale (`"fr"` | `"en"`), sent as `Accept-Language` so the server
   * localises its responses (admin settings labels, error messages). Change it
   * later with {@link KromaClient.setLocale}. */
  locale?: string;
}

export class KromaApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
    /** Parsed JSON error body when the server sent one (e.g. `{ error,
     * retryAfter }` from a rate-limited PIN verify). */
    readonly body?: unknown,
  ) {
    super(message);
    this.name = 'KromaApiError';
  }
}

/** The human-facing message for a thrown request error: the server's `{ error }`
 * text when present (far more useful than the generic "GET … failed (400)"),
 * otherwise the provided localized `fallback`. */
export function apiErrorText(e: unknown, fallback: string): string {
  if (e instanceof KromaApiError && e.body && typeof e.body === 'object') {
    const msg = (e.body as { error?: unknown }).error;
    if (typeof msg === 'string' && msg.trim()) return msg;
  }
  return fallback;
}

/** The request plumbing a domain module needs: the resolved server origin, the
 * raw fetch (for non-JSON endpoints like logs) and the authed JSON helper. */
export interface RequestContext {
  readonly baseUrl: string;
  readonly fetchFn: typeof globalThis.fetch;
  json<T>(path: string, init?: RequestInit): Promise<T>;
  /** Authed request returning the raw body as a `Blob` (file downloads). */
  blob(path: string, init?: RequestInit): Promise<Blob>;
}

/** Shared request core: attach the auth/locale headers, hit `${baseUrl}/api${path}`,
 * and throw {@link KromaApiError} (with the parsed JSON error body) on a non-2xx
 * response. Returns the raw `Response` so callers read it as JSON or a `Blob`. */
async function sendApiRequest(
  fetchFn: typeof globalThis.fetch,
  baseUrl: string,
  authToken: string | undefined,
  locale: string | undefined,
  path: string,
  init?: RequestInit,
): Promise<Response> {
  const headers = new Headers(init?.headers);
  if (authToken) headers.set('Authorization', `Bearer ${authToken}`);
  if (locale) headers.set('Accept-Language', locale);
  const res = await fetchFn(`${baseUrl}/api${path}`, { ...init, headers });
  if (!res.ok) {
    // Attach the error body (e.g. PIN verify's `{ error, retryAfter }`) so
    // callers can react without a second read.
    const body = await res.json().catch(() => undefined);
    throw new KromaApiError(
      res.status,
      `${init?.method ?? 'GET'} ${path} failed (${res.status})`,
      body,
    );
  }
  return res;
}

/** Authed `GET/POST/…` against `${baseUrl}/api${path}`, parsing the JSON body
 * (or `undefined` on 204). Throws {@link KromaApiError} with the parsed error
 * body on a non-2xx response. */
export async function requestJson<T>(
  fetchFn: typeof globalThis.fetch,
  baseUrl: string,
  authToken: string | undefined,
  locale: string | undefined,
  path: string,
  init?: RequestInit,
): Promise<T> {
  const res = await sendApiRequest(fetchFn, baseUrl, authToken, locale, path, init);
  // 204 No Content (progress writes) → nothing to parse.
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

/** Like {@link requestJson} but returns the raw body as a `Blob` for file
 * downloads (e.g. the admin backup export). Throws {@link KromaApiError} on a
 * non-2xx response, attaching the parsed JSON error body when present. */
export async function requestBlob(
  fetchFn: typeof globalThis.fetch,
  baseUrl: string,
  authToken: string | undefined,
  locale: string | undefined,
  path: string,
  init?: RequestInit,
): Promise<Blob> {
  const res = await sendApiRequest(fetchFn, baseUrl, authToken, locale, path, init);
  return res.blob();
}

export function libraryQuery(libraryId?: string): string {
  return libraryId ? `?library=${encodeURIComponent(libraryId)}` : '';
}

/** Add a `<link rel="preconnect">` to the server origin (no-op off-DOM / if dup). */
export function preconnect(baseUrl: string): void {
  if (typeof document === 'undefined') return;
  try {
    const origin = new URL(baseUrl).origin;
    if (document.querySelector(`link[rel="preconnect"][href="${origin}"]`)) return;
    const link = document.createElement('link');
    link.rel = 'preconnect';
    link.href = origin;
    link.crossOrigin = 'anonymous';
    document.head.appendChild(link);
  } catch {
    /* invalid URL or no DOM ignore */
  }
}
