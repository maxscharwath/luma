// Native mpv backend for the @kroma/desktop shell (Steam Deck the primary target),
// in one of two source modes (the same shape as the Tizen AVPlay backend):
//
//  - `direct`: mpv opens the ORIGINAL file URL (`/api/items/:id/stream`, plain
//    HTTP Range). mpv demuxes any container and hardware-decodes video + surround
//    audio via VA-API on the Deck's APU, so the server does nothing but send
//    bytes. Seeks are native and absolute; audio languages switch IN PLACE via the
//    `aid` property. This is the default; a load error falls back (once) to the
//    master at the current position.
//
//  - `master`: the server's HLS remux master, for the rare file mpv cannot demux.
//    Anchored at `baseSec` (server input `-ss`) so a resume / far seek starts fast
//    over a network mount. The stream restarts at 0, so absolute position is
//    `baseSec + mpv time-pos`; a nearby seek is native, a far one re-anchors, and
//    a language switch re-anchors (the master carries only the ONE audio track in
//    its URL).
//
// mpv renders to its OWN native window behind the transparent Tauri UI window
// (the desktop shell floats the always-on-top web UI over it), the same "video
// plane behind the page" model AVPlay uses on Tizen. So the player shows no in-page
// media element for this backend (surface: 'mpv'); the HTML chrome + subtitle
// overlay sit on top.

import type { PlaneRect } from '@kroma/ui';
import {
  BaseTvEngine,
  type EngineOptions,
  NATIVE_SEEK_AHEAD,
} from '#tv/features/playback/player/baseEngine';
import {
  getTauri,
  resolveMasterStart,
  type TauriBridge,
} from '#tv/features/playback/player/engine';

/** A single mpv IPC command: `{"command": args}` (fire-and-forget). */
type MpvArg = string | number | boolean;

export class MpvEngine extends BaseTvEngine {
  readonly kind = 'mpv';
  private readonly bridge: TauriBridge;
  private cacheSec = 0;
  /** mpv's own track ids for the audio streams, in file order (from the observed
   * `track-list`); the array index is the audio-relative rendition. Empty until the
   * list arrives, then a rendition maps to the RIGHT track even when mpv's ids are
   * not a simple 1,2,3… (embedded fonts/attachments, cover art, etc. take ids too). */
  private audioIds: number[] = [];
  /** Direct mode: absolute position to seek to once the file loads (resume /
   * fallback hand-off), else null. */
  private pendingSeek: number | null = null;
  private readonly unlisten: Array<() => void> = [];

  constructor(opts: EngineOptions) {
    super(opts);
    const bridge = getTauri();
    if (!bridge) throw new Error('mpv bridge unavailable');
    this.bridge = bridge;
    if (this.mode === 'direct') {
      this.pendingSeek = opts.startSec > 0.5 ? opts.startSec : null;
    }
  }

  /** Subscribe to the shell's mpv events and open the source. Called once right
   * after construction, kept out of the constructor so it holds no async work. */
  start(): void {
    void this.subscribe();
    this.open();
  }

  // ----- Tauri command helpers (all fire-and-forget; state comes from events) --

  /** Send one mpv IPC command (`{"command": args}`) to the native process. */
  private cmd(...args: MpvArg[]): void {
    void this.bridge.core.invoke('mpv_command', { args }).catch(() => undefined);
  }

  /** `set_property name value`. */
  private setProp(name: string, value: MpvArg): void {
    this.cmd('set_property', name, value);
  }

  /** Load a URL into mpv (replaces the current file), optionally starting at `start`
   * seconds so mpv seeks DURING the open (resume) instead of buffering at 0 first. */
  private load(url: string, start = 0): void {
    void this.bridge.core.invoke('mpv_load', { url, start }).catch(() => this.fail());
  }

  /** Subscribe to the observed-property + lifecycle events the shell forwards. */
  private async subscribe(): Promise<void> {
    const on = async (event: string, cb: (payload: unknown) => void) => {
      const un = await this.bridge.event.listen(event, (e) => {
        if (!this.destroyed) cb(e.payload);
      });
      if (this.destroyed) un();
      else this.unlisten.push(un);
    };
    await Promise.all([
      on('mpv://property', (p) => this.onProperty(p as { name: string; data: unknown })),
      on('mpv://file-loaded', () => this.onLoaded()),
      on('mpv://end-file', (p) => this.onEndFile(p as { reason?: string })),
      // The mpv process itself is gone (crashed, killed, never became reachable):
      // no direct→master fallback can help, surface the error immediately.
      on('mpv://error', () => this.fatal()),
      on('mpv://exited', () => this.fatal()),
    ]);
    // mpv may already have died (or never launched: missing binary) BEFORE this
    // engine subscribed, in which case no event will ever arrive - probe once so a
    // dead process fails fast instead of leaving an endless spinner. The command
    // only exists on the Linux shell; elsewhere the invoke rejects and we rely on
    // the player's load watchdog.
    const status = await this.bridge.core.invoke('mpv_status').catch(() => null);
    if (status === 'dead') this.fatal();
  }

  /** Fail without the direct→master retry: the mpv process itself is unusable. */
  private fatal(): void {
    if (this.destroyed) return;
    this.fellBack = true;
    this.listeners.onError();
  }

  /** An observed mpv property changed. */
  private onProperty(p: { name: string; data: unknown }): void {
    switch (p.name) {
      case 'time-pos': {
        if (typeof p.data === 'number') {
          this.elSec = p.data;
          this.listeners.onTime(this.position());
        }
        break;
      }
      case 'duration': {
        this.onDurationProp(p.data);
        break;
      }
      case 'demuxer-cache-time': {
        if (typeof p.data === 'number') {
          this.cacheSec = p.data;
          this.listeners.onBuffered(this.baseSec + p.data);
        }
        break;
      }
      case 'pause': {
        this.paused = p.data === true;
        if (this.paused) this.listeners.onPause();
        else this.listeners.onPlay();
        break;
      }
      case 'paused-for-cache': {
        if (p.data === true) this.listeners.onWaiting();
        else this.listeners.onPlaying();
        break;
      }
      case 'track-list': {
        this.onTrackList(p.data);
        break;
      }
    }
  }

  /** Direct mode: mpv's duration is the real absolute runtime; prefer it over the
   * catalogue value. Master mode: the remux restarts at 0, so mpv's duration is the
   * REMAINING tail from the anchor - keep the catalogue total. */
  private onDurationProp(data: unknown): void {
    if (typeof data === 'number' && data > 0 && this.mode === 'direct') {
      this.durSec = data;
      this.listeners.onDuration(this.durSec);
    }
  }

  /** The observed `track-list`: remember mpv's own audio track ids (in file order)
   * so a rendition maps to the right track, then re-assert the wanted one. */
  private onTrackList(data: unknown): void {
    if (!Array.isArray(data)) return;
    const audio = (data as Array<{ id?: number; type?: string }>).filter(
      (t) => t?.type === 'audio' && typeof t.id === 'number',
    );
    this.audioIds = audio.map((t) => t.id as number);
    // Re-assert the wanted track now that the real ids are known (idempotent).
    if (this.audioIds.length) this.selectAudio(this.rendition);
  }

  /** mpv finished loading a file: apply the resume seek + audio track, announce
   * ready (the hook drives the first play), and resume after a re-anchor. */
  private onLoaded(): void {
    if (this.mode === 'direct') {
      const target = this.pendingSeek;
      this.pendingSeek = null;
      if (target != null) {
        this.elSec = target;
        this.cmd('seek', target, 'absolute');
      }
      this.selectAudio(this.rendition);
    } else {
      this.elSec = 0;
    }
    this.listeners.onDuration(this.durSec);
    this.listeners.onReady();
    if (this.resumeOnLoad) {
      this.resumeOnLoad = false;
      this.play();
    }
  }

  /** mpv closed the file: a natural end vs a decode/demux error (which, in direct
   * mode, retries ONCE as the stream-copy master at the same position). */
  private onEndFile(p: { reason?: string }): void {
    if (this.destroyed) return;
    if (p.reason === 'eof') {
      this.listeners.onEnded();
      return;
    }
    if (p.reason === 'error') this.fail();
  }

  /** (Re)load the current source. An anchored master first resolves its REAL
   * start (the keyframe the server actually seeked to) so `baseSec` and every
   * absolute-time consumer stay honest; direct sources open at once. */
  private open(): void {
    const url = this.sourceUrl();
    if (this.mode === 'master' && this.baseSec > 0.5) {
      // Master: the start offset is baked into the URL (server `-ss`), so just load.
      void resolveMasterStart(url, this.baseSec).then((real) => {
        if (this.destroyed) return;
        this.baseSec = real;
        this.load(url);
      });
      return;
    }
    // Direct: open the original file AT the current position so mpv seeks during load
    // (resume). `pendingSeek` remains as a safety net for mpv builds that ignore `start`.
    this.load(url, this.mode === 'direct' ? this.elSec : 0);
  }

  /** Reopen the current mode's source at `absSec` (master: a new anchor; direct:
   * a post-load seek, used by the direct→master fallback hand-off too). */
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
    this.open();
  }

  /** Select the Nth audio track in place. mpv assigns `aid` 1,2,3… to audio
   * streams in file order, so the audio-relative rendition R maps to `aid` R+1. */
  private selectAudio(rendition: number): void {
    // Map the audio-relative rendition to mpv's own audio track id via the observed
    // track-list; fall back to R+1 (mpv usually numbers audio tracks 1,2,3… in file
    // order) until the list has arrived.
    const id = this.audioIds[rendition];
    this.setProp('aid', id ?? rendition + 1);
  }

  play(): void {
    this.setProp('pause', false);
    this.paused = false;
    this.listeners.onPlay();
  }
  pause(): void {
    this.setProp('pause', true);
    this.paused = true;
    this.listeners.onPause();
  }
  bufferedEnd(): number {
    return this.baseSec + Math.max(this.elSec, this.cacheSec);
  }

  seekTo(absSec: number): void {
    if (this.mode === 'direct') {
      // The original file is one fully-seekable VOD: every seek is native+absolute.
      this.elSec = Math.max(0, absSec);
      this.cmd('seek', Math.max(0, absSec), 'absolute');
      return;
    }
    const here = this.position();
    if (absSec >= this.baseSec && absSec <= here + NATIVE_SEEK_AHEAD) {
      this.elSec = absSec - this.baseSec;
      this.cmd('seek', Math.max(0, absSec - this.baseSec), 'absolute');
      return;
    }
    this.reanchor(absSec);
  }

  setAudioRendition(rendition: number): void {
    if (rendition === this.rendition) return;
    this.rendition = rendition;
    // Direct: an in-place native track switch (picture never stops). Master: the
    // stream carries only the ONE audio track named in its URL, so reopen it at the
    // current position with the new track (re-preps in ~1s, resumes there).
    if (this.mode === 'direct') {
      this.selectAudio(rendition);
      return;
    }
    this.reanchor(this.position());
  }

  /** Shrink/restore the mpv plane by insetting the video with margin ratios (the
   * mpv window fills the screen behind the page, so a fraction-rect maps straight
   * to left/top/right/bottom margins; the video letterboxes inside the remainder). */
  setRect(rect: PlaneRect | null): void {
    const [l, t, r, b] = rect
      ? [rect.x, rect.y, Math.max(0, 1 - (rect.x + rect.w)), Math.max(0, 1 - (rect.y + rect.h))]
      : [0, 0, 0, 0];
    this.setProp('video-margin-ratio-left', l);
    this.setProp('video-margin-ratio-top', t);
    this.setProp('video-margin-ratio-right', r);
    this.setProp('video-margin-ratio-bottom', b);
  }

  destroy(): void {
    this.destroyed = true;
    for (const un of this.unlisten) un();
    this.unlisten.length = 0;
    // Keep the mpv process alive for the next item; just stop the current file so
    // it idles behind the UI (the shell kills the process on app exit).
    this.cmd('stop');
  }
}
