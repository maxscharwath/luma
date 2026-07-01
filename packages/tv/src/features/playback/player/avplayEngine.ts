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

import type { LumaClient, MediaItem } from '@luma/core';
import {
  type AvplayApi,
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
  /** Initial position (s): master anchor / direct post-prepare seek. */
  startSec: number;
  /** Open the original file directly (see the module doc) instead of the master. */
  direct: boolean;
  listeners: EngineListeners;
}

/** In master mode, a native seek beyond this many seconds ahead of the current
 * position is assumed to be past AVPlay's buffer, so we re-anchor instead
 * (faster + no stall over a network mount). Direct mode always seeks natively. */
const NATIVE_SEEK_AHEAD = 60;

export class AvplayEngine implements TvEngine {
  readonly kind = 'avplay';
  private readonly api: AvplayApi;
  private readonly client: LumaClient;
  private readonly item: MediaItem;
  private readonly listeners: EngineListeners;
  private mode: 'direct' | 'master';
  /** One-shot guard: a failed direct attempt falls back to the master ONCE. */
  private fellBack = false;
  private durSec: number;
  private baseSec: number;
  private elSec = 0;
  private paused = false;
  private destroyed = false;
  private rendition: number;
  /** Set on a re-anchor so playback resumes once the new source is prepared. */
  private resumeOnPrepare = false;
  /** Direct mode: absolute position to seek to right after prepare (resume /
   * fallback hand-off), else null. */
  private pendingSeek: number | null = null;
  private readonly onVisibility: () => void;

  constructor(opts: AvplayOptions) {
    const api = getAvplay();
    if (!api) throw new Error('AVPlay unavailable');
    this.api = api;
    this.client = opts.client;
    this.item = opts.item;
    this.listeners = opts.listeners;
    this.durSec = opts.durationSec;
    this.mode = opts.direct ? 'direct' : 'master';
    this.rendition = opts.initialRendition;
    if (this.mode === 'direct') {
      this.baseSec = 0;
      this.elSec = opts.startSec;
      this.pendingSeek = opts.startSec > 0.5 ? opts.startSec : null;
    } else {
      this.baseSec = opts.startSec;
    }
    this.onVisibility = () => this.handleVisibility();
    if (typeof document !== 'undefined') {
      document.addEventListener('visibilitychange', this.onVisibility);
    }
    this.open();
  }

  /** The source URL for the current mode. Direct = the original file (absolute
   * timeline, no anchor); master = the remux anchored at `baseSec` with the
   * selected audio muxed in (TV decodes natively → copy master). */
  private sourceUrl(): string {
    return this.mode === 'direct'
      ? this.client.streamUrl(this.item.id)
      : this.client.hlsMasterUrl(this.item.id, false, this.baseSec, this.rendition);
  }

  /** (Re)open the current source and prepare it. */
  private open(): void {
    try {
      this.api.open(this.sourceUrl());
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

  /** A prepare/playback failure: a direct attempt retries ONCE as the master at
   * the same position (a file AVPlay can't demux still plays, remuxed); a
   * master failure is surfaced. */
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
    if (this.resumeOnPrepare) {
      this.resumeOnPrepare = false;
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
  private reanchor(absSec: number): void {
    this.resumeOnPrepare = !this.paused;
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
