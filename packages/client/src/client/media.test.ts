import { describe, expect, it } from 'vitest';
import type { Metadata } from '../types';
import { KromaApiError, type RequestContext } from './base';
import {
  backdropFor,
  featured,
  hlsMasterUrl,
  items,
  logs,
  logsUrl,
  movies,
  personCredits,
  posterFor,
  posterUrl,
  resolveArt,
  search,
  showPosterUrl,
  storyboard,
  streamUrl,
  subtitleUrl,
  themed,
  themeFor,
} from './media';

// `hlsMasterUrl` (and the other URL builders) only read `ctx.baseUrl`, so a
// minimal stub suffices.
const ctx = { baseUrl: 'http://kroma.test' } as unknown as RequestContext;

// A context that records `ctx.json` calls and drives `ctx.fetchFn` responses.
function recordCtx(resp?: { ok?: boolean; status?: number; json?: unknown; text?: string }) {
  const calls: string[] = [];
  const rich = {
    baseUrl: 'http://kroma.test',
    json: async (path: string) => {
      calls.push(path);
      return [] as never;
    },
    fetchFn: async () =>
      ({
        ok: resp?.ok ?? true,
        status: resp?.status ?? 200,
        json: async () => resp?.json ?? {},
        text: async () => resp?.text ?? '',
      }) as unknown as Response,
  } as unknown as RequestContext;
  return { ctx: rich, calls };
}

describe('hlsMasterUrl', () => {
  it('emits the copy program at anchor 0, audio 0', () => {
    expect(hlsMasterUrl(ctx, 'abc')).toBe(
      'http://kroma.test/api/items/abc/hls/copy/0/0/index.m3u8',
    );
  });

  it('emits the aac program for the AAC variant', () => {
    expect(hlsMasterUrl(ctx, 'abc', true)).toBe(
      'http://kroma.test/api/items/abc/hls/aac/0/0/index.m3u8',
    );
  });

  it('puts the anchor (rounded, clamped) and audio track in the path', () => {
    expect(hlsMasterUrl(ctx, 'abc', false, 600.4, 1)).toBe(
      'http://kroma.test/api/items/abc/hls/copy/600/1/index.m3u8',
    );
    expect(hlsMasterUrl(ctx, 'abc', false, -5, 0)).toBe(
      'http://kroma.test/api/items/abc/hls/copy/0/0/index.m3u8',
    );
  });

  it('url-encodes the item id', () => {
    expect(hlsMasterUrl(ctx, 'a b/c', true, 0, 2)).toBe(
      'http://kroma.test/api/items/a%20b%2Fc/hls/aac/0/2/index.m3u8',
    );
  });

  it('a loudness filter becomes the mode segment (forcing the transcode path)', () => {
    expect(hlsMasterUrl(ctx, 'abc', false, 600.4, 1, 'night')).toBe(
      'http://kroma.test/api/items/abc/hls/aac-night/600/1/index.m3u8',
    );
    // The filter supersedes `aac` (a filtered program is always transcoded).
    expect(hlsMasterUrl(ctx, 'abc', true, 0, 0, 'standard')).toBe(
      'http://kroma.test/api/items/abc/hls/aac-standard/0/0/index.m3u8',
    );
  });
});

describe('stream / poster / subtitle URL builders', () => {
  it('builds encoded stream + poster + subtitle URLs', () => {
    expect(streamUrl(ctx, 'a b')).toBe('http://kroma.test/api/items/a%20b/stream');
    expect(posterUrl(ctx, 'id')).toBe('http://kroma.test/api/items/id/poster');
    expect(showPosterUrl(ctx, 's/1')).toBe('http://kroma.test/api/shows/s%2F1/poster');
    expect(subtitleUrl(ctx, 'id', 3)).toBe('http://kroma.test/api/items/id/subtitles/3.vtt');
  });

  it('logsUrl carries the tail count (default 200)', () => {
    expect(logsUrl(ctx)).toBe('http://kroma.test/api/logs?tail=200');
    expect(logsUrl(ctx, 50)).toBe('http://kroma.test/api/logs?tail=50');
  });
});

describe('resolveArt + art helpers', () => {
  const meta = (m: Partial<Metadata>): { metadata?: Metadata | null } => ({
    metadata: m as Metadata,
  });

  it('resolves relative art against the origin and passes absolute URLs through', () => {
    expect(resolveArt(ctx, '/api/images/x.webp')).toBe('http://kroma.test/api/images/x.webp');
    expect(resolveArt(ctx, 'https://image.tmdb.org/x.jpg')).toBe('https://image.tmdb.org/x.jpg');
    expect(resolveArt(ctx, null)).toBeNull();
    expect(resolveArt(ctx, undefined)).toBeNull();
  });

  it('posterFor uses cached art when present, else the generated poster', () => {
    expect(
      posterFor(ctx, { id: 'i1', metadata: { posterUrl: '/api/images/p.webp' } as Metadata }),
    ).toBe('http://kroma.test/api/images/p.webp');
    expect(posterFor(ctx, { id: 'i2', metadata: null })).toBe(
      'http://kroma.test/api/items/i2/poster',
    );
  });

  it('backdropFor / themeFor resolve or return null', () => {
    expect(backdropFor(ctx, meta({ backdropUrl: '/api/images/b.webp' }))).toBe(
      'http://kroma.test/api/images/b.webp',
    );
    expect(backdropFor(ctx, meta({}))).toBeNull();
    expect(themeFor(ctx, meta({ themeUrl: '/api/themes/1.mp3' }))).toBe(
      'http://kroma.test/api/themes/1.mp3',
    );
    expect(themeFor(ctx, {})).toBeNull();
  });
});

describe('catalogue reads (json delegation)', () => {
  it('appends the library query where supported', () => {
    const { ctx: c, calls } = recordCtx();
    void items(c, 'lib1');
    void movies(c);
    expect(calls).toEqual(['/items?library=lib1', '/movies']);
  });

  it('search builds q + optional limit/library', () => {
    const { ctx: c, calls } = recordCtx();
    void search(c, 'star wars', { limit: 20, libraryId: 'lib1' });
    void search(c, 'plain');
    expect(calls[0]).toBe('/search?q=star+wars&limit=20&library=lib1');
    expect(calls[1]).toBe('/search?q=plain');
  });

  it('personCredits + themed encode their params', () => {
    const { ctx: c, calls } = recordCtx();
    void personCredits(c, 'Ana de Armas', { libraryId: 'lib1' });
    void themed(c, 'christmas movie');
    expect(calls[0]).toBe('/people?name=Ana+de+Armas&library=lib1');
    // themed uses encodeURIComponent (space -> %20), not URLSearchParams (+).
    expect(calls[1]).toBe('/themed?q=christmas%20movie');
  });

  it('featured reads the hero endpoint', () => {
    const { ctx: c, calls } = recordCtx();
    void featured(c);
    expect(calls).toEqual(['/home/featured']);
  });
});

describe('logs (raw text)', () => {
  it('returns the log body on success', async () => {
    const { ctx: c } = recordCtx({ ok: true, text: 'line1\nline2' });
    await expect(logs(c, 10)).resolves.toBe('line1\nline2');
  });

  it('throws KromaApiError on a non-ok response', async () => {
    const { ctx: c } = recordCtx({ ok: false, status: 500 });
    await expect(logs(c)).rejects.toBeInstanceOf(KromaApiError);
  });
});

describe('storyboard', () => {
  it('maps 202 to pending, non-ok to null, and 200 to the manifest', async () => {
    const manifest = {
      url: '/s.jpg',
      interval: 5,
      tileW: 1,
      tileH: 1,
      cols: 1,
      rows: 1,
      count: 1,
      duration: 5,
    };
    await expect(storyboard(recordCtx({ status: 202 }).ctx, 'x')).resolves.toBe('pending');
    await expect(storyboard(recordCtx({ ok: false, status: 404 }).ctx, 'x')).resolves.toBeNull();
    await expect(
      storyboard(recordCtx({ ok: true, status: 200, json: manifest }).ctx, 'x'),
    ).resolves.toEqual(manifest);
  });
});
