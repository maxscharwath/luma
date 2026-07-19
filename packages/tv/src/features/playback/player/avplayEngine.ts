// Native Samsung AVPlay backend, in one of two source modes:
//
//  - `direct`: AVPlay opens the ORIGINAL file URL (`/api/items/:id/stream`,
//    plain HTTP Range). The TV demuxes MKV/MP4 and hardware-decodes video +
//    surround audio itself, so the server does nothing but send bytes zero
//    ffmpeg, no remux session. Seeks are native and absolute; audio languages
//    switch IN PLACE via `setSelectTrack('AUDIO', …)`. This is the preferred
//    mode whenever `avplayDirectPlayable(item)` holds; a prepare/playback error
//    falls back (once) to the master at the current position.
//
//  - `master`: the server's HLS remux master, for files AVPlay cannot demux.
//    The remux is anchored at `baseSec` (server input `-ss`) so a resume/far
//    -seek over a network mount starts fast. The stream restarts at 0, so the
//    absolute position is `baseSec + avplay time`; a nearby seek is a native
//    `seekTo`, a far one re-anchors, and a language switch re-anchors too (the
//    stream carries only the ONE audio track named in its URL).
//
// Either way AVPlay renders to a video plane behind the page, so the player
// shows an `<object type="application/avplayer">` surface (transparent body)
// and the HTML chrome + subtitle overlay sit on top.

import type { PlaneRect } from '@kroma/ui';
import {
  BaseTvEngine,
  type EngineOptions,
  NATIVE_SEEK_AHEAD,
} from '#tv/features/playback/player/baseEngine';
import { type AvplayApi, getAvplay, resolveMasterStart } from '#tv/features/playback/player/engine';

/** AVPlay's display coordinate space is the app's fixed 1920x1080 canvas. */
const AVPLAY_W = 1920;
const AVPLAY_H = 1080;

export class AvplayEngine extends BaseTvEngine {
  readonly kind = 'avplay';
  private readonly api: AvplayApi;
  /** Direct mode: absolute position to seek to right after prepare (resume /
   * fallback hand-off), else null. */
  private pendingSeek: number | null = null;
  /** Current display rectangle (device px). Re-applied on every (re)open so a
   * shrunk plane survives a re-anchor / audio switch instead of popping back. */
  private displayRect = { x: 0, y: 0, w: AVPLAY_W, h: AVPLAY_H };
  private readonly onVisibility: () => void;

  constructor(opts: EngineOptions) {
    super(opts);
    const api = getAvplay();
    if (!api) throw new Error('AVPlay unavailable');
    this.api = api;
    if (this.mode === 'direct') {
      this.pendingSeek = opts.startSec > 0.5 ? opts.startSec : null;
    }
    this.onVisibility = () => this.handleVisibility();
    if (typeof document !== 'undefined') {
      document.addEventListener('visibilitychange', this.onVisibility);
    }
    this.open();
  }

  /** (Re)open the current source and prepare it. An anchored master first
   * resolves its REAL start (the keyframe the server actually seeked to) so
   * `baseSec` and every absolute-time consumer (progress bar, subtitle cues)
   * stay honest; direct sources have an absolute timeline and open at once. */
  private open(): void {
    const url = this.sourceUrl();
    if (this.mode === 'master' && this.baseSec > 0.5) {
      void resolveMasterStart(url, this.baseSec).then((real) => {
        if (this.destroyed) return;
        this.baseSec = real;
        this.openNow(url);
      });
      return;
    }
    this.openNow(url);
  }

  private openNow(url: string): void {
    try {
      this.api.open(url);
      const r = this.displayRect;
      this.api.setDisplayRect(r.x, r.y, r.w, r.h);
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
        onerror: () => this.fail(),
      });
      this.api.prepareAsync(
        () => this.onPrepared(),
        () => this.fail(),
      );
    } catch {
      this.fail();
    }
  }

  private onPrepared(): void {
    if (this.destroyed) return;
    try {
      const d = this.api.getDuration();
      if (d > 0) this.durSec = d / 1000;
    } catch {
      /* keep the catalogue runtime */
    }
    if (this.mode === 'direct') {
      // Resume / fallback hand-off: land on the target before frames roll.
      const target = this.pendingSeek;
      this.pendingSeek = null;
      if (target != null) {
        this.elSec = target;
        try {
          this.api.seekTo(Math.max(0, Math.round(target * 1000)));
        } catch {
          /* keep from 0 */
        }
      }
      // The container default may not be the wanted language; select explicitly.
      this.selectNativeAudio(this.rendition);
    } else {
      this.elSec = 0;
    }
    this.listeners.onDuration(this.durSec);
    this.listeners.onReady(); // the hook drives the FIRST playback start
    if (this.resumeOnLoad) {
      this.resumeOnLoad = false;
      this.play(); // a re-anchor resumes itself (the hook won't, already started)
    }
  }

  private handleVisibility(): void {
    if (this.destroyed) return;
    try {
      if (document.visibilityState === 'hidden') this.api.suspend();
      else {
        // Direct sources restore at the ABSOLUTE position; anchored masters at
        // the relative one (their clock restarts at the anchor).
        const ms = Math.round((this.mode === 'direct' ? this.position() : this.elSec) * 1000);
        this.api.restore(this.sourceUrl(), ms, 'PLAYING');
      }
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
  bufferedEnd(): number {
    return this.baseSec + this.elSec;
  }

  seekTo(absSec: number): void {
    if (this.mode === 'direct') {
      // The original file is one fully-seekable VOD: every seek is native.
      this.elSec = Math.max(0, absSec);
      try {
        this.api.seekTo(Math.max(0, Math.round(absSec * 1000)));
      } catch {
        /* transient (e.g. mid-prepare); the position state stays consistent */
      }
      return;
    }
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

  /** Reopen the current mode's source at `absSec` (master: a new anchor; direct:
   * a post-prepare seek used by the direct→master fallback hand-off too). */
  protected reanchor(absSec: number): void {
    this.resumeOnLoad = !this.paused;
    if (this.mode === 'direct') {
      this.baseSec = 0;
      this.elSec = absSec;
      this.pendingSeek = absSec > 0.5 ? absSec : null;
    } else {
      this.baseSec = absSec;
      this.elSec = 0;
    }
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

  /** Direct mode: select the Nth AUDIO track in place (audio-relative index →
   * AVPlay's internal track index). True when the switch took. */
  private selectNativeAudio(rendition: number): boolean {
    try {
      const audios = this.api.getTotalTrackInfo().filter((t) => t.type === 'AUDIO');
      const track = audios[rendition];
      if (!track) return false;
      this.api.setSelectTrack('AUDIO', track.index);
      return true;
    } catch {
      return false;
    }
  }

  /** Shrink/restore the hardware video plane (fraction-rect → 1920x1080 px). */
  setRect(rect: PlaneRect | null): void {
    const next = rect
      ? {
          x: Math.round(rect.x * AVPLAY_W),
          y: Math.round(rect.y * AVPLAY_H),
          w: Math.round(rect.w * AVPLAY_W),
          h: Math.round(rect.h * AVPLAY_H),
        }
      : { x: 0, y: 0, w: AVPLAY_W, h: AVPLAY_H };
    const p = this.displayRect;
    // Skip a redundant resize (the throttled tween settles on the same px for
    // several frames) - each setDisplayRect hits the hardware compositor.
    if (next.x === p.x && next.y === p.y && next.w === p.w && next.h === p.h) return;
    this.displayRect = next;
    try {
      this.api.setDisplayRect(next.x, next.y, next.w, next.h);
    } catch {
      /* transient (mid-prepare); re-applied on the next open() */
    }
  }

  setAudioRendition(rendition: number): void {
    if (rendition === this.rendition) return;
    this.rendition = rendition;
    // Direct: an in-place native track switch (picture never stops). Master: the
    // stream carries only the ONE audio track named in its URL (the server maps a
    // single `0:a:<n>` per session), so a language switch reopens the master at
    // the CURRENT position with the new track (re-preps in ~1s, resumes there).
    if (this.mode === 'direct' && this.selectNativeAudio(rendition)) return;
    this.reanchor(this.position());
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
