// @vitest-environment jsdom
import type { KromaClient, MediaItem } from '@kroma/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AvplayEngine } from './avplayEngine';
import type { EngineOptions } from './baseEngine';
import type { AvplayApi, AvplayListeners, AvplayTrack, EngineListeners } from './engine';

// The Samsung AVPlay backend, driven against a fake `webapis.avplay` that records
// every native call, captures the event listener + prepareAsync callbacks, and
// serves stubbed track info / duration. Asserts the native call sequence, the
// resume seek, the audio-relative -> AVPlay track-index mapping, the re-anchor
// stop/close/reopen cycle and the visibility suspend/restore, with no TV.

interface FakeAvplay {
  api: AvplayApi;
  calls(): Array<{ m: string; args: unknown[] }>;
  listener(): AvplayListeners;
  prepareOk(): void;
  prepareErr(): void;
  setTracks(t: AvplayTrack[]): void;
  setDuration(ms: number): void;
}

function fakeAvplay(): FakeAvplay {
  const calls: Array<{ m: string; args: unknown[] }> = [];
  let listener: AvplayListeners = {};
  let ok: () => void = () => {};
  let err: () => void = () => {};
  let tracks: AvplayTrack[] = [];
  let duration = 0;
  const rec =
    (m: string) =>
    (...args: unknown[]) =>
      calls.push({ m, args });
  const api = {
    open: rec('open'),
    close: rec('close'),
    stop: rec('stop'),
    play: rec('play'),
    pause: rec('pause'),
    seekTo: rec('seekTo'),
    setDisplayRect: rec('setDisplayRect'),
    setStreamingProperty: rec('setStreamingProperty'),
    setSilentSubtitle: rec('setSilentSubtitle'),
    setSelectTrack: rec('setSelectTrack'),
    suspend: rec('suspend'),
    restore: rec('restore'),
    getCurrentTime: () => 0,
    getState: () => 'PLAYING',
    getDuration: () => duration,
    getTotalTrackInfo: () => tracks,
    setListener: (l: AvplayListeners) => {
      listener = l;
      calls.push({ m: 'setListener', args: [] });
    },
    prepareAsync: (onOk: () => void, onErr: () => void) => {
      ok = onOk;
      err = onErr;
      calls.push({ m: 'prepareAsync', args: [] });
    },
  } as unknown as AvplayApi;
  return {
    api,
    calls: () => calls,
    listener: () => listener,
    prepareOk: () => ok(),
    prepareErr: () => err(),
    setTracks: (t) => {
      tracks = t;
    },
    setDuration: (ms) => {
      duration = ms;
    },
  };
}

function mkListeners(): EngineListeners {
  return {
    onTime: vi.fn(),
    onDuration: vi.fn(),
    onBuffered: vi.fn(),
    onPlay: vi.fn(),
    onPause: vi.fn(),
    onWaiting: vi.fn(),
    onPlaying: vi.fn(),
    onEnded: vi.fn(),
    onError: vi.fn(),
    onReady: vi.fn(),
  };
}

const client = {
  streamUrl: (id: string) => `stream:${id}`,
  hlsMasterUrl: (id: string, aac: boolean, startSec: number, audio: number, filter?: string) =>
    `master:${id}:${aac}:${startSec}:${audio}${filter ? `:${filter}` : ''}`,
} as unknown as KromaClient;
const item = { id: 'sm1' } as unknown as MediaItem;
const tick = () => new Promise<void>((r) => setTimeout(r, 0));
const track = (index: number, type: string): AvplayTrack => ({ index, type }) as AvplayTrack;

function opts(over: Partial<EngineOptions> = {}): EngineOptions {
  return {
    client,
    item,
    durationSec: 100,
    initialRendition: 0,
    startSec: 0,
    direct: true,
    listeners: mkListeners(),
    ...over,
  };
}

function make(over: Partial<EngineOptions> = {}) {
  const a = fakeAvplay();
  vi.stubGlobal('webapis', { avplay: a.api });
  const listeners = over.listeners ?? mkListeners();
  const e = new AvplayEngine(opts({ ...over, listeners }));
  const names = () => a.calls().map((c) => c.m);
  const lastArgs = (m: string) => [...a.calls()].reverse().find((c) => c.m === m)?.args;
  return { e, a, listeners, names, lastArgs };
}

beforeEach(() => {
  vi.stubGlobal(
    'fetch',
    vi.fn(() =>
      Promise.resolve({ headers: { get: (k: string) => (k === 'X-Hls-Start' ? '8' : null) } }),
    ),
  );
});
afterEach(() => vi.unstubAllGlobals());

describe('AvplayEngine construction', () => {
  it('throws when AVPlay is unavailable', () => {
    expect(() => new AvplayEngine(opts())).toThrow(/AVPlay unavailable/);
  });

  it('direct mode opens the original file and sets the display rect', () => {
    const { a, lastArgs } = make({ direct: true });
    expect(lastArgs('open')).toEqual(['stream:sm1']);
    expect(lastArgs('setDisplayRect')).toEqual([0, 0, 1920, 1080]);
    expect(a.calls().map((c) => c.m)).toContain('prepareAsync');
  });

  it('setRect shrinks the plane into the card (fractions -> 1920x1080 px); null restores', () => {
    const { e, lastArgs } = make({ direct: true });
    e.setRect({ x: 0.03, y: 0.25, w: 0.5, h: 0.5 });
    expect(lastArgs('setDisplayRect')).toEqual([58, 270, 960, 540]);
    e.setRect(null);
    expect(lastArgs('setDisplayRect')).toEqual([0, 0, 1920, 1080]);
  });

  it('master mode opens the anchored HLS master (no anchor -> immediate)', () => {
    const { lastArgs } = make({ direct: false, startSec: 0 });
    expect(lastArgs('open')).toEqual(['master:sm1:false:0:0']);
  });

  it('an anchored master resolves its real keyframe start before opening', async () => {
    const { e, lastArgs } = make({ direct: false, startSec: 30 });
    await tick(); // resolveMasterStart -> fetch -> X-Hls-Start 8
    expect(lastArgs('open')).toEqual(['master:sm1:false:30:0']);
    expect(e.position()).toBe(8); // baseSec corrected to the reported keyframe
  });
});

describe('AvplayEngine prepared (resume + audio mapping)', () => {
  it('applies the resume seek and maps the rendition to the AVPlay track index', () => {
    const { a, lastArgs, listeners } = make({ direct: true, startSec: 30, initialRendition: 1 });
    a.setDuration(60000);
    a.setTracks([track(0, 'VIDEO'), track(1, 'AUDIO'), track(2, 'AUDIO')]);
    a.prepareOk();
    // duration from getDuration (ms -> s)
    expect(listeners.onDuration).toHaveBeenCalledWith(60);
    // resume seek in ms
    expect(lastArgs('seekTo')).toEqual([30000]);
    // audio-relative rendition 1 -> the SECOND audio track's index (2)
    expect(lastArgs('setSelectTrack')).toEqual(['AUDIO', 2]);
    expect(listeners.onReady).toHaveBeenCalledTimes(1);
  });

  it('master prepared just announces duration + ready (no seek/track)', () => {
    const { a, names, listeners } = make({ direct: false, startSec: 0 });
    a.prepareOk();
    expect(names()).not.toContain('seekTo');
    expect(names()).not.toContain('setSelectTrack');
    expect(listeners.onReady).toHaveBeenCalledTimes(1);
  });
});

describe('AvplayEngine native listener events', () => {
  it('current-play-time updates the absolute position + buffered', () => {
    const { e, a, listeners } = make({ direct: true, startSec: 0 });
    a.listener().oncurrentplaytime?.(5000);
    expect(e.position()).toBe(5);
    expect(listeners.onTime).toHaveBeenCalledWith(5);
    expect(listeners.onBuffered).toHaveBeenCalledWith(5);
  });

  it('buffering + stream-completed + error map to the right callbacks', () => {
    const { a, listeners } = make({ direct: true, startSec: 0 });
    a.listener().onbufferingstart?.();
    a.listener().onbufferingcomplete?.();
    a.listener().onstreamcompleted?.();
    expect(listeners.onWaiting).toHaveBeenCalledTimes(1);
    expect(listeners.onPlaying).toHaveBeenCalledTimes(1);
    expect(listeners.onEnded).toHaveBeenCalledTimes(1);
    // onerror in direct mode triggers the direct->master fallback (buffering shown)
    a.listener().onerror?.(new Error('x'));
    expect(listeners.onWaiting).toHaveBeenCalledTimes(2);
  });
});

describe('AvplayEngine transport', () => {
  it('play / pause call the native API + emit optimistic state', () => {
    const { e, names, listeners } = make();
    e.play();
    expect(names()).toContain('play');
    expect(e.isPaused()).toBe(false);
    expect(listeners.onPlay).toHaveBeenCalled();
    e.pause();
    expect(names()).toContain('pause');
    expect(e.isPaused()).toBe(true);
    expect(listeners.onPause).toHaveBeenCalled();
  });

  it('direct seek is a native absolute seek in ms (floored at 0)', () => {
    const { e, lastArgs } = make({ direct: true });
    e.seekTo(50);
    expect(lastArgs('seekTo')).toEqual([50000]);
    e.seekTo(-5);
    expect(lastArgs('seekTo')).toEqual([0]);
  });

  it('master seek within the ahead window is a relative native seek', () => {
    const { e, lastArgs } = make({ direct: false, startSec: 0 });
    e.seekTo(40);
    expect(lastArgs('seekTo')).toEqual([40000]);
    expect(e.position()).toBe(40);
  });

  it('a far master seek re-anchors: stop, close, reopen at the new anchor', async () => {
    const { e, names, lastArgs } = make({ direct: false, startSec: 0 });
    e.seekTo(600);
    await tick();
    expect(names()).toContain('stop');
    expect(names()).toContain('close');
    expect(lastArgs('open')).toEqual(['master:sm1:false:600:0']);
  });
});

describe('AvplayEngine audio switching', () => {
  it('a direct in-place switch selects the mapped track (no reopen)', () => {
    const { e, a, names, lastArgs } = make({ direct: true, initialRendition: 0 });
    a.setTracks([track(1, 'AUDIO'), track(4, 'AUDIO')]);
    e.setAudioRendition(1);
    expect(lastArgs('setSelectTrack')).toEqual(['AUDIO', 4]);
    expect(names()).not.toContain('stop'); // stayed in place
  });

  it('falls back to a re-anchor when the track cannot be selected', async () => {
    const { e, names } = make({ direct: true, initialRendition: 0 });
    // no tracks -> selectNativeAudio returns false -> reanchor
    e.setAudioRendition(1);
    await tick();
    expect(names()).toContain('stop');
    expect(names()).toContain('close');
  });

  it('a master switch re-anchors at the current position with the new track', async () => {
    const { e, a, lastArgs } = make({ direct: false, startSec: 0 });
    a.listener().oncurrentplaytime?.(25000); // position -> 25
    e.setAudioRendition(1);
    await tick();
    expect(lastArgs('open')).toEqual(['master:sm1:false:25:1']);
  });
});

describe('AvplayEngine visibility + destroy', () => {
  it('suspends when hidden and restores when visible again', () => {
    const { names, lastArgs } = make({ direct: true, startSec: 0 });
    Object.defineProperty(document, 'visibilityState', { value: 'hidden', configurable: true });
    document.dispatchEvent(new Event('visibilitychange'));
    expect(names()).toContain('suspend');
    Object.defineProperty(document, 'visibilityState', { value: 'visible', configurable: true });
    document.dispatchEvent(new Event('visibilitychange'));
    expect(names()).toContain('restore');
    expect(lastArgs('restore')?.[2]).toBe('PLAYING');
  });

  it('destroy stops + closes the singleton and detaches the visibility listener', () => {
    const { e, names } = make();
    e.destroy();
    expect(names()).toContain('stop');
    expect(names()).toContain('close');
    const before = names().length;
    document.dispatchEvent(new Event('visibilitychange'));
    expect(names()).toHaveLength(before); // listener gone
  });
});

describe('AvplayEngine audio filter (server-side remux)', () => {
  it('a persisted filter opens the FILTERED master even for a direct-playable file', () => {
    const { lastArgs } = make({ direct: true, startSec: 0, audioFilter: 'night' });
    expect(lastArgs('open')).toEqual(['master:sm1:false:0:0:night']);
  });

  it('enabling the filter mid-play moves a direct source onto the filtered master', async () => {
    const { e, a, lastArgs, listeners } = make({ direct: true, startSec: 0 });
    a.listener().oncurrentplaytime?.(30000); // playing at 30s
    e.setAudioFilter('standard');
    await tick(); // anchored master resolves its real start first
    expect(listeners.onWaiting).toHaveBeenCalled();
    expect(lastArgs('open')).toEqual(['master:sm1:false:30:0:standard']);
  });

  it('turning the filter off drops a filter-forced master back to the direct file', () => {
    const { e, a, lastArgs } = make({ direct: true, startSec: 0, audioFilter: 'night' });
    a.listener().oncurrentplaytime?.(10000);
    e.setAudioFilter('off');
    expect(lastArgs('open')).toEqual(['stream:sm1']);
    a.prepareOk();
    expect(lastArgs('seekTo')).toEqual([10000]); // resumes where it was
  });

  it('a filter change on a real master reloads it with the new mode', () => {
    const { e, lastArgs } = make({ direct: false, startSec: 0 });
    e.setAudioFilter('night');
    expect(lastArgs('open')).toEqual(['master:sm1:false:0:0:night']);
  });
});
