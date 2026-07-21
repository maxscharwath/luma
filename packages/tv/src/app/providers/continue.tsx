import type { ContinueItem, KromaClient } from '@kroma/core';
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
import { getExo } from '#tv/features/playback/player/engine';

/** Shape the native Android shell's Watch Next row consumes (see WatchNext.kt). */
function toWatchNext(items: ContinueItem[], client: KromaClient) {
  return items.map((c) => {
    const it = c.item;
    return {
      id: it.id,
      title: it.showTitle ?? it.title,
      subtitle: it.episodeTitle ?? (it.year ? String(it.year) : ''),
      imageUrl: client.backdropFor(it) ?? client.posterFor(it),
      progressMs: Math.round(c.positionMs),
      durationMs: Math.round(c.durationMs ?? 0),
      kind: it.kind,
      updatedAtMs: Date.parse(c.updatedAt) || Date.now(),
    };
  });
}

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

  // Mirror the list into the Android TV / Google TV launcher's system "Continue
  // watching" (Watch Next) row, so it shows on the platform home even when the
  // app is closed. No-op off the Android shell (getExo() null / no method).
  useEffect(() => {
    const exo = getExo();
    if (!exo?.setContinueWatching || !client) return;
    exo.setContinueWatching(JSON.stringify(toWatchNext(items, client)));
  }, [items, client]);

  const value = useMemo<Continue>(() => ({ items, refresh }), [items, refresh]);
  return <ContinueCtx.Provider value={value}>{children}</ContinueCtx.Provider>;
}

export function useContinue(): Continue {
  const c = useContext(ContinueCtx);
  if (!c) throw new Error('useContinue() must be used inside <ContinueProvider>');
  return c;
}
