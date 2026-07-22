// React bindings for the shared @kroma/core i18n catalogs (fr/en). Locale
// precedence: in-app override, then the OS locale (which reflects the per-app
// language in iOS Settings, thanks to CFBundleLocalizations), then the account
// preference for devices whose OS language we don't support.

import {
  createTranslator,
  DEFAULT_LOCALE,
  type Locale,
  normalizeLocale,
  type Translate,
} from '@kroma/core';
import { getLocales } from 'expo-localization';
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from 'react';
import { useSession } from './session';
import { loadPref, savePref } from './storage';

interface I18n {
  locale: Locale;
  t: Translate;
  /** Device override; null = follow account/OS. */
  override: Locale | null;
  setOverride(locale: Locale | null): void;
}

const Ctx = createContext<I18n | null>(null);

export function useT(): Translate {
  const value = useContext(Ctx);
  if (!value) throw new Error('useT outside I18nProvider');
  return value.t;
}

export function useI18n(): I18n {
  const value = useContext(Ctx);
  if (!value) throw new Error('useI18n outside I18nProvider');
  return value;
}

function osLocale(): Locale | null {
  for (const l of getLocales()) {
    const match = normalizeLocale(l.languageTag);
    if (match) return match;
  }
  return null;
}

export function I18nProvider({ children }: Readonly<{ children: ReactNode }>) {
  const { user, client } = useSession();
  const [override, setOverrideState] = useState<Locale | null>(null);

  useEffect(() => {
    void loadPref('locale').then((v) => setOverrideState(normalizeLocale(v)));
  }, []);

  const locale = override ?? osLocale() ?? normalizeLocale(user?.language) ?? DEFAULT_LOCALE;

  // Keep Accept-Language in sync so server-rendered strings match the UI.
  useEffect(() => {
    client?.setLocale(locale);
  }, [client, locale]);

  const setOverride = useCallback((next: Locale | null) => {
    setOverrideState(next);
    void savePref('locale', next);
  }, []);

  const value = useMemo<I18n>(
    () => ({ locale, t: createTranslator(locale), override, setOverride }),
    [locale, override, setOverride],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
