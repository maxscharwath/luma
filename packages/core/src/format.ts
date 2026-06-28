import { match } from './match';
import { formatRuntime } from './player';
import type { MediaItem, VideoTrack } from './types';

/** Deterministic two-stop key-art gradient derived from an item id. */
export function posterColors(id: string): [string, string] {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
  const hue = h % 360;
  const hue2 = (hue + 40) % 360;
  return [`hsl(${hue} 38% 26%)`, `hsl(${hue2} 50% 12%)`];
}

export function codecLabel(codec: string): string {
  switch (codec) {
    case 'hevc':
      return 'H.265';
    case 'h264':
      return 'H.264';
    case 'av1':
      return 'AV1';
    case 'vp9':
      return 'VP9';
    default:
      return codec.toUpperCase();
  }
}

/** Top-right quality badge text for a video track, or null. */
export function qualityBadgeForVideo(video: VideoTrack | null): string | null {
  if (!video) return null;
  return match(video)
    .when((v) => v.hdr === true, 'HDR')
    .when((v) => (v.width ?? 0) >= 3840, '4K')
    .when((v) => v.codec === 'hevc', 'H.265')
    .otherwise(null);
}

/** Top-right quality badge text for an item, or null. */
export function qualityBadge(item: MediaItem): string | null {
  return qualityBadgeForVideo(item.video);
}

/** Terse dot-separated metadata line, e.g. "2024 · 2h08 · H.265 · 4K". */
export function metaLine(item: MediaItem): string {
  const parts: string[] = [];
  if (item.year) parts.push(String(item.year));
  const rt = formatRuntime(item.durationMs);
  if (rt) parts.push(rt);
  if (item.video?.codec) parts.push(codecLabel(item.video.codec));
  if ((item.video?.width ?? 0) >= 3840) parts.push('4K');
  if (item.video?.hdr) parts.push('HDR');
  return parts.join(' · ');
}

/** Player scrub-bar timecode — "1:04:07" / "4:07" (no leading hours when < 1h). */
export function formatTimecode(s: number): string {
  if (!Number.isFinite(s) || s < 0) s = 0;
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = Math.floor(s % 60);
  const mm = h ? String(m).padStart(2, '0') : String(m);
  const hh = h ? `${h}:` : '';
  return `${hh}${mm}:${String(sec).padStart(2, '0')}`;
}

/** Two-letter language code for a track badge, e.g. "FR" (or "ST" when unknown). */
export function langCode(lang: string | null | undefined): string {
  if (!lang) return 'ST';
  return lang.slice(0, 2).toUpperCase();
}

/** Channel-layout label, e.g. 6 → "5.1", 2 → "2.0", 1 → "Mono". Null when unknown. */
export function channelLabel(ch: number | null | undefined): string | null {
  if (!ch) return null;
  if (ch <= 1) return 'Mono';
  if (ch === 2) return '2.0';
  if (ch === 6) return '5.1';
  if (ch === 8) return '7.1';
  return `${ch}.0`;
}
