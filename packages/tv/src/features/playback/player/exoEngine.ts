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

import type { AudioFilterMode, PlaneRect } from '@kroma/ui';
import {
  BaseTvEngine,
  type EngineOptions,
  NATIVE_SEEK_AHEAD,
} from '#tv/features/playback/player/baseEngine';
import {
  type ExoShellBridge,
  getExo,
  resolveMasterStart,
} from '#tv/features/playback/player/engine';

/** Bridge levels for `{op:'filter'}`: the Kotlin side maps them onto a
 * DynamicsProcessing compressor tuned to match the Web Audio graph (§7). */
const EXO_FILTER_LEVEL: Record<AudioFilterMode, number> = { off: 0, standard: 1, night: 2 };

/** Event payload pushed by the Kotlin bridge. */
interface ExoEvent {
  t: string;
  sec?: number;
  playing?: boolean;
  active?: boolean;
  supported?: boolean;
  message?: string;
  /** On `error`: the device could not decode the source AUDIO track (so the
   * fallback must transcode it to AAC), vs a demux/video failure. */
  audio?: boolean;
}

type ExoEventGlobal = { __kromaExoEvent?: (e: ExoEvent) => void };

export class ExoEngine extends BaseTvEngine {
  readonly kind = 'exo';
  private readonly bridge: ExoShellBridge;
  private bufSec = 0;

  constructor(opts: EngineOptions & { forceVlc?: boolean }) {
    super(opts);
    // ExoPlayer reports its playing/paused state via events; assume paused until
    // the first `state` event arrives.
    this.paused = true;
    const bridge = getExo();
    if (!bridge) throw new Error('ExoPlayer bridge unavailable');
    this.bridge = bridge;
    (globalThis as ExoEventGlobal).__kromaExoEvent = (e) => {
      if (!this.destroyed) this.onEvent(e);
    };
    // The "libVLC" engine: have the bridge software-decode every item from the
    // start (not just as a decode-failure fallback). Set the mode UNCONDITIONALLY
    // (before the first load()): the bridge is a long-lived singleton shared by
    // every engine instance, so switching AWAY from libVLC must actively reset it,
    // or a stale forceVlc would keep software-decoding later titles. An older APK
    // without setEngine stays ExoPlayer-first (harmless).
    this.bridge.setEngine?.(opts.forceVlc ? 'vlc' : 'exo');
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
      case 'duration':
      case 'buffered':
        if (typeof e.sec === 'number') this.onClock(e.t, e.sec);
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
      case 'filterSupported':
        // The bridge only learns the truth when it tries: no effect on this
        // ROM, audio passed straight through to an AVR, or construction threw.
        // Say so rather than leaving "Nuit" lit over untouched audio.
        if (e.supported === false) this.listeners.onAudioFilterUnavailable?.();
        break;
      case 'error':
        this.fail(e.audio === true);
        break;
    }
  }

  /** The three time-valued events, in player-clock seconds (master mode adds the
   * anchor back where the value must be absolute). */
  private onClock(t: string, sec: number): void {
    switch (t) {
      case 'time':
        this.elSec = sec;
        this.listeners.onTime(this.position());
        break;
      case 'buffered':
        this.bufSec = sec;
        this.listeners.onBuffered(this.baseSec + sec);
        break;
      case 'duration':
        // Direct mode: the player's duration is the real absolute runtime. Master
        // mode: the remux restarts at 0, so it is only the remaining tail - keep
        // the catalogue total.
        if (sec > 0 && this.mode === 'direct') {
          this.durSec = sec;
          this.listeners.onDuration(this.durSec);
        }
        break;
    }
  }

  /** API 28+ and a real (non-passthrough) audio session, as last seen by the
   * bridge. An APK predating the capability call keeps the old optimistic
   * answer, corrected later by the `filterSupported` event if it arrives. */
  audioFilterSupported(): boolean {
    return this.bridge.audioFilterSupported?.() ?? true;
  }

  /** The player finished preparing: apply the audio track + filter, announce
   * ready (the hook drives the first play), and resume after a re-anchor. */
  private onLoaded(): void {
    if (this.mode === 'direct') this.cmd('audio', this.rendition);
    else this.elSec = 0;
    // Unconditional (off included): the native player outlives engines, so a
    // leftover effect from the previous item must be cleared, not inherited.
    this.cmd('filter', EXO_FILTER_LEVEL[this.filter]);
    this.listeners.onDuration(this.durSec);
    this.listeners.onReady();
    if (this.resumeOnLoad) {
      this.resumeOnLoad = false;
      this.play();
    }
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
  protected reanchor(absSec: number): void {
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

  /** Toggle the native audio effect in place (the Kotlin side re-attaches its
   * DynamicsProcessing chain to the live audio session; playback never stops). */
  setAudioFilter(mode: AudioFilterMode): void {
    if (mode === this.filter) return;
    this.filter = mode;
    this.cmd('filter', EXO_FILTER_LEVEL[mode]);
  }

  /** Shrink/restore the plane: the Kotlin side resizes + repositions the
   *  SurfaceView (a fraction-rect of the screen) behind the WebView; `{op:'rect'}`
   *  with no bounds restores fullscreen. */
  setRect(rect: PlaneRect | null): void {
    this.bridge.command(
      JSON.stringify(
        rect ? { op: 'rect', x: rect.x, y: rect.y, w: rect.w, h: rect.h } : { op: 'rect' },
      ),
    );
  }

  destroy(): void {
    this.destroyed = true;
    const g = globalThis as ExoEventGlobal;
    if (g.__kromaExoEvent) delete g.__kromaExoEvent;
    this.cmd('stop');
  }
}
