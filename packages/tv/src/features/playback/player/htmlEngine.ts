// HTML `<video>` backend (+ hls.js for the master). Used by webOS and by any
// plain compatible MP4 on Tizen. Direct-play for a compatible single-audio MP4,
// else the HLS master.
//
// The HLS remux is anchored at `baseSec` (server input `-ss`), so over a network
// mount a resume/far-seek starts fast (ffmpeg seeks IN the file). The element
// restarts at 0, so absolute position is `baseSec + element time`. Seeking inside
// the BUFFERED range is an instant native seek; outside it we re-anchor (reload
// the master at the new offset) rather than stall at the production edge.

import { attachDirectPlay, type LumaClient, type MediaItem } from '@luma/core';
import type { EngineListeners, TvEngine } from '#tv/features/playback/player/engine';

type HlsInstance = import('hls.js').default;

export interface HtmlOptions {
  video: HTMLVideoElement;
  client: LumaClient;
  item: MediaItem;
  /** Plain direct-play (`<video src>`) vs the HLS master. */
  direct: boolean;
  /** When using the master, request the AAC renditions (MSE can't decode AC3). */
  masterAac: boolean;
  /** Audio-relative rendition to select once the manifest parses. */
  initialRendition: number;
  durationSec: number;
  /** Initial remux anchor (s). */
  startSec: number;
  listeners: EngineListeners;
}

export class HtmlEngine implements TvEngine {
  readonly kind = 'video';
  private readonly v: HTMLVideoElement;
  private readonly opts: HtmlOptions;
  private readonly durSec: number;
  private baseSec: number;
  private rendition: number;
  private hls: HlsInstance | null = null;
  private destroyed = false;
  private readonly cleanupEvents: () => void;

  constructor(opts: HtmlOptions) {
    this.opts = opts;
    this.v = opts.video;
    this.durSec = opts.durationSec;
    this.baseSec = opts.startSec;
    this.rendition = opts.initialRendition;
    const v = this.v;
    const L = opts.listeners;
    const total = opts.durationSec;

    const onTime = () => L.onTime(this.baseSec + v.currentTime);
    const onDur = () => {
      if (total > 0) L.onDuration(total);
      else if (Number.isFinite(v.duration)) L.onDuration(v.duration);
    };
    const onProg = () =>
      L.onBuffered(v.buffered.length ? this.baseSec + v.buffered.end(v.buffered.length - 1) : 0);
    const onPlay = () => L.onPlay();
    const onPause = () => L.onPause();
    const onWaiting = () => L.onWaiting();
    const onPlaying = () => L.onPlaying();
    const onEnded = () => L.onEnded();
    const onErr = () => L.onError();
    const onReady = () => L.onReady();

    const evs: [string, EventListener][] = [
      ['timeupdate', onTime],
      ['durationchange', onDur],
      ['progress', onProg],
      ['play', onPlay],
      ['pause', onPause],
      ['waiting', onWaiting],
      ['playing', onPlaying],
      ['ended', onEnded],
      ['error', onErr],
      ['loadedmetadata', onReady],
      ['loadeddata', onReady],
      ['canplay', onReady],
    ];
    for (const [t, fn] of evs) v.addEventListener(t, fn);
    this.cleanupEvents = () => {
      for (const [t, fn] of evs) v.removeEventListener(t, fn);
    };

    if (opts.direct) {
      attachDirectPlay(v, opts.client, opts.item, { autoplay: false });
      return;
    }
    this.attachMaster();
  }

  private attachMaster(): void {
    const v = this.v;
    const url = this.opts.client.hlsMasterUrl(this.opts.item.id, this.opts.masterAac);
    void import('hls.js').then(({ default: Hls }) => {
      if (this.destroyed) return;
      if (!Hls.isSupported()) {
        v.src = url; // last resort the hook's ready-gated play starts it
        v.preload = 'auto';
        return;
      }
      const hls = new Hls({ enableWorker: true, lowLatencyMode: false, startPosition: 0 });
      this.hls = hls;
      hls.on(Hls.Events.MANIFEST_PARSED, () => {
        if (this.rendition > 0) {
          try {
            hls.audioTrack = this.rendition;
          } catch {
            /* manifest race the default rendition is already correct */
          }
        }
      });
      hls.loadSource(url);
      hls.attachMedia(v);
    });
  }

  /** Re-anchor the remux at `absSec` (reload the master at `?t=absSec`), then
   * resume playback once the new source is ready. */
  private reanchor(absSec: number): void {
    const wasPlaying = !this.v.paused;
    this.baseSec = absSec;
    this.hls?.destroy();
    this.hls = null;
    this.v.removeAttribute('src');
    this.attachMaster();
    if (wasPlaying) this.v.addEventListener('canplay', () => this.play(), { once: true });
  }

  play(): void {
    const p = this.v.play();
    if (p && typeof p.then === 'function') p.catch(() => undefined);
  }
  pause(): void {
    this.v.pause();
  }
  isPaused(): boolean {
    return this.v.paused;
  }
  position(): number {
    return this.baseSec + this.v.currentTime;
  }
  duration(): number {
    if (this.durSec > 0) return this.durSec;
    return Number.isFinite(this.v.duration) ? this.v.duration : 0;
  }
  bufferedEnd(): number {
    return this.v.buffered.length
      ? this.baseSec + this.v.buffered.end(this.v.buffered.length - 1)
      : 0;
  }

  seekTo(absSec: number): void {
    if (this.opts.direct) {
      this.v.currentTime = absSec; // direct-play timeline is absolute
      return;
    }
    const rel = absSec - this.baseSec;
    let buffered = false;
    for (let i = 0; i < this.v.buffered.length; i += 1) {
      if (rel >= this.v.buffered.start(i) - 0.1 && rel <= this.v.buffered.end(i) - 0.3) {
        buffered = true;
        break;
      }
    }
    if (rel >= 0 && buffered) {
      this.v.currentTime = rel; // already downloaded: instant
      return;
    }
    this.reanchor(absSec);
  }

  setAudioRendition(rendition: number): void {
    this.rendition = rendition;
    if (!this.hls) return;
    try {
      this.hls.audioTrack = rendition;
    } catch {
      /* manifest not parsed yet the default rendition is already correct */
    }
  }

  destroy(): void {
    this.destroyed = true;
    this.cleanupEvents();
    this.hls?.destroy();
    this.hls = null;
    this.v.removeAttribute('src');
    this.v.load();
  }
}
