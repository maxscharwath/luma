// Per-user "watched" state, hydrated once and shared across every card.
//
// The catalogue renders hundreds of poster tiles; fetching watched status per
// card would be N requests, so we load the whole set of watched item ids once
// (on sign-in) and let each card check membership. Toggling is optimistic the
// set updates immediately and reverts if the server call fails.

import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from 'react';
import { useAuth } from '#web/shared/lib/auth';

interface WatchedValue {
  /** True once the watched set has been hydrated (or there's no user). */
  ready: boolean;
  /** Whether the given item is marked watched by the current user. */
  isWatched: (id: string) => boolean;
  /** Optimistically set/clear an item's watched flag, persisting to the server. */
  setWatched: (id: string, watched: boolean) => void;
  /** Flip an item's watched flag. */
  toggleWatched: (id: string) => void;
}

const WatchedContext = createContext<WatchedValue | null>(null);

export function WatchedProvider({ children }: Readonly<{ children: ReactNode }>) {
  const { client, user, ready: authReady } = useAuth();
  const [ids, setIds] = useState<ReadonlySet<string>>(() => new Set());
  const [ready, setReady] = useState(false);

  // Hydrate the watched set when the signed-in user changes (clear when signed out).
  useEffect(() => {
    if (!authReady) return;
    if (!user) {
      setIds(new Set());
      setReady(true);
      return;
    }
    let cancelled = false;
    setReady(false);
    client
      .watched()
      .then((list) => {
        if (!cancelled) {
          setIds(new Set(list));
          setReady(true);
        }
      })
      .catch(() => {
        if (!cancelled) setReady(true);
      });
    return () => {
      cancelled = true;
    };
  }, [client, user, authReady]);

  const setWatched = useCallback(
    (id: string, watched: boolean) => {
      if (!user) return;
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
    [client, user],
  );

  const value = useMemo<WatchedValue>(
    () => ({
      ready,
      isWatched: (id) => ids.has(id),
      setWatched,
      toggleWatched: (id) => setWatched(id, !ids.has(id)),
    }),
    [ids, ready, setWatched],
  );

  return <WatchedContext.Provider value={value}>{children}</WatchedContext.Provider>;
}

/** Access the watched-state context. Throws if used outside `<WatchedProvider>`. */
export function useWatched(): WatchedValue {
  const ctx = useContext(WatchedContext);
  if (!ctx) throw new Error('useWatched must be used within <WatchedProvider>');
  return ctx;
}
