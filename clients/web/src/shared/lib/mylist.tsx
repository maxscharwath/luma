// "Ma liste" the user's bookmarked titles, hydrated once and shared across the
// detail toggle and the "Ma liste" page. Server-backed (synced with the TV), with
// optimistic toggles that revert if the server call fails. Mirrors the watched
// provider ([[kroma-accounts-permissions]]).

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

interface MyListValue {
  /** True once the list has been hydrated (or there's no user). */
  ready: boolean;
  /** Item ids in the list (newest first) for the "Ma liste" page. */
  ids: readonly string[];
  /** Whether a title is in the list. */
  inList: (id: string) => boolean;
  /** Optimistically add/remove a title, persisting to the server. */
  setInList: (id: string, inList: boolean) => void;
  /** Flip a title's membership. */
  toggle: (id: string) => void;
}

const MyListContext = createContext<MyListValue | null>(null);

export function MyListProvider({ children }: Readonly<{ children: ReactNode }>) {
  const { client, user, ready: authReady } = useAuth();
  const [ids, setIds] = useState<readonly string[]>([]);
  const [ready, setReady] = useState(false);

  // Hydrate when the signed-in user changes (clear when signed out).
  useEffect(() => {
    if (!authReady) return;
    if (!user) {
      setIds([]);
      setReady(true);
      return;
    }
    let cancelled = false;
    setReady(false);
    client
      .myList()
      .then((list) => {
        if (!cancelled) {
          setIds(list);
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

  const setInList = useCallback(
    (id: string, inList: boolean) => {
      if (!user) return;
      setIds((prev) => {
        if (prev.includes(id) === inList) return prev;
        return inList ? [id, ...prev] : prev.filter((x) => x !== id);
      });
      const call = inList ? client.addToList(id) : client.removeFromList(id);
      call.catch(() => {
        // Revert the optimistic change on failure.
        setIds((prev) => {
          if (inList) return prev.filter((x) => x !== id);
          return prev.includes(id) ? prev : [id, ...prev];
        });
      });
    },
    [client, user],
  );

  const value = useMemo<MyListValue>(() => {
    const set = new Set(ids);
    return {
      ready,
      ids,
      inList: (id) => set.has(id),
      setInList,
      toggle: (id) => setInList(id, !set.has(id)),
    };
  }, [ids, ready, setInList]);

  return <MyListContext.Provider value={value}>{children}</MyListContext.Provider>;
}

/** Access the "Ma liste" context. Throws if used outside `<MyListProvider>`. */
export function useMyList(): MyListValue {
  const ctx = useContext(MyListContext);
  if (!ctx) throw new Error('useMyList must be used within <MyListProvider>');
  return ctx;
}
