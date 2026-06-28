// Resolves and persists the active UI locale, then feeds it to <I18nProvider>.
// Shared by the web and TV apps. Precedence:
//   1. the signed-in account's preference (synced across devices), adopted on
//      sign-in / profile switch;
//   2. the device-level override the user last picked (localStorage);
//   3. the browser locale, else the project default (fr).
// A change is persisted to the device AND, when signed in, to the account.
//
// Tolerates a null client (the TV `connect` screen runs before a server is
// reached), so even pre-auth copy is translated.

import {
  detectLocale,
  isLocale,
  type Locale,
  type LumaClient,
  loadLocalePref,
  normalizeLocale,
  saveLocalePref,
} from '@luma/core';
import { type ReactNode, useCallback, useEffect, useState } from 'react';
import { I18nProvider } from './i18n';

export interface LocaleProviderProps {
  /** API client whose Accept-Language is kept in sync. Null before a server is reached. */
  client: LumaClient | null;
  /** The signed-in account's language preference, or null/undefined when signed out. */
  accountLanguage?: string | null;
  /** Persist a manual change to the signed-in account. Omit when signed out. */
  onAccountChange?: (locale: Locale) => void;
  /** Mirror the locale onto `<html lang>` (web only; TVs have no document chrome). */
  syncHtmlLang?: boolean;
  children: ReactNode;
}

export function LocaleProvider({
  client,
  accountLanguage,
  onAccountChange,
  syncHtmlLang,
  children,
}: Readonly<LocaleProviderProps>) {
  const accountLocale = normalizeLocale(accountLanguage);

  const [override, setOverride] = useState<Locale>(() => {
    const stored = loadLocalePref();
    return isLocale(stored) ? stored : detectLocale();
  });

  // The signed-in account's preference is authoritative: adopt it whenever it
  // becomes known or changes (sign-in, profile switch, or an `me()` refresh that
  // pulled a change made on another device). A manual switch updates the account
  // too (handleChange), so this never reverts a deliberate choice; runs only when
  // accountLocale changes, so it leaves a signed-out device override alone.
  useEffect(() => {
    if (accountLocale) {
      setOverride(accountLocale);
      saveLocalePref(accountLocale);
    }
  }, [accountLocale]);

  const locale = override;

  // Keep the API client (Accept-Language) and optionally <html lang> in sync.
  useEffect(() => {
    client?.setLocale(locale);
    if (syncHtmlLang && typeof document !== 'undefined') document.documentElement.lang = locale;
  }, [client, locale, syncHtmlLang]);

  const handleChange = useCallback(
    (next: Locale) => {
      setOverride(next);
      saveLocalePref(next);
      client?.setLocale(next);
      // Best-effort account sync so the choice follows the profile everywhere.
      if (onAccountChange) {
        onAccountChange(next);
        client?.updateLanguage(next).catch(() => {});
      }
    },
    [client, onAccountChange],
  );

  return (
    <I18nProvider locale={locale} onLocaleChange={handleChange}>
      {children}
    </I18nProvider>
  );
}
