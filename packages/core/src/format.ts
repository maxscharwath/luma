import type { MessageKey, Translate } from './i18n';
import { match } from './match';
import { formatRuntime } from './player';
import type { AudioTrack, MediaItem, VideoTrack } from '@luma/client';

/** Request a downscaled rendition of LOCALLY-CACHED artwork (`?w=`, snapped to
 * a server-side bucket): a 200px card must not download the full 780px poster.
 * Pass the DISPLAY width; this asks for 2x for crisp hidpi/TV rendering. Remote
 * (TMDB fallback) URLs and non-image URLs pass through untouched. */
export function sizedImageUrl(url: string | null | undefined, displayWidth: number): string | null {
  if (!url) return null;
  if (!url.includes('/api/images/') || url.includes('?')) return url;
  return `${url}?w=${Math.max(1, Math.round(displayWidth * 2))}`;
}

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
export function qualityBadgeForVideo(video: VideoTrack | null | undefined): string | null {
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

/** Zero-padded season/episode tag, e.g. "S01E05" (or a range "S01E05-E06" for a
 * multi-episode file). Empty string when the item carries no episode numbering. */
export function episodeTag(
  item: Pick<MediaItem, 'season' | 'episode'> & { episodeEnd?: number | null },
): string {
  if (item.season == null || item.episode == null) return '';
  const pad = (n: number) => String(n).padStart(2, '0');
  const base = `S${pad(item.season)}E${pad(item.episode)}`;
  return item.episodeEnd != null && item.episodeEnd > item.episode
    ? `${base}-E${pad(item.episodeEnd)}`
    : base;
}

/** The player header's secondary line: for an episode "Show · S01E05" (the parts
 * that exist), else the movie meta line "2024 · 2h08 · H.265 · 4K". */
export function playerSubtitle(item: MediaItem): string {
  if (item.kind === 'episode') {
    const parts = [item.showTitle, episodeTag(item)].filter(Boolean);
    if (parts.length) return parts.join(' · ');
  }
  return metaLine(item);
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

/** Player scrub-bar timecode "1:04:07" / "4:07" (no leading hours when < 1h). */
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

/** ISO 639 code (2- or 3-letter) → the `lang.*` catalog key for its native name. */
const LANG_KEYS: Record<string, MessageKey> = {
  fr: 'lang.fr',
  fra: 'lang.fr',
  fre: 'lang.fr',
  en: 'lang.en',
  eng: 'lang.en',
  es: 'lang.es',
  spa: 'lang.es',
  de: 'lang.de',
  ger: 'lang.de',
  deu: 'lang.de',
  it: 'lang.it',
  ita: 'lang.it',
  ja: 'lang.ja',
  jpn: 'lang.ja',
  ko: 'lang.ko',
  kor: 'lang.ko',
  zh: 'lang.zh',
  zho: 'lang.zh',
  chi: 'lang.zh',
  ru: 'lang.ru',
  rus: 'lang.ru',
  pt: 'lang.pt',
  por: 'lang.pt',
  nl: 'lang.nl',
  dut: 'lang.nl',
  nld: 'lang.nl',
};

/** Localized language name for an ISO code, the upper-cased code if unknown, or
 * null when there is no code at all. Shared by every client (audio/subtitle track
 * labels localize identically). */
export function langName(t: Translate, code: string | null | undefined): string | null {
  if (!code) return null;
  const key = LANG_KEYS[code.toLowerCase()];
  return key ? t(key) : code.toUpperCase();
}

/** Concise label for the audio track a viewer has selected, language first:
 * "Français · 5.1 · EAC3" (a stream `title` tag wins over the language name).
 * Fed to the playback heartbeat so the admin dashboard reflects the chosen
 * track, not the file's default. Returns undefined when there is no track. */
export function audioTrackLabel(
  t: Translate,
  track: AudioTrack | null | undefined,
): string | undefined {
  if (!track) return undefined;
  const name = track.title?.trim() || langName(t, track.language) || undefined;
  const codec = track.codec ? track.codec.toUpperCase() : undefined;
  const label = [name, channelLabel(track.channels), codec].filter(Boolean).join(' · ');
  return label || undefined;
}
