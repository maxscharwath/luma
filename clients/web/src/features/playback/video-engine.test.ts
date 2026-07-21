// @vitest-environment jsdom
import type { EngineDecision } from '@kroma/core';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { MovieView } from '#web/shared/lib/api';
import {
  type AttachSourceOptions,
  attachMediaSource,
  bindMediaEvents,
  type MediaEventSetters,
} from './video-engine';

// The web `<video>` transport: media-event -> hook-state wiring and the source
// decision (direct-play vs the HLS master). The API client + hls.js are mocked so
// the anchor math, the buffered/duration reporting and the ready-gated autoplay
// are exercised against a hand-rolled fake element, with no network or real MSE.

const H = vi.hoisted(() => {
  const hlsMasterUrl = vi.fn(
    (id: string, aac: boolean, startSec: number, audio: number) =>
      `hls:${id}:${aac}:${startSec}:${audio}`,
  );
  const instances: Array<{
    loadSource: ReturnType<typeof vi.fn>;
    attachMedia: ReturnType<typeof vi.fn>;
    destroy: ReturnType<typeof vi.fn>;
  }> = [];
  class FakeHls {
    static supported = true;
    static isSupported() {
      return FakeHls.supported;
    }
    loadSource = vi.fn();
    attachMedia = vi.fn();
    destroy = vi.fn();
    constructor(_cfg: unknown) {
      instances.push(this);
    }
  }
  const shakaInstances: Array<{
    attach: ReturnType<typeof vi.fn>;
    load: ReturnType<typeof vi.fn>;
    destroy: ReturnType<typeof vi.fn>;
    configure: ReturnType<typeof vi.fn>;
  }> = [];
  class FakeShakaPlayer {
    static supported = true;
    static isBrowserSupported() {
      return FakeShakaPlayer.supported;
    }
    attach = vi.fn(() => Promise.resolve());
    load = vi.fn(() => Promise.resolve());
    destroy = vi.fn(() => Promise.resolve());
    configure = vi.fn(() => true);
    constructor() {
      shakaInstances.push(this);
    }
  }
  const installAll = vi.fn();
  const FakeShaka = { Player: FakeShakaPlayer, polyfill: { installAll } };
  return { hlsMasterUrl, instances, FakeHls, shakaInstances, FakeShaka, installAll };
});

vi.mock('#web/shared/lib/api', () => ({
  kromaClient: () => ({ hlsMasterUrl: H.hlsMasterUrl }),
}));
vi.mock('hls.js', () => ({ default: H.FakeHls }));
vi.mock('shaka-player/dist/shaka-player.compiled.js', () => ({ default: H.FakeShaka }));

interface FakeVideo {
  el: HTMLVideoElement;
  fire(type: string): void;
  setBuffered(ranges: [number, number][]): void;
  set(key: string, value: unknown): void;
  get(key: string): unknown;
  playCalls(): number;
}

function fakeVideo(init: Record<string, unknown> = {}): FakeVideo {
  const listeners = new Map<string, Set<EventListener>>();
  let ranges: [number, number][] = [];
  let plays = 0;
  const buffered = {
    get length() {
      return ranges.length;
    },
    start: (i: number) => ranges[i]?.[0] ?? 0,
    end: (i: number) => ranges[i]?.[1] ?? 0,
  };
  const v: Record<string, unknown> = {
    currentTime: 0,
    duration: Number.NaN,
    paused: true,
    volume: 1,
    muted: false,
    playbackRate: 1,
    readyState: 0,
    preload: '',
    src: '',
    buffered,
    play() {
      plays += 1;
      v.paused = false;
      return Promise.resolve();
    },
    load() {},
    removeAttribute(_n: string) {
      v.src = '';
    },
    addEventListener(t: string, fn: EventListener) {
      let s = listeners.get(t);
      if (!s) {
        s = new Set();
        listeners.set(t, s);
      }
      s.add(fn);
    },
    removeEventListener(t: string, fn: EventListener) {
      listeners.get(t)?.delete(fn);
    },
    ...init,
  };
  return {
    el: v as unknown as HTMLVideoElement,
    fire: (t) => {
      for (const fn of [...(listeners.get(t) ?? [])]) fn(new Event(t));
    },
    setBuffered: (r) => {
      ranges = r;
    },
    set: (k, val) => {
      v[k] = val;
    },
    get: (k) => v[k],
    playCalls: () => plays,
  };
}

function mkSetters(): MediaEventSetters {
  return {
    setCur: vi.fn(),
    setDur: vi.fn(),
    setBufEnd: vi.fn(),
    setPlaying: vi.fn(),
    setWaiting: vi.fn(),
    setVolume: vi.fn(),
    setMuted: vi.fn(),
    setRate: vi.fn(),
    setReady: vi.fn(),
  };
}

const item = { id: 'w1', stream: 'stream://w1', durationMs: 7_200_000 } as unknown as MovieView;
const tick = () => new Promise<void>((r) => setTimeout(r, 0));

afterEach(() => {
  H.hlsMasterUrl.mockClear();
  H.instances.length = 0;
  H.FakeHls.supported = true;
  H.shakaInstances.length = 0;
  H.FakeShaka.Player.supported = true;
  H.installAll.mockClear();
});

describe('bindMediaEvents', () => {
  it('reports the absolute position from the anchor + element clock', () => {
    const fv = fakeVideo();
    const s = mkSetters();
    bindMediaEvents(fv.el, item, s, 100);
    fv.set('currentTime', 12);
    fv.fire('timeupdate');
    expect(s.setCur).toHaveBeenCalledWith(112);
  });

  it('prefers the catalogue runtime for duration, else the element duration', () => {
    const fv = fakeVideo();
    const s = mkSetters();
    bindMediaEvents(fv.el, item, s, 0);
    fv.fire('durationchange');
    expect(s.setDur).toHaveBeenCalledWith(7200); // durationMs / 1000

    const fv2 = fakeVideo({ duration: 900 });
    const s2 = mkSetters();
    bindMediaEvents(fv2.el, { ...item, durationMs: 0 } as MovieView, s2, 10);
    fv2.fire('durationchange');
    expect(s2.setDur).toHaveBeenCalledWith(910); // baseSec + element duration
  });

  it('prefers the known (server-header) duration over the element clock', () => {
    // Unprobed catalog row (durationMs 0), but the server supplied 5885s: the
    // slider must span the whole movie, not the growing playlist's live edge.
    const fv = fakeVideo({ duration: 172 });
    const s = mkSetters();
    bindMediaEvents(fv.el, { ...item, durationMs: 0 } as MovieView, s, 0, 5_885_000);
    fv.fire('durationchange');
    expect(s.setDur).toHaveBeenCalledWith(5885);
  });

  it('reports the buffered end (anchor + last range), or 0 when empty', () => {
    const fv = fakeVideo();
    const s = mkSetters();
    bindMediaEvents(fv.el, item, s, 100);
    fv.fire('progress');
    expect(s.setBufEnd).toHaveBeenCalledWith(0);
    fv.setBuffered([
      [0, 30],
      [50, 80],
    ]);
    fv.fire('progress');
    expect(s.setBufEnd).toHaveBeenCalledWith(180); // 100 + 80
  });

  it('maps pause / waiting / playing / volume / rate events', () => {
    const fv = fakeVideo();
    const s = mkSetters();
    bindMediaEvents(fv.el, item, s, 0);
    fv.fire('pause');
    expect(s.setPlaying).toHaveBeenCalledWith(false);
    fv.fire('waiting');
    expect(s.setWaiting).toHaveBeenCalledWith(true);
    fv.fire('playing');
    expect(s.setWaiting).toHaveBeenCalledWith(false);
    fv.set('volume', 0.5);
    fv.set('muted', true);
    fv.fire('volumechange');
    expect(s.setVolume).toHaveBeenCalledWith(0.5);
    expect(s.setMuted).toHaveBeenCalledWith(true);
    fv.set('playbackRate', 1.5);
    fv.fire('ratechange');
    expect(s.setRate).toHaveBeenCalledWith(1.5);
  });

  it('ready-gates autoplay: plays once when ready+paused, then a play event latches', () => {
    const fv = fakeVideo();
    const s = mkSetters();
    bindMediaEvents(fv.el, item, s, 0);
    fv.fire('canplay'); // ready + paused -> autoplay
    expect(s.setReady).toHaveBeenCalledWith(true);
    expect(fv.playCalls()).toBe(1);
    fv.fire('play'); // latches "started" + reports playing
    expect(s.setPlaying).toHaveBeenCalledWith(true);
    fv.set('paused', true);
    fv.fire('canplay'); // already started -> no retry
    expect(fv.playCalls()).toBe(1);
  });

  it('cleanup detaches every listener', () => {
    const fv = fakeVideo();
    const s = mkSetters();
    const off = bindMediaEvents(fv.el, item, s, 0);
    off();
    fv.fire('timeupdate');
    fv.fire('pause');
    expect(s.setCur).not.toHaveBeenCalled();
    expect(s.setPlaying).not.toHaveBeenCalled();
  });
});

describe('attachMediaSource direct-play', () => {
  function base(over: Partial<AttachSourceOptions> = {}): AttachSourceOptions {
    return {
      v: fakeVideo().el,
      item,
      decision: { kind: 'direct', aacMaster: false } as EngineDecision,
      useNativeHls: false,
      useShaka: false,
      startSec: 0,
      audioRel: 0,
      hlsRef: { current: null },
      shakaRef: { current: null },
      setUseHls: vi.fn(),
      setReady: vi.fn(),
      ...over,
    };
  }

  it('points a bare <video> at the original stream and marks direct', () => {
    const fv = fakeVideo();
    const opts = base({ v: fv.el });
    const cleanup = attachMediaSource(opts);
    expect(opts.setUseHls).toHaveBeenCalledWith(false);
    expect(fv.get('src')).toBe('stream://w1');
    expect(fv.get('preload')).toBe('auto');
    cleanup();
    expect(fv.get('src')).toBe(''); // removed on teardown
  });

  it('resume-seeks to the absolute start once metadata is available', () => {
    const fv = fakeVideo({ readyState: 0 });
    attachMediaSource(base({ v: fv.el, startSec: 300 }));
    expect(fv.get('currentTime')).toBe(0); // waits for metadata
    fv.fire('loadedmetadata');
    expect(fv.get('currentTime')).toBe(300);
  });

  it('seeks immediately when the element already has metadata', () => {
    const fv = fakeVideo({ readyState: 1, currentTime: 0 });
    attachMediaSource(base({ v: fv.el, startSec: 300 }));
    expect(fv.get('currentTime')).toBe(300);
  });
});

describe('attachMediaSource HLS master', () => {
  function hlsOpts(over: Partial<AttachSourceOptions> = {}): AttachSourceOptions {
    return {
      v: fakeVideo().el,
      item,
      decision: { kind: 'web-mse', aacMaster: true } as EngineDecision,
      useNativeHls: false,
      useShaka: false,
      startSec: 600,
      audioRel: 2,
      hlsRef: { current: null },
      shakaRef: { current: null },
      setUseHls: vi.fn(),
      setReady: vi.fn(),
      ...over,
    };
  }

  it('native HLS points the element at the muxed master URL', () => {
    const fv = fakeVideo();
    const opts = hlsOpts({ v: fv.el, useNativeHls: true });
    attachMediaSource(opts);
    expect(opts.setUseHls).toHaveBeenCalledWith(true);
    expect(H.hlsMasterUrl).toHaveBeenCalledWith('w1', true, 600, 2);
    expect(fv.get('src')).toBe('hls:w1:true:600:2');
  });

  it('hls.js attaches the master and stores the instance, then destroys on cleanup', async () => {
    const fv = fakeVideo();
    const opts = hlsOpts({ v: fv.el });
    const cleanup = attachMediaSource(opts);
    await tick(); // dynamic import('hls.js') resolves
    expect(H.instances).toHaveLength(1);
    const inst = H.instances[0];
    expect(inst?.loadSource).toHaveBeenCalledWith('hls:w1:true:600:2');
    expect(inst?.attachMedia).toHaveBeenCalledWith(fv.el);
    expect(opts.hlsRef.current).toBe(inst);
    cleanup();
    expect(inst?.destroy).toHaveBeenCalled();
    expect(opts.hlsRef.current).toBeNull();
  });

  it('falls back to a native src when hls.js is unsupported', async () => {
    H.FakeHls.supported = false;
    const fv = fakeVideo();
    attachMediaSource(hlsOpts({ v: fv.el }));
    await tick();
    expect(H.instances).toHaveLength(0);
    expect(fv.get('src')).toBe('hls:w1:true:600:2');
  });
});

describe('attachMediaSource HLS master via Shaka', () => {
  function shakaOpts(over: Partial<AttachSourceOptions> = {}): AttachSourceOptions {
    return {
      v: fakeVideo().el,
      item,
      decision: { kind: 'web-mse', aacMaster: true } as EngineDecision,
      useNativeHls: false,
      useShaka: true,
      startSec: 600,
      audioRel: 2,
      hlsRef: { current: null },
      shakaRef: { current: null },
      setUseHls: vi.fn(),
      setReady: vi.fn(),
      ...over,
    };
  }

  it('installs polyfills, attaches and loads the muxed master, then destroys on cleanup', async () => {
    const fv = fakeVideo();
    const opts = shakaOpts({ v: fv.el });
    const cleanup = attachMediaSource(opts);
    expect(opts.setUseHls).toHaveBeenCalledWith(true);
    await tick(); // dynamic import + attach()/load() microtasks resolve
    expect(H.installAll).toHaveBeenCalled();
    expect(H.shakaInstances).toHaveLength(1);
    const inst = H.shakaInstances[0];
    expect(inst?.attach).toHaveBeenCalledWith(fv.el);
    expect(inst?.load).toHaveBeenCalledWith('hls:w1:true:600:2');
    // A generous forward buffer is configured (default bufferingGoal is only 10s).
    const cfg = inst?.configure.mock.calls[0]?.[0] as {
      streaming?: { bufferingGoal?: number };
    };
    expect(cfg?.streaming?.bufferingGoal).toBeGreaterThanOrEqual(60);
    cleanup();
    expect(inst?.destroy).toHaveBeenCalled();
  });

  // Shaka wins over Safari's native HLS when the user explicitly picks it.
  it('uses Shaka even when useNativeHls is set', async () => {
    const fv = fakeVideo();
    attachMediaSource(shakaOpts({ v: fv.el, useNativeHls: true }));
    await tick();
    expect(H.shakaInstances).toHaveLength(1);
    expect(fv.get('src')).toBe(''); // no native src attach
  });

  it('falls back to a native src when Shaka is unsupported', async () => {
    H.FakeShaka.Player.supported = false;
    const fv = fakeVideo();
    attachMediaSource(shakaOpts({ v: fv.el }));
    await tick();
    expect(H.shakaInstances).toHaveLength(0); // support check fails before construction
    expect(fv.get('src')).toBe('hls:w1:true:600:2');
  });
});
