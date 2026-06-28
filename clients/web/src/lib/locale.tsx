// Resolves and persists the active UI locale for the web app, then feeds it to
// the shared <I18nProvider>. Precedence:
//   1. the signed-in account's preference (synced across devices), adopted on
//      sign-in / profile switch;
//   2. the device-level override the user last picked here (localStorage);
//   3. the browser locale, else the project default (fr).
// A change is persisted to the device AND, when signed in, to the account.

import {
  detectLocale,
  isLocale,
  type Locale,
  loadLocalePref,
  normalizeLocale,
  saveLocalePref,
} from '@luma/core';
import { I18nProvider } from '@luma/ui';
import { type ReactNode, useCallback, useEffect, useState } from 'react';
import { useAuth } from '#web/lib/auth';

export function LocaleProvider({ children }: Readonly<{ children: ReactNode }>) {
  const { user, client, updateUser } = useAuth();
  const accountLocale = normalizeLocale(user?.language);

  const [override, setOverride] = useState<Locale>(() => {
    const stored = loadLocalePref();
    return isLocale(stored) ? stored : detectLocale();
  });

  // The signed-in account's preference is authoritative: adopt it whenever it
  // becomes known or changes (sign-in, profile switch, or an `me()` refresh that
  // pulled a change made on ANOTHER device). A manual switch updates the account
  // too (see handleChange), so `override` and `accountLocale` stay consistent and
  // this never fights/reverts a deliberate choice. Runs only when accountLocale
  // changes, so it doesn't clobber a signed-out device override (accountLocale null).
  useEffect(() => {
    if (accountLocale) {
      setOverride(accountLocale);
      saveLocalePref(accountLocale);
    }
  }, [accountLocale]);

  const locale = override;

  // Keep the API client (Accept-Language) and <html lang> in sync.
  useEffect(() => {
    client.setLocale(locale);
    if (typeof document !== 'undefined') document.documentElement.lang = locale;
  }, [client, locale]);

  const handleChange = useCallback(
    (next: Locale) => {
      setOverride(next);
      saveLocalePref(next);
      client.setLocale(next);
      // Best-effort account sync so the choice follows the profile everywhere,
      // plus an immediate in-memory + stored-session update so a reload keeps it.
      if (user) {
        updateUser({ language: next });
        client.updateLanguage(next).catch(() => {});
      }
    },
    [client, user, updateUser],
  );

  return (
    <I18nProvider locale={locale} onLocaleChange={handleChange}>
      {children}
    </I18nProvider>
  );
}
