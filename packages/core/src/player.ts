import type { LumaClient } from './api';
import { canDirectPlay, capabilities, type DirectPlayVerdict } from './hevc';
import type { MediaItem } from './types';

export interface AttachOptions {
  /** Resume position in milliseconds. */
  startMs?: number;
  autoplay?: boolean;
}

/**
 * Direct-play attach: point a <video> element at the server's range-streamed
 * original file. No MSE, no transcoding — the device decodes the source codec
 * (HEVC included) natively. Returns the playback verdict so the caller can warn
 * when the codec is unsupported.
 */
export function attachDirectPlay(
  video: HTMLVideoElement,
  client: LumaClient,
  item: MediaItem,
  opts: AttachOptions = {},
): DirectPlayVerdict {
  const verdict = canDirectPlay(item, capabilities());

  video.src = client.streamUrl(item.id);
  video.preload = 'auto';
  if (opts.startMs && opts.startMs > 0) {
    const seekTo = opts.startMs / 1000;
    const onLoaded = () => {
      try {
        video.currentTime = seekTo;
      } catch {
        /* ignore */
      }
      video.removeEventListener('loadedmetadata', onLoaded);
    };
    video.addEventListener('loadedmetadata', onLoaded);
  }
  if (opts.autoplay) {
    void video.play().catch(() => {
      /* autoplay may be blocked; caller can surface a Play button */
    });
  }
  return verdict;
}

/**
 * Preserve playback position (and play/pause state) across a source swap — e.g.
 * switching the audio track re-points `<video>` at a per-track HLS remux, which
 * resets `currentTime` to 0. Call it RIGHT AFTER assigning the new source,
 * passing the position/state captured from the old one (read them before the swap
 * — assigning `src` zeroes `currentTime`).
 *
 * The per-track remux is a growing HLS *event* playlist (no finite duration,
 * segments appear from 0 over a second or two). Seeking before the target is in
 * the playlist either does nothing (→ stuck at 0) or clamps to the buffered edge
 * and, if retried, bounces around as it grows (→ "random" teleport). So we POLL
 * `video.seekable` and issue exactly ONE seek once the target is actually
 * reachable, then resume — never fighting a later manual seek. A timeout gives up
 * gracefully (plays from wherever it is). Returns a cleanup for the effect teardown.
 */
export function restorePlaybackAfterSwap(
  video: HTMLVideoElement,
  resumeAt: number,
  wasPlaying: boolean,
  {
    tolerance = 1,
    pollMs = 150,
    timeoutMs = 20000,
  }: { tolerance?: number; pollMs?: number; timeoutMs?: number } = {},
): () => void {
  let done = false;
  let timer: ReturnType<typeof setInterval> | undefined;
  let giveUp: ReturnType<typeof setTimeout> | undefined;

  const cleanup = () => {
    done = true;
    if (timer) clearInterval(timer);
    if (giveUp) clearTimeout(giveUp);
  };
  const finish = () => {
    if (done) return;
    if (wasPlaying) {
      const p = video.play();
      if (p && typeof p.then === 'function') p.catch(() => undefined);
    }
    cleanup();
  };
  const tick = () => {
    if (done) return;
    if (resumeAt <= 0 || Math.abs(video.currentTime - resumeAt) <= tolerance) {
      finish();
      return;
    }
    const s = video.seekable;
    const end = s.length ? s.end(s.length - 1) : 0;
    if (end + 0.5 < resumeAt) return; // target not yet in the playlist — keep waiting
    try {
      video.currentTime = resumeAt; // one shot, now that it's reachable
    } catch {
      return; // not ready after all — next tick retries (no clamp/bounce)
    }
    finish();
  };

  timer = setInterval(tick, pollMs);
  giveUp = setTimeout(finish, timeoutMs);
  tick(); // try immediately (direct-play is seekable at once)
  return cleanup;
}

/** Format milliseconds as cinematic French runtime, e.g. "2h08" or "47min". */
export function formatRuntime(durationMs: number | null): string {
  if (!durationMs || durationMs <= 0) return '';
  const totalMin = Math.round(durationMs / 60000);
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  if (h <= 0) return `${m}min`;
  return `${h}h${m.toString().padStart(2, '0')}`;
}
