// Resolves and persists the active UI locale for the TV app, then feeds it to
// the shared <I18nProvider>. Same precedence as the web app: account preference
// (adopted on sign-in / switch) → device override (localStorage) → browser/default.
//
// Tolerates a null client (the `connect` screen runs before a server is reached),
// so even discovery copy is translated.

import {
  detectLocale,
  isLocale,
  type Locale,
  type LumaClient,
  loadLocalePref,
  normalizeLocale,
  saveLocalePref,
} from '@luma/core';
import { I18nProvider } from '@luma/ui';
import { type ReactNode, useCallback, useEffect, useState } from 'react';
import { useAuth } from '#tv/auth';

export function LocaleProvider({
  client,
  children,
}: Readonly<{ client: LumaClient | null; children: ReactNode }>) {
  const { user, updateUser } = useAuth();
  const accountLocale = normalizeLocale(user?.language);

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

  // Keep the API client's Accept-Language in sync (when a client exists).
  useEffect(() => {
    client?.setLocale(locale);
  }, [client, locale]);

  const handleChange = useCallback(
    (next: Locale) => {
      setOverride(next);
      saveLocalePref(next);
      client?.setLocale(next);
      if (user) {
        updateUser({ language: next });
        client?.updateLanguage(next).catch(() => {});
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
