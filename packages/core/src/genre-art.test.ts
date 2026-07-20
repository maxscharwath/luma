import type { Metadata } from '@kroma/client';
import { describe, expect, it } from 'vitest';
import type { Sortable } from './browse';
import { hueFromString } from './format';
import { genreAccent, genreColors, genreHue, genreShowcases, genreTint } from './genre-art';

function title(p: {
  title: string;
  rating?: number | null;
  genres?: string[];
  backdropUrl?: string;
}): Sortable {
  const { title, rating = null, genres = [], backdropUrl } = p;
  return {
    title,
    year: null,
    addedAt: '2020-01-01T00:00:00Z',
    metadata: { rating, genres, backdropUrl } as unknown as Metadata,
  };
}

describe('genreShowcases', () => {
  it('picks the best-rated backdropped title per genre (trim-tolerant)', () => {
    const items = [
      title({ title: 'no-art', rating: 9.9, genres: ['Drame'] }),
      title({
        title: 'good',
        rating: 8,
        genres: ['Drame', 'Crime'],
        backdropUrl: '/api/images/a.webp',
      }),
      title({ title: 'better', rating: 9, genres: [' Drame '], backdropUrl: '/api/images/b.webp' }),
    ];
    const picks = genreShowcases(items);
    expect(picks.get('Drame')?.title).toBe('better');
    expect(picks.get('Crime')?.title).toBe('good');
  });

  it('has no pick for a genre whose titles all lack backdrops', () => {
    const picks = genreShowcases([title({ title: 'x', rating: 10, genres: ['Western'] })]);
    expect(picks.get('Western')).toBeUndefined();
  });

  it('ignores blank genre names', () => {
    const picks = genreShowcases([
      title({ title: 'x', genres: ['  '], backdropUrl: '/api/images/a.webp' }),
    ]);
    expect(picks.size).toBe(0);
  });

  it('fronts each genre with a different title when possible', () => {
    const items = [
      title({
        title: 'hit',
        rating: 9,
        genres: ['Drame', 'Guerre'],
        backdropUrl: '/api/images/a.webp',
      }),
      title({ title: 'other', rating: 5, genres: ['Drame'], backdropUrl: '/api/images/b.webp' }),
    ];
    const picks = genreShowcases(items);
    // Guerre (scarcer) claims the shared hit; Drame falls back to its unused title.
    expect(picks.get('Guerre')?.title).toBe('hit');
    expect(picks.get('Drame')?.title).toBe('other');
  });

  it('reuses a title rather than leaving a genre bare', () => {
    const only = title({
      title: 'only',
      genres: ['Drame', 'Guerre'],
      backdropUrl: '/api/images/a.webp',
    });
    const picks = genreShowcases([only]);
    expect(picks.get('Drame')?.title).toBe('only');
    expect(picks.get('Guerre')?.title).toBe('only');
  });
});

describe('genreHue / genreColors', () => {
  it('gives both localizations of a genre the same curated hue', () => {
    expect(genreHue('Drame')).toBe(genreHue('Drama'));
    expect(genreColors('Comédie')).toEqual(genreColors('comedy'));
  });

  it('keeps neighbouring common genres visually distinct', () => {
    const hues = ['Action', 'Comédie', 'Drame', 'Horreur', 'Science-Fiction'].map(genreHue);
    expect(new Set(hues).size).toBe(hues.length);
  });

  it('hashes unknown genres deterministically into legacy-safe hsl()', () => {
    expect(genreHue('K-Drama')).toBe(genreHue('K-Drama'));
    expect(genreHue('K-Drama')).toBeLessThan(360);
    expect(genreColors('K-Drama')[0]).toMatch(/^hsl\(\d+, \d+%, \d+%\)$/);
  });

  it('hashes with the shared string hue (trimmed + lowercased)', () => {
    expect(genreHue(' K-Drama ')).toBe(hueFromString('k-drama'));
  });

  it('builds the accent and tint from the genre hue', () => {
    expect(genreAccent('Drame')).toBe(`hsl(${genreHue('Drame')}, 82%, 62%)`);
    expect(genreTint('Drame')).toContain(`hsla(${genreHue('Drame')}, `);
    expect(genreTint('Drame')).toContain('linear-gradient(to top');
  });
});
