import { afterEach, describe, expect, it, vi } from 'vitest';
import { onDeepLink, readDeepLink } from './deeplink';

type Opts = {
  payload?: string; // the PAYLOAD value[0], if present
  noPayloadKey?: boolean; // data present but no PAYLOAD entry
  nullControl?: boolean; // getRequestedAppControl() returns null
  throwControl?: boolean; // getRequestedAppControl() throws
};

/** Install a fake `tizen` global whose requested app-control carries `payload`. */
function stubTizen(opts: Opts = {}) {
  const data = opts.noPayloadKey
    ? [{ key: 'OTHER', value: ['x'] }]
    : opts.payload !== undefined
      ? [{ key: 'PAYLOAD', value: [opts.payload] }]
      : [];
  const getRequestedAppControl = () => {
    if (opts.throwControl) throw new Error('boom');
    if (opts.nullControl) return null;
    return { appControl: { operation: 'op', data } };
  };
  vi.stubGlobal('tizen', {
    filesystem: {},
    application: { getCurrentApplication: () => ({ getRequestedAppControl }) },
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('readDeepLink', () => {
  it('returns null when there is no tizen platform', () => {
    expect(readDeepLink()).toBeNull();
  });

  it('reads the Android TV ?deeplink= param as a movie target', () => {
    vi.stubGlobal('location', { search: '?deeplink=itm42' });
    expect(readDeepLink()).toEqual({ type: 'movie', id: 'itm42' });
  });

  it('decodes a direct JSON payload', () => {
    stubTizen({ payload: JSON.stringify({ type: 'movie', id: 'abc' }) });
    expect(readDeepLink()).toEqual({ type: 'movie', id: 'abc' });
  });

  it('decodes a Samsung { values } wrapped, uri-encoded payload', () => {
    const inner = encodeURIComponent(JSON.stringify({ type: 'show', id: 's1' }));
    stubTizen({ payload: JSON.stringify({ values: inner }) });
    expect(readDeepLink()).toEqual({ type: 'show', id: 's1' });
  });

  it('returns null for malformed JSON', () => {
    stubTizen({ payload: '{not json' });
    expect(readDeepLink()).toBeNull();
  });

  it('returns null for a well-formed but wrong-shaped payload', () => {
    stubTizen({ payload: JSON.stringify({ type: 'other', id: 5 }) });
    expect(readDeepLink()).toBeNull();
  });

  it('returns null when the id is not a string', () => {
    stubTizen({ payload: JSON.stringify({ type: 'movie', id: 42 }) });
    expect(readDeepLink()).toBeNull();
  });

  it('returns null when there is no PAYLOAD entry', () => {
    stubTizen({ noPayloadKey: true });
    expect(readDeepLink()).toBeNull();
  });

  it('returns null when getRequestedAppControl yields null', () => {
    stubTizen({ nullControl: true });
    expect(readDeepLink()).toBeNull();
  });

  it('returns null (not throw) when the platform call throws', () => {
    stubTizen({ throwControl: true });
    expect(readDeepLink()).toBeNull();
  });
});

describe('onDeepLink', () => {
  it('returns a no-op cleanup when there is no tizen platform', () => {
    const cleanup = onDeepLink(() => undefined);
    expect(cleanup()).toBeUndefined();
  });

  it('subscribes to appcontrol and fires cb with the decoded link', () => {
    const listeners = new Map<string, () => void>();
    vi.stubGlobal('window', {
      addEventListener: (type: string, h: () => void) => listeners.set(type, h),
      removeEventListener: (type: string) => listeners.delete(type),
    });
    stubTizen({ payload: JSON.stringify({ type: 'movie', id: 'm9' }) });

    const cb = vi.fn();
    const cleanup = onDeepLink(cb);
    // Simulate the platform re-targeting the running app.
    listeners.get('appcontrol')?.();
    expect(cb).toHaveBeenCalledWith({ type: 'movie', id: 'm9' });

    cleanup();
    expect(listeners.has('appcontrol')).toBe(false);
  });

  it('does not fire cb when the re-target carries no valid link', () => {
    const listeners = new Map<string, () => void>();
    vi.stubGlobal('window', {
      addEventListener: (type: string, h: () => void) => listeners.set(type, h),
      removeEventListener: (type: string) => listeners.delete(type),
    });
    stubTizen({ payload: '{bad' });
    const cb = vi.fn();
    onDeepLink(cb);
    listeners.get('appcontrol')?.();
    expect(cb).not.toHaveBeenCalled();
  });
});
