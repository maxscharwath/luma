// Maps an account's preferred audio/subtitle language onto a specific file's
// tracks. Media tracks carry free-form ISO codes (2- or 3-letter, e.g. "en" /
// "eng" / "fra"), so we normalise both sides to a 2-letter base before matching.

import type { AudioTrack } from '@luma/core';
import type { SubtitleView } from '#web/shared/lib/api';

/** Common ISO-639-2 → 639-1 aliases for the languages LUMA labels. */
const ALIAS: Record<string, string> = {
  fra: 'fr',
  fre: 'fr',
  eng: 'en',
  spa: 'es',
  ger: 'de',
  deu: 'de',
  ita: 'it',
  jpn: 'ja',
  kor: 'ko',
  zho: 'zh',
  chi: 'zh',
  rus: 'ru',
  por: 'pt',
  dut: 'nl',
  nld: 'nl',
};

/** Canonical 2-letter base for a language code (`"eng"` → `"en"`), or null. */
function langBase(code?: string | null): string | null {
  if (!code) return null;
  const c = code.toLowerCase();
  return ALIAS[c] ?? c.slice(0, 2);
}

/** Whether a track's language code matches a preferred code. */
export function matchesLang(pref: string, code?: string | null): boolean {
  const a = langBase(pref);
  return a != null && a === langBase(code);
}

/** Index of the audio track matching `pref`, or null when none does (caller
 * keeps the file's default). */
export function preferredAudioIndex(tracks: AudioTrack[], pref?: string | null): number | null {
  if (!pref) return null;
  const hit = tracks.find((tr) => matchesLang(pref, tr.language));
  return hit ? hit.index : null;
}

/** Index of the subtitle track to auto-enable for `pref`, or null. The `"off"`
 * sentinel and an absent preference both yield null (leave subtitles off). Only
 * selectable (text, has a URL) embedded tracks are considered AI-generated
 * tracks are never auto-picked. */
export function preferredSubIndex(subs: SubtitleView[], pref?: string | null): number | null {
  if (!pref || pref === 'off') return null;
  const hit = subs.find((s) => Boolean(s.url) && !s.downloaded && matchesLang(pref, s.language));
  return hit ? hit.index : null;
}
