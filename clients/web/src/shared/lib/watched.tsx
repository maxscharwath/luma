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
  useRef,
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
  // Authoritative mirror of `ids`, readable synchronously from `setWatched`
  // without putting `ids` in its dep list (which would churn its identity). Lets
  // us act only on a real transition.
  const idsRef = useRef<ReadonlySet<string>>(ids);
  const apply = useCallback((next: ReadonlySet<string>) => {
    idsRef.current = next;
    setIds(next);
  }, []);

  // Hydrate the watched set when the signed-in user changes (clear when signed out).
  useEffect(() => {
    if (!authReady) return;
    if (!user) {
      apply(new Set());
      setReady(true);
      return;
    }
    let cancelled = false;
    setReady(false);
    client
      .watched()
      .then((list) => {
        if (!cancelled) {
          apply(new Set(list));
          setReady(true);
        }
      })
      .catch(() => {
        if (!cancelled) setReady(true);
      });
    return () => {
      cancelled = true;
    };
  }, [client, user, authReady, apply]);

  const setWatched = useCallback(
    (id: string, watched: boolean) => {
      if (!user) return;
      // Only act on a real transition. Skipping no-ops avoids the redundant POSTs
      // that progress-save fires every 10s near the end, and (crucially) avoids a
      // failed redundant call reverting and clearing an already-watched badge.
      if (idsRef.current.has(id) === watched) return;
      const next = new Set(idsRef.current);
      if (watched) next.add(id);
      else next.delete(id);
      apply(next);
      const call = watched ? client.markWatched(id) : client.unmarkWatched(id);
      call.catch(() => {
        // Revert the optimistic change on failure.
        const reverted = new Set(idsRef.current);
        if (watched) reverted.delete(id);
        else reverted.add(id);
        apply(reverted);
      });
    },
    [client, user, apply],
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
