// TV adapter over the shared <LocaleProvider> (@luma/ui): the client is passed in
// (null on the `connect` screen, before a server is reached) and wired together
// with the signed-in account's preference.
import type { Locale, LumaClient } from '@luma/core';
import { LocaleProvider as UiLocaleProvider } from '@luma/ui';
import type { ReactNode } from 'react';
import { useAuth } from '#tv/auth';

export function LocaleProvider({
  client,
  children,
}: Readonly<{ client: LumaClient | null; children: ReactNode }>) {
  const { user, updateUser } = useAuth();
  return (
    <UiLocaleProvider
      client={client}
      accountLanguage={user?.language}
      onAccountChange={user ? (next: Locale) => updateUser({ language: next }) : undefined}
    >
      {children}
    </UiLocaleProvider>
  );
}
