// Web adapter over the shared <LocaleProvider> (@luma/ui): wires the app's auth
// (client + signed-in account) into the controlled locale resolver and mirrors
// the choice onto <html lang>.
import type { Locale } from '@luma/core';
import { LocaleProvider as UiLocaleProvider } from '@luma/ui';
import type { ReactNode } from 'react';
import { useAuth } from '#web/lib/auth';

export function LocaleProvider({ children }: Readonly<{ children: ReactNode }>) {
  const { user, client, updateUser } = useAuth();
  return (
    <UiLocaleProvider
      client={client}
      accountLanguage={user?.language}
      onAccountChange={user ? (next: Locale) => updateUser({ language: next }) : undefined}
      syncHtmlLang
    >
      {children}
    </UiLocaleProvider>
  );
}
