import { afterEach, describe, expect, it, vi } from 'vitest';
import { ALL_KEYBOARD_LAYOUTS, KEYBOARD_LAYOUT_LABEL_KEY } from './keyboardLayoutPref';

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
  // The pref is a reactive store (settings/store.ts) that reads storage ONCE at
  // module creation and is the in-process source of truth afterwards, so each
  // test imports a fresh module instance with its storage stub already up.
  async function fresh(storage: unknown) {
    vi.resetModules();
    vi.stubGlobal('localStorage', storage);
    return import('./keyboardLayoutPref');
  }

  it('defaults to abc when nothing is stored', async () => {
    const m = await fresh(fakeStorage());
    expect(m.getKeyboardLayoutPref()).toBe('abc');
  });

  it('returns a stored valid preference', async () => {
    const m = await fresh(fakeStorage({ 'kroma:kbd-layout': 'azerty' }));
    expect(m.getKeyboardLayoutPref()).toBe('azerty');
  });

  it('ignores an unknown stored value', async () => {
    const m = await fresh(fakeStorage({ 'kroma:kbd-layout': 'dvorak' }));
    expect(m.getKeyboardLayoutPref()).toBe('abc');
  });

  it('persists the preference', async () => {
    const store = fakeStorage();
    const m = await fresh(store);
    m.setKeyboardLayoutPref('qwertz');
    expect(store._map.get('kroma:kbd-layout')).toBe('qwertz');
    expect(m.getKeyboardLayoutPref()).toBe('qwertz');
  });

  it('swallows storage errors on read and write', async () => {
    const m = await fresh({
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('blocked');
      },
    });
    expect(m.getKeyboardLayoutPref()).toBe('abc');
    expect(() => m.setKeyboardLayoutPref('qwerty')).not.toThrow();
    // The in-process value holds even when the storage write failed.
    expect(m.getKeyboardLayoutPref()).toBe('qwerty');
  });
});

describe('KEYBOARD_LAYOUT_LABEL_KEY', () => {
  it('maps every layout to its i18n label key', () => {
    for (const l of ALL_KEYBOARD_LAYOUTS) {
      expect(KEYBOARD_LAYOUT_LABEL_KEY[l]).toBe(`keyboardLayout.${l}`);
    }
  });
});
