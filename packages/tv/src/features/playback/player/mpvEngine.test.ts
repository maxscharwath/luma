import type { KromaClient, MediaItem } from '@kroma/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { EngineOptions } from './baseEngine';
import type { EngineListeners } from './engine';
import { MpvEngine } from './mpvEngine';

// The desktop mpv backend, driven against a fake Tauri bridge that records every
// IPC command and lets the test push the observed-property / lifecycle events the
// shell would forward. Asserts the IPC the REAL transport emits (load / seek /
// set_property aid) and the event -> listener mapping, not a native process.

interface FakeTauri {
  bridge: unknown;
  cmds(): unknown[][]; // mpv_command arg arrays
  loads(): { url: string; start: number }[];
  emit(event: string, payload: unknown): void;
  hasListener(event: string): boolean;
}

function fakeTauri(status: string = 'ok'): FakeTauri {
  const cmds: unknown[][] = [];
  const loads: { url: string; start: number }[] = [];
  const listeners = new Map<string, (e: { payload: unknown }) => void>();
  const bridge = {
    core: {
      invoke: (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === 'mpv_command') cmds.push(args?.args as unknown[]);
        else if (cmd === 'mpv_load')
          loads.push({ url: args?.url as string, start: args?.start as number });
        else if (cmd === 'mpv_status') return Promise.resolve(status);
        return Promise.resolve(undefined);
      },
    },
    event: {
      listen: (event: string, cb: (e: { payload: unknown }) => void) => {
        listeners.set(event, cb);
        return Promise.resolve(() => listeners.delete(event));
      },
    },
  };
  return {
    bridge,
    cmds: () => cmds,
    loads: () => loads,
    emit: (event, payload) => listeners.get(event)?.({ payload }),
    hasListener: (event) => listeners.has(event),
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
  hlsMasterUrl: (id: string, aac: boolean, startSec: number, audio: number) =>
    `master:${id}:${aac}:${startSec}:${audio}`,
} as unknown as KromaClient;
const item = { id: 'v9' } as unknown as MediaItem;
const tick = () => new Promise<void>((r) => setTimeout(r, 0));
const props = (name: string, data: unknown) => ({ name, data });

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

beforeEach(() => {
  vi.stubGlobal(
    'fetch',
    vi.fn(() =>
      Promise.resolve({ headers: { get: (k: string) => (k === 'X-Hls-Start' ? '7' : null) } }),
    ),
  );
});
afterEach(() => vi.unstubAllGlobals());

describe('MpvEngine construction / open', () => {
  it('throws when the Tauri bridge is unavailable', () => {
    expect(() => new MpvEngine(opts())).toThrow(/mpv bridge/);
  });

  it('direct mode loads the original file at the resume offset', () => {
    const t = fakeTauri();
    vi.stubGlobal('__TAURI__', t.bridge);
    const e = new MpvEngine(opts({ direct: true, startSec: 30 }));
    e.start();
    expect(t.loads()).toEqual([{ url: 'stream:v9', start: 30 }]);
  });

  it('master mode loads the anchored master (no anchor -> immediate)', () => {
    const t = fakeTauri();
    vi.stubGlobal('__TAURI__', t.bridge);
    const e = new MpvEngine(opts({ direct: false, startSec: 0 }));
    e.start();
    expect(t.loads()).toEqual([{ url: 'master:v9:false:0:0', start: 0 }]);
  });

  it('subscribes to the shell events after start', async () => {
    const t = fakeTauri();
    vi.stubGlobal('__TAURI__', t.bridge);
    new MpvEngine(opts()).start();
    await tick();
    for (const ev of ['mpv://property', 'mpv://file-loaded', 'mpv://end-file', 'mpv://error'])
      expect(t.hasListener(ev)).toBe(true);
  });

  it('a dead mpv process fails fast on the status probe', async () => {
    const t = fakeTauri('dead');
    vi.stubGlobal('__TAURI__', t.bridge);
    const listeners = mkListeners();
    new MpvEngine(opts({ listeners })).start();
    await tick();
    expect(listeners.onError).toHaveBeenCalledTimes(1);
  });
});

describe('MpvEngine observed properties', () => {
  function started(over: Partial<EngineOptions> = {}) {
    const t = fakeTauri();
    vi.stubGlobal('__TAURI__', t.bridge);
    const listeners = over.listeners ?? mkListeners();
    const e = new MpvEngine(opts({ ...over, listeners }));
    e.start();
    return { e, t, listeners };
  }

  it('time-pos updates the absolute position', () => {
    const { e, t, listeners } = started();
    t.emit('mpv://property', props('time-pos', 12));
    expect(e.position()).toBe(12);
    expect(listeners.onTime).toHaveBeenCalledWith(12);
  });

  it('pause property drives play/pause callbacks + state', () => {
    const { e, t, listeners } = started();
    t.emit('mpv://property', props('pause', true));
    expect(e.isPaused()).toBe(true);
    expect(listeners.onPause).toHaveBeenCalledTimes(1);
    t.emit('mpv://property', props('pause', false));
    expect(e.isPaused()).toBe(false);
    expect(listeners.onPlay).toHaveBeenCalledTimes(1);
  });

  it('paused-for-cache maps to waiting / playing', () => {
    const { t, listeners } = started();
    t.emit('mpv://property', props('paused-for-cache', true));
    t.emit('mpv://property', props('paused-for-cache', false));
    expect(listeners.onWaiting).toHaveBeenCalledTimes(1);
    expect(listeners.onPlaying).toHaveBeenCalledTimes(1);
  });

  it('demuxer-cache-time feeds bufferedEnd + onBuffered', () => {
    const { e, t, listeners } = started();
    t.emit('mpv://property', props('demuxer-cache-time', 48));
    expect(listeners.onBuffered).toHaveBeenCalledWith(48);
    expect(e.bufferedEnd()).toBe(48); // baseSec 0 + max(elSec 0, cache 48)
  });

  it('direct-mode duration overrides the catalogue runtime; master keeps it', () => {
    const direct = started({ direct: true });
    direct.t.emit('mpv://property', props('duration', 3600));
    expect(direct.e.duration()).toBe(3600);
    expect(direct.listeners.onDuration).toHaveBeenCalledWith(3600);

    const master = started({ direct: false });
    master.t.emit('mpv://property', props('duration', 3600));
    expect(master.e.duration()).toBe(100); // catalogue runtime kept
  });
});

describe('MpvEngine audio track mapping', () => {
  function started(rendition = 0) {
    const t = fakeTauri();
    vi.stubGlobal('__TAURI__', t.bridge);
    const e = new MpvEngine(opts({ direct: true, initialRendition: rendition }));
    e.start();
    return { e, t };
  }

  it('maps the audio-relative rendition to mpv aid via the observed track-list', () => {
    const { t } = started(1);
    t.emit(
      'mpv://property',
      props('track-list', [
        { id: 1, type: 'video' },
        { id: 3, type: 'audio' },
        { id: 4, type: 'audio' },
      ]),
    );
    // rendition 1 -> the SECOND audio track's real id (4), not a naive 1,2,3.
    expect(t.cmds()).toContainEqual(['set_property', 'aid', 4]);
  });

  it('falls back to rendition+1 before the track-list arrives', () => {
    const { t } = started(0);
    t.emit('mpv://file-loaded', null); // onLoaded selects audio with no list yet
    expect(t.cmds()).toContainEqual(['set_property', 'aid', 1]); // 0 + 1
  });

  it('an in-place switch selects the mapped track (direct)', () => {
    const { e, t } = started(0);
    t.emit(
      'mpv://property',
      props('track-list', [
        { id: 2, type: 'audio' },
        { id: 5, type: 'audio' },
      ]),
    );
    e.setAudioRendition(1);
    expect(t.cmds()).toContainEqual(['set_property', 'aid', 5]);
  });

  it('ignores selecting the already-active rendition', () => {
    const { e, t } = started(0);
    const before = t.cmds().length;
    e.setAudioRendition(0);
    expect(t.cmds()).toHaveLength(before);
  });
});

describe('MpvEngine seek + play/pause', () => {
  function started(over: Partial<EngineOptions> = {}) {
    const t = fakeTauri();
    vi.stubGlobal('__TAURI__', t.bridge);
    const listeners = mkListeners();
    const e = new MpvEngine(opts({ ...over, listeners }));
    e.start();
    return { e, t, listeners };
  }

  it('direct seek is native + absolute (floored at 0)', () => {
    const { e, t } = started({ direct: true });
    e.seekTo(50);
    expect(t.cmds()).toContainEqual(['seek', 50, 'absolute']);
    e.seekTo(-5);
    expect(t.cmds()).toContainEqual(['seek', 0, 'absolute']);
    expect(e.position()).toBe(0);
  });

  it('master seek within the ahead window is a relative native seek', () => {
    const { e, t } = started({ direct: false, startSec: 0 });
    e.seekTo(30); // baseSec 0, within +60
    expect(t.cmds()).toContainEqual(['seek', 30, 'absolute']);
    expect(e.position()).toBe(30);
  });

  it('a far master seek re-anchors + reloads the master', async () => {
    const { e, t } = started({ direct: false, startSec: 0 });
    e.seekTo(600); // beyond the ahead window -> re-anchor
    await tick();
    expect(t.loads().some((l) => l.url === 'master:v9:false:600:0')).toBe(true);
  });

  it('play / pause emit the IPC + optimistic state', () => {
    const { e, t, listeners } = started();
    e.play();
    expect(t.cmds()).toContainEqual(['set_property', 'pause', false]);
    expect(listeners.onPlay).toHaveBeenCalled();
    e.pause();
    expect(t.cmds()).toContainEqual(['set_property', 'pause', true]);
    expect(listeners.onPause).toHaveBeenCalled();
  });
});

describe('MpvEngine end-of-file + destroy', () => {
  function started(over: Partial<EngineOptions> = {}) {
    const t = fakeTauri();
    vi.stubGlobal('__TAURI__', t.bridge);
    const listeners = mkListeners();
    const e = new MpvEngine(opts({ ...over, listeners }));
    e.start();
    return { e, t, listeners };
  }

  it('eof reports ended', () => {
    const { t, listeners } = started();
    t.emit('mpv://end-file', { reason: 'eof' });
    expect(listeners.onEnded).toHaveBeenCalledTimes(1);
  });

  it('a decode error in direct mode falls back to the master', () => {
    const { t, listeners } = started({ direct: true, startSec: 0 });
    t.emit('mpv://end-file', { reason: 'error' });
    expect(listeners.onWaiting).toHaveBeenCalled(); // fallback announces buffering
    expect(t.loads().some((l) => l.url === 'master:v9:false:0:0')).toBe(true);
  });

  it('destroy stops the file and detaches listeners', async () => {
    const { e, t } = started();
    await tick(); // let subscribe() finish pushing its unlisten handles
    e.destroy();
    expect(t.cmds()).toContainEqual(['stop']);
    expect(t.hasListener('mpv://property')).toBe(false);
  });

  it('setRect insets the video with margin ratios; null clears them', () => {
    const { e, t } = started();
    e.setRect({ x: 0.03, y: 0.25, w: 0.5, h: 0.5 });
    expect(t.cmds()).toContainEqual(['set_property', 'video-margin-ratio-left', 0.03]);
    expect(t.cmds()).toContainEqual(['set_property', 'video-margin-ratio-top', 0.25]);
    expect(t.cmds()).toContainEqual(['set_property', 'video-margin-ratio-right', 1 - (0.03 + 0.5)]);
    expect(t.cmds()).toContainEqual([
      'set_property',
      'video-margin-ratio-bottom',
      1 - (0.25 + 0.5),
    ]);
    e.setRect(null);
    expect(t.cmds()).toContainEqual(['set_property', 'video-margin-ratio-left', 0]);
  });
});
