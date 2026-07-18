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
  /** Set on a re-anchor so playback resumes once the new source has loaded. */
  protected resumeOnLoad = false;

  protected constructor(opts: EngineOptions) {
    this.client = opts.client;
    this.item = opts.item;
    this.listeners = opts.listeners;
    this.durSec = opts.durationSec;
    this.rendition = opts.initialRendition;
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
      : this.client.hlsMasterUrl(this.item.id, false, this.baseSec, this.rendition);
  }

  /** A prepare/playback failure: a direct attempt retries ONCE as the master at
   * the same position (a file the backend can't demux still plays, remuxed); a
   * master failure is surfaced. */
  protected fail(): void {
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
