import type { KromaClient, MediaItem } from '@kroma/core';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { EngineOptions } from './baseEngine';
import type { EngineListeners } from './engine';
import { ExoEngine } from './exoEngine';

// The Android TV ExoPlayer backend, driven against a fake `__KROMA_ANDROID__`
// bridge (records load/command) with events pushed through the global
// `__kromaExoEvent` callback the Kotlin side would call. Asserts the JSON command
// protocol, the source decision, the event -> listener mapping and the
// direct->master fallback.

interface ExoEvt {
  t: string;
  sec?: number;
  playing?: boolean;
  active?: boolean;
}

interface FakeExo {
  bridge: unknown;
  loads(): { url: string; startSec: number; master: boolean }[];
  cmds(): Array<{ op: string; value?: number }>;
}

function fakeExo(): FakeExo {
  const loads: { url: string; startSec: number; master: boolean }[] = [];
  const cmds: Array<{ op: string; value?: number }> = [];
  const bridge = {
    load: (url: string, startSec: number, master: boolean) => loads.push({ url, startSec, master }),
    command: (json: string) => cmds.push(JSON.parse(json)),
  };
  return { bridge, loads: () => loads, cmds: () => cmds };
}

function emit(e: ExoEvt): void {
  (globalThis as { __kromaExoEvent?: (e: ExoEvt) => void }).__kromaExoEvent?.(e);
}
function hasHandler(): boolean {
  return '__kromaExoEvent' in globalThis;
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
const item = { id: 'ex1' } as unknown as MediaItem;
const tick = () => new Promise<void>((r) => setTimeout(r, 0));

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
  const x = fakeExo();
  vi.stubGlobal('__KROMA_ANDROID__', x.bridge);
  const listeners = over.listeners ?? mkListeners();
  const e = new ExoEngine(opts({ ...over, listeners }));
  return { e, x, listeners };
}

beforeEach(() => {
  vi.stubGlobal(
    'fetch',
    vi.fn(() =>
      Promise.resolve({ headers: { get: (k: string) => (k === 'X-Hls-Start' ? '9' : null) } }),
    ),
  );
});
afterEach(() => {
  delete (globalThis as { __kromaExoEvent?: unknown }).__kromaExoEvent;
  vi.unstubAllGlobals();
});

describe('ExoEngine construction', () => {
  it('throws when the bridge is missing', () => {
    expect(() => new ExoEngine(opts())).toThrow(/ExoPlayer bridge/);
  });

  it('direct mode loads the original file (progressive) at the resume offset', () => {
    const { e, x } = make({ direct: true, startSec: 20 });
    expect(x.loads()).toEqual([{ url: 'stream:ex1', startSec: 20, master: false }]);
    expect(e.isPaused()).toBe(true); // paused until the first state event
  });

  it('master mode loads the anchored HLS master (no anchor -> immediate)', () => {
    const { x } = make({ direct: false, startSec: 0 });
    expect(x.loads()).toEqual([{ url: 'master:ex1:false:0:0', startSec: 0, master: true }]);
  });
});

describe('ExoEngine event mapping', () => {
  it('ready applies the audio track then announces ready (direct)', () => {
    const { x, listeners } = make({ direct: true, initialRendition: 2 });
    emit({ t: 'ready' });
    expect(x.cmds()).toContainEqual({ op: 'audio', value: 2 });
    expect(listeners.onDuration).toHaveBeenCalledWith(100);
    expect(listeners.onReady).toHaveBeenCalledTimes(1);
  });

  it('time updates the absolute position', () => {
    const { e, listeners } = make();
    emit({ t: 'time', sec: 15 });
    expect(e.position()).toBe(15);
    expect(listeners.onTime).toHaveBeenCalledWith(15);
  });

  it('direct duration overrides the runtime; master keeps the catalogue value', () => {
    const direct = make({ direct: true });
    emit({ t: 'duration', sec: 5400 });
    expect(direct.e.duration()).toBe(5400);

    delete (globalThis as { __kromaExoEvent?: unknown }).__kromaExoEvent;
    const master = make({ direct: false });
    emit({ t: 'duration', sec: 5400 });
    expect(master.e.duration()).toBe(100);
  });

  it('buffered feeds bufferedEnd + onBuffered', () => {
    const { e, listeners } = make();
    emit({ t: 'buffered', sec: 42 });
    expect(listeners.onBuffered).toHaveBeenCalledWith(42);
    expect(e.bufferedEnd()).toBe(42);
  });

  it('state drives play/pause; waiting drives waiting/playing', () => {
    const { e, listeners } = make();
    emit({ t: 'state', playing: true });
    expect(e.isPaused()).toBe(false);
    expect(listeners.onPlay).toHaveBeenCalled();
    emit({ t: 'state', playing: false });
    expect(e.isPaused()).toBe(true);
    expect(listeners.onPause).toHaveBeenCalled();
    emit({ t: 'waiting', active: true });
    emit({ t: 'waiting', active: false });
    expect(listeners.onWaiting).toHaveBeenCalledTimes(1);
    expect(listeners.onPlaying).toHaveBeenCalledTimes(1);
  });

  it('ended reports ended', () => {
    const { listeners } = make();
    emit({ t: 'ended' });
    expect(listeners.onEnded).toHaveBeenCalledTimes(1);
  });

  it('an error in direct mode falls back to the master', () => {
    const { x, listeners } = make({ direct: true, startSec: 0 });
    emit({ t: 'error' });
    expect(listeners.onWaiting).toHaveBeenCalled();
    expect(x.loads().some((l) => l.url === 'master:ex1:false:0:0' && l.master === true)).toBe(true);
  });
});

describe('ExoEngine transport', () => {
  it('play / pause emit the JSON command + optimistic state', () => {
    const { e, x, listeners } = make();
    e.play();
    expect(x.cmds()).toContainEqual({ op: 'play' });
    expect(e.isPaused()).toBe(false);
    expect(listeners.onPlay).toHaveBeenCalled();
    e.pause();
    expect(x.cmds()).toContainEqual({ op: 'pause' });
    expect(e.isPaused()).toBe(true);
  });

  it('direct seek is native + absolute (floored at 0)', () => {
    const { e, x } = make({ direct: true });
    e.seekTo(88);
    expect(x.cmds()).toContainEqual({ op: 'seek', value: 88 });
    e.seekTo(-3);
    expect(x.cmds()).toContainEqual({ op: 'seek', value: 0 });
  });

  it('master seek within the ahead window is a relative native seek', () => {
    const { e, x } = make({ direct: false, startSec: 0 });
    e.seekTo(40);
    expect(x.cmds()).toContainEqual({ op: 'seek', value: 40 });
    expect(e.position()).toBe(40);
  });

  it('a far master seek re-anchors + reloads', async () => {
    const { e, x } = make({ direct: false, startSec: 0 });
    e.seekTo(600);
    await tick();
    expect(x.loads().some((l) => l.url === 'master:ex1:false:600:0')).toBe(true);
  });

  it('direct audio switch is an in-place command; same rendition is a no-op', () => {
    const { e, x } = make({ direct: true, initialRendition: 0 });
    e.setAudioRendition(1);
    expect(x.cmds()).toContainEqual({ op: 'audio', value: 1 });
    const before = x.cmds().length;
    e.setAudioRendition(1);
    expect(x.cmds()).toHaveLength(before);
  });

  it('master audio switch re-anchors at the current position', async () => {
    const { e, x } = make({ direct: false, startSec: 0 });
    emit({ t: 'time', sec: 25 });
    e.setAudioRendition(1);
    await tick();
    expect(x.loads().some((l) => l.url === 'master:ex1:false:25:1')).toBe(true);
  });

  it('destroy stops playback and removes the global event handler', () => {
    const { e, x } = make();
    e.destroy();
    expect(x.cmds()).toContainEqual({ op: 'stop' });
    expect(hasHandler()).toBe(false);
  });

  it('setRect sends the fraction-rect; null restores fullscreen', () => {
    const { e, x } = make({ direct: true });
    e.setRect({ x: 0.03, y: 0.25, w: 0.5, h: 0.5 });
    expect(x.cmds()).toContainEqual({ op: 'rect', x: 0.03, y: 0.25, w: 0.5, h: 0.5 });
    e.setRect(null);
    expect(x.cmds()).toContainEqual({ op: 'rect' });
  });
});
