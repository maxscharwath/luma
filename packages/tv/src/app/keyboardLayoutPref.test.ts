import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  ALL_KEYBOARD_LAYOUTS,
  getKeyboardLayoutPref,
  KEYBOARD_LAYOUT_LABEL_KEY,
  setKeyboardLayoutPref,
} from './keyboardLayoutPref';

/** A Map-backed localStorage stand-in. */
function fakeStorage(initial: Record<string, string> = {}) {
  const m = new Map(Object.entries(initial));
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    _map: m,
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('getKeyboardLayoutPref / setKeyboardLayoutPref', () => {
  it('defaults to abc when nothing is stored', () => {
    vi.stubGlobal('localStorage', fakeStorage());
    expect(getKeyboardLayoutPref()).toBe('abc');
  });

  it('returns a stored valid preference', () => {
    vi.stubGlobal('localStorage', fakeStorage({ 'kroma:kbd-layout': 'azerty' }));
    expect(getKeyboardLayoutPref()).toBe('azerty');
  });

  it('ignores an unknown stored value', () => {
    vi.stubGlobal('localStorage', fakeStorage({ 'kroma:kbd-layout': 'dvorak' }));
    expect(getKeyboardLayoutPref()).toBe('abc');
  });

  it('persists the preference', () => {
    const store = fakeStorage();
    vi.stubGlobal('localStorage', store);
    setKeyboardLayoutPref('qwertz');
    expect(store._map.get('kroma:kbd-layout')).toBe('qwertz');
  });

  it('swallows storage errors on read and write', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('blocked');
      },
    });
    expect(getKeyboardLayoutPref()).toBe('abc');
    expect(() => setKeyboardLayoutPref('qwerty')).not.toThrow();
  });
});

describe('KEYBOARD_LAYOUT_LABEL_KEY', () => {
  it('maps every layout to its i18n label key', () => {
    for (const l of ALL_KEYBOARD_LAYOUTS) {
      expect(KEYBOARD_LAYOUT_LABEL_KEY[l]).toBe(`keyboardLayout.${l}`);
    }
  });
});
