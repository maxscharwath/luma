import { afterEach, describe, expect, it, vi } from 'vitest';
import { getWebEnginePref, setWebEnginePref } from './engine-pref';

/** A Map-backed localStorage stand-in. */
function fakeStorage(initial: Record<string, string> = {}) {
  const m = new Map(Object.entries(initial));
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    _map: m,
  };
}

afterEach(() => vi.unstubAllGlobals());

describe('getWebEnginePref / setWebEnginePref', () => {
  it('defaults to auto when nothing is stored', () => {
    vi.stubGlobal('localStorage', fakeStorage());
    expect(getWebEnginePref()).toBe('auto');
  });

  it('round-trips a saved preference', () => {
    const store = fakeStorage();
    vi.stubGlobal('localStorage', store);
    setWebEnginePref('remux');
    expect(store._map.get('kroma:web-engine')).toBe('remux');
    expect(getWebEnginePref()).toBe('remux');
    setWebEnginePref('direct');
    expect(getWebEnginePref()).toBe('direct');
  });

  it('ignores an unknown stored value', () => {
    vi.stubGlobal('localStorage', fakeStorage({ 'kroma:web-engine': 'bogus' }));
    expect(getWebEnginePref()).toBe('auto');
  });

  it('is safe when storage throws (private mode / disabled)', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('denied');
      },
      setItem: () => {
        throw new Error('denied');
      },
    });
    expect(getWebEnginePref()).toBe('auto');
    expect(() => setWebEnginePref('remux')).not.toThrow();
  });
});
