import type { KromaClient } from '@kroma/client';
import { canDirectPlay, capabilities, type DirectPlayVerdict } from './hevc';
import type { MediaItem } from '@kroma/client';

export interface AttachOptions {
  /** Resume position in milliseconds. */
  startMs?: number;
  autoplay?: boolean;
}

/**
 * Direct-play attach: point a <video> element at the server's range-streamed
 * original file. No MSE, no transcoding the device decodes the source codec
 * (HEVC included) natively. Returns the playback verdict so the caller can warn
 * when the codec is unsupported.
 */
export function attachDirectPlay(
  video: HTMLVideoElement,
  client: KromaClient,
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

/** Format milliseconds as cinematic French runtime, e.g. "2h08" or "47min". */
export function formatRuntime(durationMs: number | null | undefined): string {
  if (!durationMs || durationMs <= 0) return '';
  const totalMin = Math.round(durationMs / 60000);
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  if (h <= 0) return `${m}min`;
  return `${h}h${m.toString().padStart(2, '0')}`;
}
