// Per-user "watched" state, hydrated once and shared across every tile + the
// detail toggle. Unlike "Ma liste" (local-only), this is server-backed: the set
// of watched item/show ids comes from `client.watched()` and toggles persist via
// `client.markWatched` / `client.unmarkWatched`. Toggles are optimistic the set
// updates immediately and reverts if the server call fails.

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

interface Watched {
  /** Whether the given item/show id is marked watched. */
  has: (id: string) => boolean;
  /** Optimistically set/clear an id's watched flag, persisting to the server. */
  setWatched: (id: string, watched: boolean) => void;
  /** Flip an id's watched flag. */
  toggle: (id: string) => void;
  /** Re-fetch the watched set from the server (e.g. after finishing playback). */
  refresh: () => void;
}

const Ctx = createContext<Watched | null>(null);

export function WatchedProvider({ children }: Readonly<{ children: ReactNode }>) {
  const { user } = useAuth();
  const { client } = useConnection();
  const [ids, setIds] = useState<ReadonlySet<string>>(() => new Set());

  const refresh = useCallback(() => {
    if (!user || !client) {
      setIds(new Set());
      return;
    }
    client
      .watched()
      .then((list) => setIds(new Set(list)))
      .catch(() => undefined);
  }, [client, user]);

  // Hydrate (and clear on sign-out / server switch).
  useEffect(() => refresh(), [refresh]);

  const setWatched = useCallback(
    (id: string, watched: boolean) => {
      if (!client) return;
      setIds((prev) => {
        if (prev.has(id) === watched) return prev;
        const next = new Set(prev);
        if (watched) next.add(id);
        else next.delete(id);
        return next;
      });
      const call = watched ? client.markWatched(id) : client.unmarkWatched(id);
      call.catch(() => {
        // Revert the optimistic change on failure.
        setIds((prev) => {
          const next = new Set(prev);
          if (watched) next.delete(id);
          else next.add(id);
          return next;
        });
      });
    },
    [client],
  );

  const value = useMemo<Watched>(
    () => ({
      has: (id) => ids.has(id),
      setWatched,
      toggle: (id) => setWatched(id, !ids.has(id)),
      refresh,
    }),
    [ids, setWatched, refresh],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useWatched(): Watched {
  const c = useContext(Ctx);
  if (!c) throw new Error('useWatched() must be used inside <WatchedProvider>');
  return c;
}
