// React bindings for the shared i18n core (`@luma/core`). Web and TV both mount
// <I18nProvider> and read strings with useT(). The provider is "controlled": the
// app owns the resolved `locale` (account preference → device override → browser)
// and passes it in; useSetLocale() bubbles a change request back via
// `onLocaleChange` so the app can persist it and sync it to the account.

import { createTranslator, type Locale, type Translate } from '@luma/core';
import { createContext, type ReactNode, useContext, useMemo } from 'react';

interface I18nValue {
  locale: Locale;
  t: Translate;
  /** Request a locale change. The app persists it and re-renders with the new
   * `locale` prop; a no-op if no `onLocaleChange` was provided. */
  setLocale: (locale: Locale) => void;
}

const I18nContext = createContext<I18nValue | null>(null);

export interface I18nProviderProps {
  locale: Locale;
  onLocaleChange?: (locale: Locale) => void;
  children: ReactNode;
}

export function I18nProvider({ locale, onLocaleChange, children }: Readonly<I18nProviderProps>) {
  const value = useMemo<I18nValue>(
    () => ({
      locale,
      t: createTranslator(locale),
      setLocale: (next) => onLocaleChange?.(next),
    }),
    [locale, onLocaleChange],
  );
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

function useI18n(): I18nValue {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error('useT/useLocale must be used within <I18nProvider>');
  return ctx;
}

/** The bound translation function for the active locale. */
export function useT(): Translate {
  return useI18n().t;
}

/** The active locale. */
export function useLocale(): Locale {
  return useI18n().locale;
}

/** Request a locale change (persisted + account-synced by the app). */
export function useSetLocale(): (locale: Locale) => void {
  return useI18n().setLocale;
}
