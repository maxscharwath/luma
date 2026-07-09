// Debounced dual search: the local catalog (always) + TMDB discovery (gated
// on requests.create). Backed by TanStack Query (dedup + cache); latest-wins is
// handled by the query key changing per (query, type).

import { type DiscoverEntry, type DiscoverType, hasPermission, type SearchHit } from '@luma/core';
import { keepPreviousData, useQuery } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { lumaClient } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';
import { discoverQueries } from '#web/shared/lib/queries';

/** Value that only updates after it stops changing for `ms` — debounces the
 * search box so a keystroke burst issues a single query. */
function useDebouncedValue<T>(value: T, ms: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const h = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(h);
  }, [value, ms]);
  return debounced;
}

export interface DiscoverSearchState {
  loading: boolean;
  local: SearchHit[];
  discover: DiscoverEntry[];
  /** The user can see + use the TMDB section. */
  canDiscover: boolean;
}

export function useDiscoverSearch(query: string, type: DiscoverType): DiscoverSearchState {
  const { user } = useAuth();
  const canDiscover = !!user && hasPermission(user, 'requests.create');
  const q = useDebouncedValue(query.trim(), 250);

  const { data, isFetching } = useQuery({
    queryKey: ['discoverSearch', q, type, canDiscover],
    queryFn: async () => {
      const c = lumaClient();
      const [local, discover] = await Promise.all([
        c
          .search(q, { limit: 24 })
          .then((r) => r.results)
          .catch(() => [] as SearchHit[]),
        canDiscover
          ? c
              .discoverSearch(q, { type })
              .then((r) => r.results)
              .catch(() => [] as DiscoverEntry[])
          : Promise.resolve<DiscoverEntry[]>([]),
      ]);
      // The type filter also narrows the local library hits: `movie` keeps
      // movies; `tv` keeps shows + episodes; `all` keeps everything.
      const filteredLocal =
        type === 'all'
          ? local
          : local.filter((h) => (type === 'movie' ? h.type === 'movie' : h.type !== 'movie'));
      return { local: filteredLocal, discover };
    },
    enabled: q.length > 0,
    placeholderData: keepPreviousData,
  });

  return {
    loading: q.length > 0 && isFetching,
    local: data?.local ?? [],
    discover: data?.discover ?? [],
    canDiscover,
  };
}

export interface TrendingState {
  loading: boolean;
  entries: DiscoverEntry[];
}

/** This week's trending movies + shows, for the browse-first empty state.
 * Fetched once when `enabled` (the user can discover); flagged against the
 * library + open requests the same as search results. */
export function useTrending(enabled: boolean): TrendingState {
  const { data, isFetching } = useQuery({
    queryKey: ['discover', 'trending', 'all'],
    queryFn: () => lumaClient().discoverTrending(),
    enabled,
    select: (r) => r.results,
  });
  return { loading: enabled && isFetching && data === undefined, entries: data ?? [] };
}

export interface TrendingPageState {
  loading: boolean;
  entries: DiscoverEntry[];
  totalPages: number;
}

/** One page of trending titles for a single kind (`movie` | `tv`), backing the
 * full "trending movies" / "trending shows" pages. `keepPreviousData` retains
 * the prior page (and its total) while the next one loads. */
export function useTrendingPage(
  type: 'movie' | 'tv',
  page: number,
  enabled: boolean,
): TrendingPageState {
  const { data, isFetching } = useQuery({
    ...discoverQueries.trending(type, page),
    enabled,
    placeholderData: keepPreviousData,
  });
  return {
    loading: enabled && isFetching,
    entries: data?.results ?? [],
    totalPages: data?.totalPages ?? 1,
  };
}
