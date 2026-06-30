import type { ContinueItem } from '@luma/core';
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from 'react';
import { useAuth } from '#tv/app/providers/auth';
import { useConnection } from '#tv/app/providers/connection';

interface Continue {
  items: ContinueItem[];
  /** Re-fetch (e.g. the home screen calls this on mount, after the player closes). */
  refresh: () => void;
}

const ContinueCtx = createContext<Continue | null>(null);

/** "Reprendre" (continue watching) per-user. Re-fetched on sign-in and whenever
 * the home screen asks. Mounted inside the auth + connection providers. */
export function ContinueProvider({ children }: Readonly<{ children: ReactNode }>) {
  const { user } = useAuth();
  const { client } = useConnection();
  const [items, setItems] = useState<ContinueItem[]>([]);

  const refresh = useCallback(() => {
    if (!user || !client) {
      setItems([]);
      return;
    }
    client
      .continueWatching()
      .then(setItems)
      .catch(() => undefined);
  }, [client, user]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const value = useMemo<Continue>(() => ({ items, refresh }), [items, refresh]);
  return <ContinueCtx.Provider value={value}>{children}</ContinueCtx.Provider>;
}

export function useContinue(): Continue {
  const c = useContext(ContinueCtx);
  if (!c) throw new Error('useContinue() must be used inside <ContinueProvider>');
  return c;
}
