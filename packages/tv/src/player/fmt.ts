// Player time + language formatting now live in @luma/core (shared with the web
// player). Re-exported here so the player modules keep their local import path.
import type { MessageKey, Translate } from '@luma/core';

export { formatTimecode as fmtTime, langCode } from '@luma/core';

// ISO-639 (2- and 3-letter) → `lang.*` catalog key. Mirrors the web client's map
// (#web/components/detail) so audio/subtitle track languages localize the same way.
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
 * null when there is no code at all. */
export function langName(t: Translate, code: string | null | undefined): string | null {
  if (!code) return null;
  const key = LANG_KEYS[code.toLowerCase()];
  return key ? t(key) : code.toUpperCase();
}
