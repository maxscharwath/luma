import { KromaClient } from '@kroma/core';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { apiBase, imageUrl, toMovieView, toShowView } from './api';

afterEach(() => vi.unstubAllGlobals());

describe('apiBase', () => {
  it('returns a non-empty origin with no trailing slash', () => {
    const base = apiBase();
    expect(base.length).toBeGreaterThan(0);
    expect(base.endsWith('/')).toBe(false);
  });

  it('prefers window.__KROMA_API__ and strips its trailing slashes', () => {
    vi.stubGlobal('window', { __KROMA_API__: 'http://custom:9000///' });
    expect(apiBase()).toBe('http://custom:9000');
  });
});

describe('imageUrl', () => {
  it('returns null for a missing url', () => {
    expect(imageUrl(null)).toBeNull();
    expect(imageUrl(undefined)).toBeNull();
    expect(imageUrl('')).toBeNull();
  });

  it('passes absolute urls through untouched', () => {
    expect(imageUrl('https://image.tmdb.org/x.jpg')).toBe('https://image.tmdb.org/x.jpg');
    expect(imageUrl('http://x/y')).toBe('http://x/y');
  });

  it('resolves a relative path against the api base', () => {
    expect(imageUrl('/api/images/x.webp')).toBe(`${apiBase()}/api/images/x.webp`);
  });
});

describe('toMovieView / toShowView', () => {
  const client = new KromaClient({ baseUrl: 'http://kroma.test' });

  it('resolves art + stream + subtitle urls, gating image subs to null', () => {
    const item = {
      id: 'i1',
      subtitles: [
        { language: 'en', codec: 'subrip' },
        { language: 'fr', codec: 'hdmv_pgs_subtitle' },
      ],
      metadata: { posterUrl: '/api/p.webp', backdropUrl: null },
      video: null,
      // biome-ignore lint/suspicious/noExplicitAny: minimal MediaItem fixture
    } as any;
    const v = toMovieView(client, item);
    expect(v.poster).toBe('http://kroma.test/api/p.webp');
    expect(v.stream).toBe('http://kroma.test/api/items/i1/stream');
    expect(v.backdrop).toBeNull();
    // Text sub -> a WebVTT url; PGS image sub -> null.
    expect(v.subs[0]?.url).toBe('http://kroma.test/api/items/i1/subtitles/0.vtt');
    expect(v.subs[1]?.url).toBeNull();
    expect(v.subs[1]?.language).toBe('fr');
  });

  it('toShowView resolves show art', () => {
    const show = {
      id: 's1',
      metadata: { posterUrl: '/api/sp.webp', backdropUrl: '/api/sb.webp' },
      // biome-ignore lint/suspicious/noExplicitAny: minimal Show fixture
    } as any;
    const sv = toShowView(client, show);
    expect(sv.poster).toBe('http://kroma.test/api/sp.webp');
    expect(sv.backdrop).toBe('http://kroma.test/api/sb.webp');
  });
});
