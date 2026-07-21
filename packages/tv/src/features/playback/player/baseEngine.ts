// Shared base for the three "video plane behind the page" native backends
// (Samsung AVPlay, Android ExoPlayer, desktop mpv). They all drive playback on
// the same model: a `direct` mode that opens the ORIGINAL file (absolute
// timeline) and a `master` mode that opens the server's HLS remux anchored at
// `baseSec` (its clock restarts at 0, so the absolute position is
// `baseSec + elSec`), with a one-shot direct->master fallback for a file the
// backend cannot demux. This class owns that shared state plus the identical
// source-URL / position / duration / fallback logic; each backend subclass adds
// only its own transport (open, seek, audio switch, teardown) via `reanchor`.

import type { KromaClient, MediaItem } from '@kroma/core';
import type { AudioFilterMode } from '@kroma/ui';
import type { EngineListeners, TvEngine } from '#tv/features/playback/player/engine';

/** Construction options common to every native backend. */
export interface EngineOptions {
  client: KromaClient;
  item: MediaItem;
  durationSec: number;
  /** Audio-relative rendition to select once loaded (0 = the first/default track). */
  initialRendition: number;
  /** Initial position (s): master anchor / direct start offset. */
  startSec: number;
  /** Open the original file directly (see each backend's module doc) instead of
   * the master. */
  direct: boolean;
  /** Audio filter / volume normalizer (§7) to apply from the first open, so a
   * persisted mode never plays its first seconds unfiltered. Default `off`. */
  audioFilter?: AudioFilterMode;
  listeners: EngineListeners;
}

/** In master mode, a native seek beyond this many seconds ahead of the current
 * position is assumed past the anchored remux's buffer/production edge, so we
 * re-anchor instead of stalling. Direct mode always seeks natively. */
export const NATIVE_SEEK_AHEAD = 60;

export abstract class BaseTvEngine implements TvEngine {
  abstract readonly kind: TvEngine['kind'];
  protected readonly client: KromaClient;
  protected readonly item: MediaItem;
  protected readonly listeners: EngineListeners;
  protected mode: 'direct' | 'master';
  /** One-shot guard: a failed direct attempt falls back to the master ONCE. */
  protected fellBack = false;
  protected durSec: number;
  protected baseSec = 0;
  protected elSec = 0;
  protected paused = false;
  protected destroyed = false;
  protected rendition: number;
  /** Current audio filter / volume normalizer mode (§7). */
  protected filter: AudioFilterMode;
  /** The master is open ONLY because the audio filter needs the server's DSP
   * (the backend has none); turning the filter off drops back to the direct
   * file, and a remux failure degrades rather than costing the title. */
  protected filterMaster = false;
  /** Force the server to TRANSCODE the audio to stereo AAC in the master. Set
   * (one-shot) after the device fails to decode the source audio (e.g. E-AC3 /
   * DTS / TrueHD with no hardware decoder), so the movie plays at the cost of
   * surround rather than not at all. */
  protected forceAac = false;
  /** Set on a re-anchor so playback resumes once the new source has loaded. */
  protected resumeOnLoad = false;

  protected constructor(opts: EngineOptions) {
    this.client = opts.client;
    this.item = opts.item;
    this.listeners = opts.listeners;
    this.durSec = opts.durationSec;
    this.rendition = opts.initialRendition;
    this.filter = opts.audioFilter ?? 'off';
    this.mode = opts.direct ? 'direct' : 'master';
    // Direct: an absolute timeline starting at `startSec`. Master: the remux is
    // anchored at `startSec` (its clock restarts at 0).
    if (this.mode === 'direct') {
      this.elSec = opts.startSec;
    } else {
      this.baseSec = opts.startSec;
    }
  }

  /** The source URL for the current mode (direct = the original file, absolute
   * timeline; master = the remux anchored at `baseSec` with the chosen audio). */
  protected sourceUrl(): string {
    return this.mode === 'direct'
      ? this.client.streamUrl(this.item.id)
      : this.client.hlsMasterUrl(this.item.id, this.forceAac, this.baseSec, this.rendition);
  }

  /** A prepare/playback failure. `audioUnsupported` = the device can't decode the
   * source audio track (the master must then transcode it to AAC). Otherwise a
   * direct attempt retries ONCE as the master at the same position (a file the
   * backend can't demux still plays, remuxed); a filter-forced master degrades
   * ONCE to the clean direct file; anything else is surfaced. */
  protected fail(audioUnsupported = false): void {
    if (this.destroyed) return;
    const pos = this.position();
    // Audio the device has no decoder for: open the AAC-transcoded master (once).
    // Works whether we were direct or on a copy-audio master that still carried
    // the undecodable track.
    if (audioUnsupported && !this.forceAac) {
      this.forceAac = true;
      this.fellBack = true; // an unrelated direct failure afterwards still errors
      this.mode = 'master';
      this.listeners.onWaiting();
      this.reanchor(pos);
      return;
    }
    if (this.mode === 'direct' && !this.fellBack) {
      this.fellBack = true;
      this.mode = 'master';
      this.listeners.onWaiting();
      this.reanchor(pos);
      return;
    }
    // This master exists only to apply the filter, and the title direct-plays
    // fine. A remux the server can't produce (no acompressor, an audio track
    // ffmpeg won't decode) must cost the FILTER, not the title.
    if (this.mode === 'master' && this.filterMaster && !this.fellBack) {
      this.fellBack = true; // one-shot: a direct failure after this still errors
      this.filterMaster = false;
      this.filter = 'off';
      this.mode = 'direct';
      this.listeners.onAudioFilterUnavailable?.();
      this.listeners.onWaiting();
      this.reanchor(pos);
      return;
    }
    this.listeners.onError();
  }

  position(): number {
    return this.baseSec + this.elSec;
  }
  duration(): number {
    return this.durSec;
  }
  isPaused(): boolean {
    return this.paused;
  }

  /** Reopen the current mode's source at `absSec` (master: a new anchor; direct:
   * a start/post-load seek, used by the direct->master fallback hand-off too). */
  protected abstract reanchor(absSec: number): void;

  abstract play(): void;
  abstract pause(): void;
  abstract bufferedEnd(): number;
  abstract seekTo(absSec: number): void;
  abstract setAudioRendition(rendition: number): void;
  abstract destroy(): void;
}
