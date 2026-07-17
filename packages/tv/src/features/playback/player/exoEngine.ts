// Native media3/ExoPlayer backend for the @kroma/androidtv shell, in one of two
// source modes (the same shape as the AVPlay and mpv backends):
//
//  - `direct`: ExoPlayer opens the ORIGINAL file URL (`/api/items/:id/stream`,
//    plain HTTP Range). It demuxes MKV/MP4/TS, hardware-decodes HEVC and lets
//    the platform decode surround audio, so the server does nothing but send
//    bytes. Seeks are native and absolute; audio languages switch IN PLACE via a
//    track-selection override. Default; a load error falls back (once) to the
//    master at the current position.
//
//  - `master`: the server's stream-copy HLS remux, for the rare file ExoPlayer
//    cannot open. Anchored at `baseSec` (server input `-ss`), so the stream
//    restarts at 0 and absolute position is `baseSec + player position`; a far
//    seek or a language switch re-anchors (the master carries ONE audio track).
//
// ExoPlayer renders to a SurfaceView BEHIND the transparent WebView (the same
// "video plane behind the page" model as AVPlay/mpv), so this backend has no
// in-page media element (surface: 'exo'); the HTML chrome + subtitle overlay
// sit on top. Events arrive through the global `__kromaExoEvent` callback the
// Kotlin side invokes with a JSON payload.

import type { KromaClient, MediaItem } from '@kroma/core';
import {
  type EngineListeners,
  type ExoShellBridge,
  getExo,
  resolveMasterStart,
  type TvEngine,
} from '#tv/features/playback/player/engine';

export interface ExoOptions {
  client: KromaClient;
  item: MediaItem;
  durationSec: number;
  /** Audio-relative rendition to select once loaded (0 = the first/default track). */
  initialRendition: number;
  /** Initial position (s): master anchor / direct start offset. */
  startSec: number;
  /** Open the original file directly (see the module doc) instead of the master. */
  direct: boolean;
  listeners: EngineListeners;
}

/** A native seek beyond this many seconds ahead of the current position (master
 * mode only) is assumed past the anchored remux's production edge, so we
 * re-anchor instead of stalling there. Direct mode always seeks natively. */
const NATIVE_SEEK_AHEAD = 60;

/** Event payload pushed by the Kotlin bridge. */
interface ExoEvent {
  t: string;
  sec?: number;
  playing?: boolean;
  active?: boolean;
  message?: string;
}

type ExoEventGlobal = { __kromaExoEvent?: (e: ExoEvent) => void };

export class ExoEngine implements TvEngine {
  readonly kind = 'exo';
  private readonly bridge: ExoShellBridge;
  private readonly client: KromaClient;
  private readonly item: MediaItem;
  private readonly listeners: EngineListeners;
  private mode: 'direct' | 'master';
  /** One-shot guard: a failed direct attempt falls back to the master ONCE. */
  private fellBack = false;
  private durSec: number;
  private baseSec: number;
  private elSec = 0;
  private bufSec = 0;
  private paused = true;
  private destroyed = false;
  private rendition: number;
  /** Set on a re-anchor so playback resumes once the new source has loaded. */
  private resumeOnLoad = false;

  constructor(opts: ExoOptions) {
    const bridge = getExo();
    if (!bridge) throw new Error('ExoPlayer bridge unavailable');
    this.bridge = bridge;
    this.client = opts.client;
    this.item = opts.item;
    this.listeners = opts.listeners;
    this.durSec = opts.durationSec;
    this.mode = opts.direct ? 'direct' : 'master';
    this.rendition = opts.initialRendition;
    if (this.mode === 'direct') {
      this.baseSec = 0;
      this.elSec = opts.startSec;
    } else {
      this.baseSec = opts.startSec;
    }
    (globalThis as ExoEventGlobal).__kromaExoEvent = (e) => {
      if (!this.destroyed) this.onEvent(e);
    };
    this.open();
  }

  private cmd(op: string, value?: number): void {
    this.bridge.command(JSON.stringify(value === undefined ? { op } : { op, value }));
  }

  /** An event pushed by the Kotlin ExoPlayer bridge. */
  private onEvent(e: ExoEvent): void {
    switch (e.t) {
      case 'ready':
        this.onLoaded();
        break;
      case 'time':
        if (typeof e.sec === 'number') {
          this.elSec = e.sec;
          this.listeners.onTime(this.position());
        }
        break;
      case 'duration':
        // Direct mode: the player's duration is the real absolute runtime. Master
        // mode: the remux restarts at 0, so it is only the remaining tail - keep
        // the catalogue total.
        if (typeof e.sec === 'number' && e.sec > 0 && this.mode === 'direct') {
          this.durSec = e.sec;
          this.listeners.onDuration(this.durSec);
        }
        break;
      case 'buffered':
        if (typeof e.sec === 'number') {
          this.bufSec = e.sec;
          this.listeners.onBuffered(this.baseSec + e.sec);
        }
        break;
      case 'state':
        this.paused = e.playing !== true;
        if (this.paused) this.listeners.onPause();
        else this.listeners.onPlay();
        break;
      case 'waiting':
        if (e.active === true) this.listeners.onWaiting();
        else this.listeners.onPlaying();
        break;
      case 'ended':
        this.listeners.onEnded();
        break;
      case 'error':
        this.fail();
        break;
    }
  }

  /** The player finished preparing: apply the audio track, announce ready (the
   * hook drives the first play), and resume after a re-anchor. */
  private onLoaded(): void {
    if (this.mode === 'direct') this.cmd('audio', this.rendition);
    else this.elSec = 0;
    this.listeners.onDuration(this.durSec);
    this.listeners.onReady();
    if (this.resumeOnLoad) {
      this.resumeOnLoad = false;
      this.play();
    }
  }

  private fail(): void {
    if (this.destroyed) return;
    if (this.mode === 'direct' && !this.fellBack) {
      this.fellBack = true;
      const pos = this.position();
      this.mode = 'master';
      this.listeners.onWaiting();
      this.reanchor(pos);
      return;
    }
    this.listeners.onError();
  }

  /** The source URL for the current mode (direct = original file, absolute
   * timeline; master = the remux anchored at `baseSec` with the chosen audio,
   * stream-copied: the platform decodes surround itself). */
  private sourceUrl(): string {
    return this.mode === 'direct'
      ? this.client.streamUrl(this.item.id)
      : this.client.hlsMasterUrl(this.item.id, false, this.baseSec, this.rendition);
  }

  /** (Re)load the current source. An anchored master first resolves its REAL
   * start (the keyframe the server actually seeked to) so `baseSec` and every
   * absolute-time consumer stay honest; direct sources open at once. */
  private open(): void {
    const url = this.sourceUrl();
    if (this.mode === 'master') {
      if (this.baseSec <= 0.5) {
        this.bridge.load(url, 0, true);
        return;
      }
      void resolveMasterStart(url, this.baseSec).then((real) => {
        if (this.destroyed) return;
        this.baseSec = real;
        this.bridge.load(url, 0, true);
      });
      return;
    }
    this.bridge.load(url, this.elSec, false);
  }

  /** Reopen the current mode's source at `absSec` (master: a new anchor; direct:
   * a start offset, used by the direct→master fallback hand-off too). */
  private reanchor(absSec: number): void {
    this.resumeOnLoad = !this.paused;
    if (this.mode === 'direct') {
      this.baseSec = 0;
      this.elSec = absSec;
    } else {
      this.baseSec = absSec;
      this.elSec = 0;
    }
    this.open();
  }

  play(): void {
    this.cmd('play');
    this.paused = false;
    this.listeners.onPlay();
  }
  pause(): void {
    this.cmd('pause');
    this.paused = true;
    this.listeners.onPause();
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
    return this.baseSec + Math.max(this.elSec, this.bufSec);
  }

  seekTo(absSec: number): void {
    if (this.mode === 'direct') {
      // The original file is one fully-seekable VOD: every seek is native+absolute.
      this.elSec = Math.max(0, absSec);
      this.cmd('seek', this.elSec);
      return;
    }
    if (absSec >= this.baseSec && absSec <= this.position() + NATIVE_SEEK_AHEAD) {
      this.elSec = absSec - this.baseSec;
      this.cmd('seek', Math.max(0, absSec - this.baseSec));
      return;
    }
    this.reanchor(absSec);
  }

  setAudioRendition(rendition: number): void {
    if (rendition === this.rendition) return;
    this.rendition = rendition;
    // Direct: an in-place native track switch (picture never stops). Master: the
    // stream carries only the ONE audio track named in its URL, so reopen it at
    // the current position with the new track.
    if (this.mode === 'direct') {
      this.cmd('audio', rendition);
      return;
    }
    this.reanchor(this.position());
  }

  destroy(): void {
    this.destroyed = true;
    const g = globalThis as ExoEventGlobal;
    if (g.__kromaExoEvent) delete g.__kromaExoEvent;
    this.cmd('stop');
  }
}
