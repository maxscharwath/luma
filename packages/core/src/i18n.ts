// Shared, framework-agnostic i18n core. The JSON catalogs in ./locales are the
// single source of truth for every KROMA surface they are bundled into the TS
// clients here AND `include_str!`'d by the Rust server (see server/src/i18n.rs),
// so message keys stay in lockstep across the whole stack.
//
// React bindings (provider + hooks) live in `@kroma/ui` to keep this package
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

/** A single-locale message catalog (dotted key -> template). */
export type Catalog = Record<string, string>;
/** A per-locale set of catalogs (some locales may be absent). */
export type Catalogs = Partial<Record<Locale, Catalog>>;

/** Whether a key resolves in `catalogs` for `locale` (or the French fallback). */
function hasKeyIn(catalogs: Catalogs, locale: Locale, key: string): boolean {
  return catalogs[locale]?.[key] != null || catalogs[DEFAULT_LOCALE]?.[key] != null;
}

/**
 * i18next-style plural resolution over an explicit catalog set. When a
 * translation is called with a numeric `count`, the catalog can carry CLDR plural
 * variants suffixed with the category name `key_one`, `key_other` (and
 * `_zero`/`_two`/`_few`/`_many` where a locale needs them). `Intl.PluralRules`
 * picks the category; we use the matching variant, else `key_other`, else the
 * base key. `{count}` is available as an interpolation token in every variant.
 *
 *   "content.seasonCount":      "{count} saisons"   ← base / default
 *   "content.seasonCount_one":  "{count} saison"    ← used when count selects "one"
 *   t('content.seasonCount', { count })             ← call site stays count-only
 */
function pluralKeyIn(catalogs: Catalogs, locale: Locale, key: string, count: number): string {
  let category: Intl.LDMLPluralRule = count === 1 ? 'one' : 'other';
  try {
    category = new Intl.PluralRules(locale).select(count);
  } catch {
    /* environments without Intl.PluralRules → the one/other heuristic above */
  }
  const variant = `${key}_${category}`;
  if (hasKeyIn(catalogs, locale, variant)) return variant;
  const other = `${key}_other`;
  if (hasKeyIn(catalogs, locale, other)) return other;
  return key;
}

/** Translate `key` against an explicit catalog set (e.g. a module's own
 * catalogs), returning `undefined` when the key is absent so the caller can fall
 * back to the core translator. Applies the same plural + `{name}` interpolation
 * as {@link translate}. */
export function translateIn(
  catalogs: Catalogs,
  locale: Locale,
  key: string,
  vars?: TVars,
): string | undefined {
  const lookupKey =
    typeof vars?.count === 'number' ? pluralKeyIn(catalogs, locale, key, vars.count) : key;
  const template = catalogs[locale]?.[lookupKey] ?? catalogs[DEFAULT_LOCALE]?.[lookupKey];
  return template == null ? undefined : interpolate(template, vars);
}

/** Translate a key in a locale, falling back to French then to the raw key.
 * Pass a numeric `count` in `vars` to select a plural variant; `{count}` and any
 * other `vars` are interpolated. */
export function translate(locale: Locale, key: MessageKey, vars?: TVars): string {
  return translateIn(CATALOGS, locale, key, vars) ?? key;
}

/** Build a translation function bound to `locale`. */
export function createTranslator(locale: Locale): Translate {
  return (key, vars) => translate(locale, key, vars);
}
