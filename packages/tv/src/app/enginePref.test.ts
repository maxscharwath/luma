import { afterEach, describe, expect, it, vi } from 'vitest';
import { availableEngines, ENGINE_LABEL_KEY, type EnginePref } from './enginePref';

/** A Map-backed localStorage stand-in. */
function fakeStorage(initial: Record<string, string> = {}) {
  const m = new Map(Object.entries(initial));
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    _map: m,
  };
}

const tauri = { core: { invoke: () => undefined }, event: { listen: () => undefined } };
const exo = { load: () => undefined, command: () => undefined };

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('getEnginePref / setEnginePref', () => {
  // The pref is a reactive store (settings/store.ts) that reads storage ONCE at
  // module creation and is the in-process source of truth afterwards, so each
  // test imports a fresh module instance with its storage stub already up.
  async function fresh(storage: unknown) {
    vi.resetModules();
    vi.stubGlobal('localStorage', storage);
    return import('./enginePref');
  }

  it('defaults to auto when nothing is stored', async () => {
    const m = await fresh(fakeStorage());
    expect(m.getEnginePref()).toBe('auto');
  });

  it('returns a stored valid preference', async () => {
    const m = await fresh(fakeStorage({ 'kroma:engine': 'avplay' }));
    expect(m.getEnginePref()).toBe('avplay');
  });

  it('ignores an unknown stored value', async () => {
    const m = await fresh(fakeStorage({ 'kroma:engine': 'bogus' }));
    expect(m.getEnginePref()).toBe('auto');
  });

  it('persists the preference', async () => {
    const store = fakeStorage();
    const m = await fresh(store);
    m.setEnginePref('remux');
    expect(store._map.get('kroma:engine')).toBe('remux');
    expect(m.getEnginePref()).toBe('remux');
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
    expect(m.getEnginePref()).toBe('auto');
    expect(() => m.setEnginePref('mpv')).not.toThrow();
    // The in-process value holds even when the storage write failed.
    expect(m.getEnginePref()).toBe('mpv');
  });
});

describe('availableEngines', () => {
  it('offers exo + remux on the Android TV shell', () => {
    vi.stubGlobal('__KROMA_ANDROID__', exo);
    vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Linux; Android 12)' });
    expect(availableEngines()).toEqual(['auto', 'exo', 'remux']);
  });

  it('offers avplay + remux on Tizen', () => {
    vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (SMART-TV; Tizen 6.0)' });
    expect(availableEngines()).toEqual(['auto', 'avplay', 'remux']);
  });

  it('offers webview + remux on webOS', () => {
    vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Web0S; LG)' });
    expect(availableEngines()).toEqual(['auto', 'webview', 'remux']);
  });

  it('falls back to webview + remux on an unknown platform', () => {
    vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Macintosh)' });
    expect(availableEngines()).toEqual(['auto', 'webview', 'remux']);
  });

  it('inserts mpv on a Linux Tauri desktop shell', () => {
    vi.stubGlobal('__TAURI__', tauri);
    vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (X11; Linux x86_64) Tauri' });
    expect(availableEngines()).toEqual(['auto', 'mpv', 'webview', 'remux']);
  });

  it('inserts mpv on a macOS Tauri shell that flagged libmpv', () => {
    vi.stubGlobal('__TAURI__', tauri);
    vi.stubGlobal('__KROMA_MPV__', true);
    vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X) Tauri' });
    expect(availableEngines()).toEqual(['auto', 'mpv', 'webview', 'remux']);
  });

  it('does NOT insert mpv on a Tauri Android shell', () => {
    vi.stubGlobal('__TAURI__', tauri);
    vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Linux; Android 12) Tauri' });
    expect(availableEngines()).toEqual(['auto', 'webview', 'remux']);
  });
});

describe('ENGINE_LABEL_KEY', () => {
  it('maps every engine to its i18n label key', () => {
    const engines: EnginePref[] = ['auto', 'avplay', 'webview', 'remux', 'mpv', 'exo'];
    for (const e of engines) {
      expect(ENGINE_LABEL_KEY[e]).toBe(`playbackEngine.${e}`);
    }
  });
});
