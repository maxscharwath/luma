// Native Samsung AVPlay backend. Plays the server's HLS master with hardware
// decode (AC3/EAC3/DTS surround passthrough) and switches audio renditions in
// place. Renders to a video plane behind the page, so the player shows an
// `<object type="application/avplayer">` surface (transparent body) and the HTML
// chrome + subtitle overlay sit on top.
//
// The remux is anchored at `baseSec` (server input `-ss`) so a resume/far-seek
// over a network mount starts fast. The stream restarts at 0, so absolute
// position is `baseSec + avplay time`; a nearby seek is a native `seekTo`, a far
// one re-anchors (reopen the master at the new offset).

import type { LumaClient, MediaItem } from '@luma/core';
import {
  type AvplayApi,
  type AvplayTrack,
  audioAbsoluteIndex,
  type EngineListeners,
  getAvplay,
  type TvEngine,
} from '#tv/features/playback/player/engine';

export interface AvplayOptions {
  client: LumaClient;
  item: MediaItem;
  durationSec: number;
  /** Audio-relative rendition to select once prepared (0 = the master default). */
  initialRendition: number;
  /** Initial remux anchor (s). */
  startSec: number;
  listeners: EngineListeners;
}

/** A native seek beyond this many seconds ahead of the current position is
 * assumed to be past AVPlay's buffer, so we re-anchor instead (faster + no stall
 * over a network mount). */
const NATIVE_SEEK_AHEAD = 60;

export class AvplayEngine implements TvEngine {
  readonly kind = 'avplay';
  private readonly api: AvplayApi;
  private readonly client: LumaClient;
  private readonly item: MediaItem;
  private readonly listeners: EngineListeners;
  private audioStreams: AvplayTrack[] = [];
  private durSec: number;
  private baseSec: number;
  private elSec = 0;
  private paused = false;
  private destroyed = false;
  private rendition: number;
  /** Set on a re-anchor so playback resumes once the new source is prepared. */
  private resumeOnPrepare = false;
  private readonly onVisibility: () => void;

  constructor(opts: AvplayOptions) {
    const api = getAvplay();
    if (!api) throw new Error('AVPlay unavailable');
    this.api = api;
    this.client = opts.client;
    this.item = opts.item;
    this.listeners = opts.listeners;
    this.durSec = opts.durationSec;
    this.baseSec = opts.startSec;
    this.rendition = opts.initialRendition;
    this.onVisibility = () => this.handleVisibility();
    if (typeof document !== 'undefined') {
      document.addEventListener('visibilitychange', this.onVisibility);
    }
    this.open();
  }

  /** (Re)open the master at the current `baseSec` and prepare it. */
  private open(): void {
    const url = this.client.hlsMasterUrl(this.item.id, false); // TV decodes natively → copy master
    try {
      this.api.open(url);
      this.api.setDisplayRect(0, 0, 1920, 1080);
      try {
        this.api.setStreamingProperty('ADAPTIVE_INFO', 'STARTBITRATE=HIGHEST|SKIPBITRATE=LOWEST');
      } catch {
        /* not all firmwares accept this */
      }
      try {
        this.api.setSilentSubtitle(true);
      } catch {
        /* optional we render our own overlay */
      }
      this.api.setListener({
        onbufferingstart: () => this.listeners.onWaiting(),
        onbufferingcomplete: () => this.listeners.onPlaying(),
        oncurrentplaytime: (ms: number) => {
          this.elSec = ms / 1000;
          this.listeners.onTime(this.baseSec + this.elSec);
          this.listeners.onBuffered(this.baseSec + this.elSec);
        },
        onstreamcompleted: () => this.listeners.onEnded(),
        onerror: () => this.listeners.onError(),
      });
      this.api.prepareAsync(
        () => this.onPrepared(),
        () => this.listeners.onError(),
      );
    } catch {
      this.listeners.onError();
    }
  }

  private onPrepared(): void {
    if (this.destroyed) return;
    this.elSec = 0;
    try {
      this.audioStreams = this.api.getTotalTrackInfo().filter((t) => t.type === 'AUDIO');
    } catch {
      this.audioStreams = [];
    }
    try {
      const d = this.api.getDuration();
      if (d > 0) this.durSec = d / 1000;
    } catch {
      /* keep the catalogue runtime */
    }
    this.listeners.onDuration(this.durSec);
    if (this.rendition > 0) this.applyRendition(this.rendition);
    this.listeners.onReady(); // the hook drives the FIRST playback start
    if (this.resumeOnPrepare) {
      this.resumeOnPrepare = false;
      this.play(); // a re-anchor resumes itself (the hook won't, already started)
    }
  }

  /** Map the audio-relative rendition to AVPlay's absolute stream index. */
  private applyRendition(rendition: number): void {
    const abs = audioAbsoluteIndex(this.audioStreams, rendition);
    if (abs == null) return;
    try {
      this.api.setSelectTrack('AUDIO', abs);
    } catch {
      /* track switch can fail in a transient state */
    }
  }

  private handleVisibility(): void {
    if (this.destroyed) return;
    try {
      if (document.visibilityState === 'hidden') this.api.suspend();
      else
        this.api.restore(
          this.client.hlsMasterUrl(this.item.id, false),
          Math.round(this.elSec * 1000),
          'PLAYING',
        );
    } catch {
      /* best effort */
    }
  }

  play(): void {
    try {
      this.api.play();
      this.paused = false;
      this.listeners.onPlay();
    } catch {
      /* ignore */
    }
  }
  pause(): void {
    try {
      this.api.pause();
      this.paused = true;
      this.listeners.onPause();
    } catch {
      /* ignore */
    }
  }
  isPaused(): boolean {
    return this.paused;
  }
  position(): number {
    return this.baseSec + this.elSec;
  }
  duration(): number {
    return this.durSec;
  }
  bufferedEnd(): number {
    return this.baseSec + this.elSec;
  }

  seekTo(absSec: number): void {
    const here = this.position();
    // Native within the current remux + its buffer; otherwise re-anchor.
    if (absSec >= this.baseSec && absSec <= here + NATIVE_SEEK_AHEAD) {
      this.elSec = absSec - this.baseSec;
      try {
        this.api.seekTo(Math.max(0, Math.round((absSec - this.baseSec) * 1000)));
      } catch {
        this.reanchor(absSec);
      }
      return;
    }
    this.reanchor(absSec);
  }

  /** Re-anchor the remux at `absSec` (reopen the master at `?t=absSec`). */
  private reanchor(absSec: number): void {
    this.resumeOnPrepare = !this.paused;
    this.baseSec = absSec;
    this.elSec = 0;
    try {
      this.api.stop();
    } catch {
      /* ignore */
    }
    try {
      this.api.close();
    } catch {
      /* ignore */
    }
    this.open();
    // onPrepared fires onReady; the hook restarts playback there.
  }

  setAudioRendition(rendition: number): void {
    this.rendition = rendition;
    if (this.audioStreams.length === 0) return; // applied on prepare
    this.applyRendition(rendition);
  }

  destroy(): void {
    this.destroyed = true;
    if (typeof document !== 'undefined') {
      document.removeEventListener('visibilitychange', this.onVisibility);
    }
    // Singleton hardware resource: stop THEN close, or the next open() fails.
    try {
      this.api.stop();
    } catch {
      /* ignore */
    }
    try {
      this.api.close();
    } catch {
      /* ignore */
    }
  }
}
