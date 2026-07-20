import { describe, expect, it, vi } from 'vitest';
import {
  apiErrorText,
  KromaApiError,
  libraryQuery,
  preconnect,
  requestBlob,
  requestJson,
} from './base';

// A fetch stub whose single response is configured per call.
function stubFetch(resp: {
  ok?: boolean;
  status?: number;
  json?: () => unknown;
  /** Raw body for the text() path. Defaults to the JSON stringified, so an
   * omitted `text` mirrors a normal JSON response; pass '' for an empty body. */
  text?: () => string;
  blob?: () => unknown;
}): { fetch: typeof globalThis.fetch; calls: { url: string; init?: RequestInit }[] } {
  const calls: { url: string; init?: RequestInit }[] = [];
  const fetch = vi.fn(async (url: RequestInfo | URL, init?: RequestInit) => {
    calls.push({ url: String(url), init });
    return {
      ok: resp.ok ?? true,
      status: resp.status ?? 200,
      json: async () => (resp.json ? resp.json() : {}),
      // requestJson now reads the body via text(); mirror the JSON body unless
      // the test overrides it (e.g. an empty 202 ack).
      text: async () => (resp.text ? resp.text() : JSON.stringify(resp.json ? resp.json() : {})),
      blob: async () => (resp.blob ? resp.blob() : new Blob()),
    } as unknown as Response;
  }) as unknown as typeof globalThis.fetch;
  return { fetch, calls };
}

describe('libraryQuery', () => {
  it('builds an encoded query, or empty when absent', () => {
    expect(libraryQuery('lib 1')).toBe('?library=lib%201');
    expect(libraryQuery()).toBe('');
    expect(libraryQuery(undefined)).toBe('');
  });
});

describe('KromaApiError', () => {
  it('carries status, message, body and name', () => {
    const e = new KromaApiError(429, 'rate limited', { error: 'slow down', retryAfter: 5 });
    expect(e).toBeInstanceOf(Error);
    expect(e.status).toBe(429);
    expect(e.message).toBe('rate limited');
    expect(e.name).toBe('KromaApiError');
    expect(e.body).toEqual({ error: 'slow down', retryAfter: 5 });
  });
});

describe('apiErrorText', () => {
  it('prefers the server error body text', () => {
    const e = new KromaApiError(400, 'GET /x failed (400)', { error: 'PIN incorrect' });
    expect(apiErrorText(e, 'fallback')).toBe('PIN incorrect');
  });

  it('uses the fallback for a blank / missing / non-KromaApiError error', () => {
    expect(apiErrorText(new KromaApiError(400, 'm', { error: '   ' }), 'fb')).toBe('fb');
    expect(apiErrorText(new KromaApiError(400, 'm', {}), 'fb')).toBe('fb');
    expect(apiErrorText(new Error('boom'), 'fb')).toBe('fb');
    expect(apiErrorText('nope', 'fb')).toBe('fb');
  });
});

describe('requestJson', () => {
  it('hits ${baseUrl}/api${path} with auth + locale headers and parses JSON', async () => {
    const { fetch, calls } = stubFetch({ json: () => ({ ok: true, n: 1 }) });
    const out = await requestJson(fetch, 'http://nas:4040', 'tok', 'fr', '/items');
    expect(out).toEqual({ ok: true, n: 1 });
    expect(calls[0]?.url).toBe('http://nas:4040/api/items');
    const headers = calls[0]?.init?.headers as Headers;
    expect(headers.get('Authorization')).toBe('Bearer tok');
    expect(headers.get('Accept-Language')).toBe('fr');
  });

  it('sends no auth/locale headers when they are absent', async () => {
    const { fetch, calls } = stubFetch({ json: () => ({}) });
    await requestJson(fetch, 'http://nas', undefined, undefined, '/health');
    const headers = calls[0]?.init?.headers as Headers;
    expect(headers.get('Authorization')).toBeNull();
    expect(headers.get('Accept-Language')).toBeNull();
  });

  it('returns undefined for a 204 No Content', async () => {
    const { fetch } = stubFetch({ status: 204 });
    await expect(
      requestJson(fetch, 'http://nas', 't', 'en', '/progress/x'),
    ).resolves.toBeUndefined();
  });

  it('returns undefined for a 202 Accepted with an empty body (no JSON parse throw)', async () => {
    // Regression: the rematch apply returns 202 with an empty body; calling
    // res.json() on it threw and turned the success into a failure toast.
    const { fetch } = stubFetch({ status: 202, text: () => '' });
    await expect(
      requestJson(fetch, 'http://nas', 't', 'en', '/rematch/movie/x'),
    ).resolves.toBeUndefined();
  });

  it('returns undefined for an empty 200 body rather than throwing', async () => {
    const { fetch } = stubFetch({ status: 200, text: () => '' });
    await expect(requestJson(fetch, 'http://nas', 't', 'en', '/x')).resolves.toBeUndefined();
  });

  it('throws KromaApiError with the parsed error body on a non-2xx', async () => {
    const { fetch } = stubFetch({ ok: false, status: 400, json: () => ({ error: 'bad' }) });
    await expect(
      requestJson(fetch, 'http://nas', 't', 'en', '/x', { method: 'POST' }),
    ).rejects.toMatchObject({ status: 400, body: { error: 'bad' }, name: 'KromaApiError' });
  });

  it('tolerates an unparseable error body (body undefined)', async () => {
    const { fetch } = stubFetch({
      ok: false,
      status: 500,
      json: () => {
        throw new Error('not json');
      },
    });
    await expect(
      requestJson(fetch, 'http://nas', undefined, undefined, '/x'),
    ).rejects.toMatchObject({
      status: 500,
      body: undefined,
    });
  });
});

describe('requestBlob', () => {
  it('returns the raw body as a Blob', async () => {
    const payload = new Blob(['data']);
    const { fetch } = stubFetch({ blob: () => payload });
    await expect(requestBlob(fetch, 'http://nas', 't', 'en', '/backup')).resolves.toBe(payload);
  });

  it('throws KromaApiError on a non-2xx', async () => {
    const { fetch } = stubFetch({ ok: false, status: 403, json: () => ({ error: 'nope' }) });
    await expect(requestBlob(fetch, 'http://nas', 't', 'en', '/backup')).rejects.toBeInstanceOf(
      KromaApiError,
    );
  });
});

describe('preconnect', () => {
  it('is a no-op without a DOM', () => {
    expect(() => preconnect('http://nas:4040')).not.toThrow();
  });
});
