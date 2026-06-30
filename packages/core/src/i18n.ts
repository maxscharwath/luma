// Shared, framework-agnostic i18n core. The JSON catalogs in ./locales are the
// single source of truth for every LUMA surface they are bundled into the TS
// clients here AND `include_str!`'d by the Rust server (see server/src/i18n.rs),
// so message keys stay in lockstep across the whole stack.
//
// React bindings (provider + hooks) live in `@luma/ui` to keep this package
// React-free; the server has its own tiny mirror of `translate`.

import en from './locales/en.json';
import fr from './locales/fr.json';

/** Supported UI locales. `fr` is the project default. */
export type Locale = 'fr' | 'en';

/** Every translatable key. `fr.json` is the authoritative key set; other locales
 * may lag and fall back to French (then to the raw key). */
export type MessageKey = keyof typeof fr;

/** Interpolation values for `{placeholder}` tokens in a message. */
export type TVars = Record<string, string | number>;

/** A bound translation function for one locale. */
export type Translate = (key: MessageKey, vars?: TVars) => string;

export const DEFAULT_LOCALE: Locale = 'fr';

/** Ordered list of selectable locales, with the key for their native label. */
export const LOCALES: ReadonlyArray<{ code: Locale; labelKey: MessageKey }> = [
  { code: 'fr', labelKey: 'lang.fr' },
  { code: 'en', labelKey: 'lang.en' },
];

const CATALOGS: Record<Locale, Record<string, string>> = { fr, en };

/** Narrow an arbitrary value to a supported {@link Locale}. */
export function isLocale(value: unknown): value is Locale {
  return value === 'fr' || value === 'en';
}

/** Map a BCP-47 tag (`"en-US"`, `"fr"`, `"FR"`) to a supported locale, or null
 * when none matches. Also accepts the server's display names (`"Français"`). */
export function normalizeLocale(tag?: string | null): Locale | null {
  if (!tag) return null;
  const base = tag.toLowerCase().split(/[-_]/)[0];
  if (base === 'fr' || tag === 'Français') return 'fr';
  if (base === 'en' || tag === 'English') return 'en';
  return null;
}

/** Best locale for the current device: an explicit preference wins, then the
 * browser's languages, else the default. Safe to call without a DOM. */
export function detectLocale(preferred?: string | null): Locale {
  const explicit = normalizeLocale(preferred);
  if (explicit) return explicit;
  const nav = typeof navigator !== 'undefined' ? navigator : undefined;
  let tags: readonly string[] = [];
  if (nav?.languages?.length) tags = nav.languages;
  else if (nav?.language) tags = [nav.language];
  for (const tag of tags) {
    const loc = normalizeLocale(tag);
    if (loc) return loc;
  }
  return DEFAULT_LOCALE;
}

/** Replace `{name}` tokens in `template` from `vars`. Unknown tokens are kept. */
function interpolate(template: string, vars?: TVars): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in vars ? String(vars[name]) : whole,
  );
}

/** Whether a key resolves in `locale` (or the French fallback). */
function hasKey(locale: Locale, key: string): boolean {
  return CATALOGS[locale]?.[key] != null || CATALOGS[DEFAULT_LOCALE][key] != null;
}

/**
 * i18next-style plural resolution. When a translation is called with a numeric
 * `count`, the catalog can carry CLDR plural variants suffixed with the category
 * name `key_one`, `key_other` (and `_zero`/`_two`/`_few`/`_many` where a locale
 * needs them, e.g. Russian/Arabic). `Intl.PluralRules` picks the category for the
 * locale + count; we use the matching variant, else `key_other`, else the base
 * key. `{count}` is available as an interpolation token in every variant.
 *
 *   "content.seasonCount":      "{count} saisons"   ← base / default
 *   "content.seasonCount_one":  "{count} saison"    ← used when count selects "one"
 *   t('content.seasonCount', { count })             ← call site stays count-only
 */
function resolvePluralKey(locale: Locale, key: string, count: number): string {
  let category: Intl.LDMLPluralRule = count === 1 ? 'one' : 'other';
  try {
    category = new Intl.PluralRules(locale).select(count);
  } catch {
    /* environments without Intl.PluralRules → the one/other heuristic above */
  }
  const variant = `${key}_${category}`;
  if (hasKey(locale, variant)) return variant;
  const other = `${key}_other`;
  if (hasKey(locale, other)) return other;
  return key;
}

/** Translate a key in a locale, falling back to French then to the raw key.
 * Pass a numeric `count` in `vars` to select a plural variant (see
 * {@link resolvePluralKey}); `{count}` and any other `vars` are interpolated. */
export function translate(locale: Locale, key: MessageKey, vars?: TVars): string {
  const lookupKey =
    typeof vars?.count === 'number' ? resolvePluralKey(locale, key, vars.count) : key;
  const template = CATALOGS[locale]?.[lookupKey] ?? CATALOGS[DEFAULT_LOCALE][lookupKey] ?? key;
  return interpolate(template, vars);
}

/** Build a translation function bound to `locale`. */
export function createTranslator(locale: Locale): Translate {
  return (key, vars) => translate(locale, key, vars);
}
