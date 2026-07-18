import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  type Catalogs,
  createTranslator,
  detectLocale,
  isLocale,
  normalizeLocale,
  translate,
  translateIn,
} from './i18n';

describe('isLocale', () => {
  it('accepts supported codes and rejects everything else', () => {
    expect(isLocale('fr')).toBe(true);
    expect(isLocale('en')).toBe(true);
    expect(isLocale('de')).toBe(false);
    expect(isLocale('')).toBe(false);
    expect(isLocale(null)).toBe(false);
    expect(isLocale(42)).toBe(false);
  });
});

describe('normalizeLocale', () => {
  it('maps BCP-47 tags to a base locale', () => {
    expect(normalizeLocale('en-US')).toBe('en');
    expect(normalizeLocale('fr')).toBe('fr');
    expect(normalizeLocale('FR')).toBe('fr');
    expect(normalizeLocale('fr_CA')).toBe('fr');
  });

  it('accepts the server display names', () => {
    expect(normalizeLocale('Français')).toBe('fr');
    expect(normalizeLocale('English')).toBe('en');
  });

  it('returns null for unknown / empty tags', () => {
    expect(normalizeLocale('de-DE')).toBeNull();
    expect(normalizeLocale('')).toBeNull();
    expect(normalizeLocale(null)).toBeNull();
    expect(normalizeLocale(undefined)).toBeNull();
  });
});

describe('detectLocale', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('prefers an explicit valid preference', () => {
    expect(detectLocale('en')).toBe('en');
    expect(detectLocale('fr-CH')).toBe('fr');
  });

  it('falls back to the default locale when navigator has no supported locale', () => {
    // Bun provides a navigator.language that varies by machine/CI, so stub it
    // out to make the fallback deterministic.
    vi.stubGlobal('navigator', undefined);
    expect(detectLocale('xx')).toBe('fr');
    expect(detectLocale(null)).toBe('fr');
  });

  it('uses navigator languages when no explicit preference resolves', () => {
    vi.stubGlobal('navigator', { languages: ['de', 'en-US'] });
    expect(detectLocale(null)).toBe('en'); // de unsupported, en-US -> en
    vi.stubGlobal('navigator', { language: 'fr-CH' });
    expect(detectLocale('xx')).toBe('fr');
  });
});

describe('translate', () => {
  it('returns the localized string for a known key', () => {
    expect(translate('fr', 'person.role.actor')).toBe('Acteur');
    expect(translate('en', 'person.role.actor')).toBe('Actor');
  });

  it('interpolates named tokens', () => {
    expect(translate('en', 'discover.pageOf', { page: 2, total: 5 })).toBe('Page 2 / 5');
  });

  it('selects the plural variant by count', () => {
    expect(translate('en', 'content.seasonCount', { count: 1 })).toBe('1 season');
    expect(translate('en', 'content.seasonCount', { count: 3 })).toBe('3 seasons');
  });
});

describe('createTranslator', () => {
  it('binds a locale', () => {
    const t = createTranslator('fr');
    expect(t('person.role.director')).toBe('Réalisateur');
  });
});

describe('translateIn', () => {
  const catalogs: Catalogs = {
    fr: { greeting: 'Bonjour {name}', item_one: '{count} article', item: '{count} articles' },
    en: { greeting: 'Hello {name}' },
  };

  it('translates against an explicit catalog set with interpolation', () => {
    expect(translateIn(catalogs, 'en', 'greeting', { name: 'Max' })).toBe('Hello Max');
  });

  it('falls back to the French default when the locale lacks the key', () => {
    // `en` has no `item`; French catalog is the fallback.
    expect(translateIn(catalogs, 'en', 'item', { count: 5 })).toBe('5 articles');
  });

  it('picks the _one plural variant, else the base', () => {
    expect(translateIn(catalogs, 'fr', 'item', { count: 1 })).toBe('1 article');
    expect(translateIn(catalogs, 'fr', 'item', { count: 4 })).toBe('4 articles');
  });

  it('keeps unknown interpolation tokens verbatim', () => {
    expect(translateIn(catalogs, 'fr', 'greeting', { other: 'x' })).toBe('Bonjour {name}');
  });

  it('returns undefined for a key absent from every catalog', () => {
    expect(translateIn(catalogs, 'en', 'missing.key')).toBeUndefined();
  });
});
