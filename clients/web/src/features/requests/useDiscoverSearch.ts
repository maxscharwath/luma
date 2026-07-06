// Debounced dual search: the local catalog (always) + TMDB discovery (gated
// on requests.create). Latest-wins via a sequence ref, mirroring TvSearch.

import {
  type DiscoverEntry,
  type DiscoverType,
  hasPermission,
  type SearchHit,
} from '@luma/core';
import { useEffect, useRef, useState } from 'react';
import { useAuth } from '#web/shared/lib/auth';

export interface DiscoverSearchState {
  loading: boolean;
  local: SearchHit[];
  discover: DiscoverEntry[];
  /** The user can see + use the TMDB section. */
  canDiscover: boolean;
}

export function useDiscoverSearch(query: string, type: DiscoverType): DiscoverSearchState {
  const { client, user } = useAuth();
  const canDiscover = !!user && hasPermission(user, 'requests.create');
  const [state, setState] = useState<DiscoverSearchState>({
    loading: false,
    local: [],
    discover: [],
    canDiscover,
  });
  const seq = useRef(0);

  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setState({ loading: false, local: [], discover: [], canDiscover });
      return;
    }
    const mine = ++seq.current;
    setState((s) => ({ ...s, loading: true, canDiscover }));
    const handle = setTimeout(() => {
      const localP = client.search(q, { limit: 24 }).then((r) => r.results).catch(() => []);
      const discoverP = canDiscover
        ? client.discoverSearch(q, { type }).then((r) => r.results).catch(() => [])
        : Promise.resolve<DiscoverEntry[]>([]);
      Promise.all([localP, discoverP]).then(([local, discover]) => {
        // Drop stale answers: only the most recent query may commit.
        if (mine !== seq.current) return;
        // The type filter also narrows the local library hits: `movie` keeps
        // movies; `tv` keeps shows + episodes; `all` keeps everything.
        const filteredLocal =
          type === 'all'
            ? local
            : local.filter((h) => (type === 'movie' ? h.type === 'movie' : h.type !== 'movie'));
        setState({ loading: false, local: filteredLocal, discover, canDiscover });
      });
    }, 250);
    return () => clearTimeout(handle);
  }, [query, type, client, canDiscover]);

  return state;
}

export interface TrendingState {
  loading: boolean;
  entries: DiscoverEntry[];
}

/** This week's trending movies + shows, for the browse-first empty state.
 * Fetched once when `enabled` (the user can discover); flagged against the
 * library + open requests the same as search results. */
export function useTrending(enabled: boolean): TrendingState {
  const { client } = useAuth();
  const [state, setState] = useState<TrendingState>({ loading: enabled, entries: [] });

  useEffect(() => {
    if (!enabled) {
      setState({ loading: false, entries: [] });
      return;
    }
    let active = true;
    setState((s) => ({ ...s, loading: true }));
    client
      .discoverTrending()
      .then((r) => {
        if (active) setState({ loading: false, entries: r.results });
      })
      .catch(() => {
        if (active) setState({ loading: false, entries: [] });
      });
    return () => {
      active = false;
    };
  }, [enabled, client]);

  return state;
}

export interface TrendingPageState {
  loading: boolean;
  entries: DiscoverEntry[];
  totalPages: number;
}

/** One page of trending titles for a single kind (`movie` | `tv`), backing the
 * full "trending movies" / "trending shows" pages. Latest-wins via a sequence
 * ref so fast paging never commits a stale page. */
export function useTrendingPage(
  type: 'movie' | 'tv',
  page: number,
  enabled: boolean,
): TrendingPageState {
  const { client } = useAuth();
  const [state, setState] = useState<TrendingPageState>({
    loading: enabled,
    entries: [],
    totalPages: 1,
  });
  const seq = useRef(0);

  useEffect(() => {
    if (!enabled) {
      setState({ loading: false, entries: [], totalPages: 1 });
      return;
    }
    const mine = ++seq.current;
    setState((s) => ({ ...s, loading: true }));
    client
      .discoverTrending({ type, page })
      .then((r) => {
        if (mine !== seq.current) return;
        setState({ loading: false, entries: r.results, totalPages: r.totalPages });
      })
      .catch(() => {
        if (mine !== seq.current) return;
        setState({ loading: false, entries: [], totalPages: 1 });
      });
  }, [type, page, enabled, client]);

  return state;
}
