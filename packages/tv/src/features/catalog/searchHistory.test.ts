import { afterEach, describe, expect, it, vi } from 'vitest';
import { addRecentSearch, getRecentSearches } from './searchHistory';

/** A Map-backed localStorage stand-in. */
function fakeStorage(initial: Record<string, string> = {}) {
  const m = new Map(Object.entries(initial));
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    _map: m,
  };
}

const KEY = 'kroma:recent-searches';

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('getRecentSearches', () => {
  it('returns an empty list when nothing is stored', () => {
    vi.stubGlobal('localStorage', fakeStorage());
    expect(getRecentSearches()).toEqual([]);
  });

  it('returns the stored list', () => {
    vi.stubGlobal('localStorage', fakeStorage({ [KEY]: '["dune","alien"]' }));
    expect(getRecentSearches()).toEqual(['dune', 'alien']);
  });

  it('drops non-string entries and survives corrupt JSON', () => {
    vi.stubGlobal('localStorage', fakeStorage({ [KEY]: '["ok",42,null,""]' }));
    expect(getRecentSearches()).toEqual(['ok']);
    vi.stubGlobal('localStorage', fakeStorage({ [KEY]: 'not json {' }));
    expect(getRecentSearches()).toEqual([]);
  });

  it('swallows storage errors', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('blocked');
      },
    });
    expect(getRecentSearches()).toEqual([]);
  });
});

describe('addRecentSearch', () => {
  it('puts the newest query first and persists', () => {
    const store = fakeStorage({ [KEY]: '["alien"]' });
    vi.stubGlobal('localStorage', store);
    expect(addRecentSearch('dune')).toEqual(['dune', 'alien']);
    expect(store._map.get(KEY)).toBe('["dune","alien"]');
  });

  it('dedupes case-insensitively, moving the query to the front', () => {
    vi.stubGlobal('localStorage', fakeStorage({ [KEY]: '["dune","alien"]' }));
    expect(addRecentSearch('ALIEN')).toEqual(['ALIEN', 'dune']);
  });

  it('trims and ignores blank queries', () => {
    vi.stubGlobal('localStorage', fakeStorage({ [KEY]: '["dune"]' }));
    expect(addRecentSearch('   ')).toEqual(['dune']);
    expect(addRecentSearch(' alien ')).toEqual(['alien', 'dune']);
  });

  it('caps the list at eight entries', () => {
    const nine = JSON.stringify(['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h']);
    vi.stubGlobal('localStorage', fakeStorage({ [KEY]: nine }));
    const next = addRecentSearch('i');
    expect(next).toHaveLength(8);
    expect(next[0]).toBe('i');
    expect(next).not.toContain('h');
  });

  it('swallows storage write errors and still returns the list', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => null,
      setItem: () => {
        throw new Error('blocked');
      },
    });
    expect(addRecentSearch('dune')).toEqual(['dune']);
  });
});
