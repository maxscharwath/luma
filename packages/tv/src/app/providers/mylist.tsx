// "Ma liste" the user's bookmarked titles. Server-backed (synced with the web
// client) via the my-list API, hydrated once into a set; toggles are optimistic
// and revert if the server call fails. Mirrors the watched provider.

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

interface MyList {
  /** Whether a title (movie item id OR show id) is in the list. */
  has: (id: string) => boolean;
  /** Flip a title's membership (optimistic + persisted). */
  toggle: (id: string) => void;
  /** Re-fetch the list from the server (e.g. to pick up web-side changes). */
  refresh: () => void;
}

const Ctx = createContext<MyList | null>(null);

export function MyListProvider({ children }: Readonly<{ children: ReactNode }>) {
  const { user } = useAuth();
  const { client } = useConnection();
  const [ids, setIds] = useState<ReadonlySet<string>>(() => new Set());

  const refresh = useCallback(() => {
    if (!user || !client) {
      setIds(new Set());
      return;
    }
    client
      .myList()
      .then((list) => setIds(new Set(list)))
      .catch(() => undefined);
  }, [client, user]);

  useEffect(() => refresh(), [refresh]);

  const setInList = useCallback(
    (id: string, inList: boolean) => {
      if (!client) return;
      setIds((prev) => {
        if (prev.has(id) === inList) return prev;
        const next = new Set(prev);
        if (inList) next.add(id);
        else next.delete(id);
        return next;
      });
      const call = inList ? client.addToList(id) : client.removeFromList(id);
      call.catch(() => {
        setIds((prev) => {
          const next = new Set(prev);
          if (inList) next.delete(id);
          else next.add(id);
          return next;
        });
      });
    },
    [client],
  );

  const value = useMemo<MyList>(
    () => ({
      has: (id) => ids.has(id),
      toggle: (id) => setInList(id, !ids.has(id)),
      refresh,
    }),
    [ids, setInList, refresh],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useMyList(): MyList {
  const c = useContext(Ctx);
  if (!c) throw new Error('useMyList() must be used inside <MyListProvider>');
  return c;
}
