// Central query-options factory: one place for every query key + fetcher, so
// route loaders (`ensureQueryData`) and components (`useSuspenseQuery`/`useQuery`)
// share the exact same cache entry. Each fetcher goes through the ad-hoc
// `lumaClient()` (in-memory bearer, self-refreshing on 401), so these work the
// same whether called from a loader or a component.
//
// View-shaped queries (`moviesView`, `showsView`) pre-resolve art/stream URLs via
// the existing `toMovieView`/`toShowView` mappers so consumers keep the same data
// shape the old loaders returned.
import type { DiscoverDetail, DiscoverType, Show, ShowDetail, UpNext } from '@luma/core';
import { queryOptions } from '@tanstack/react-query';
import {
  lumaClient,
  type MovieView,
  type ShowView,
  toMovieView,
  toShowView,
} from '#web/shared/lib/api';

/** Everything the show-detail page needs, in one cache entry. It's inherently
 * two-stage (the TMDB discover overlay keys off the show's tmdbId, known only
 * after the show loads) and conditional, so unlike the movie page it doesn't
 * decompose into independent `useSuspenseQuery` calls. */
export interface ShowBundle {
  detail: ShowDetail;
  similarShows: Show[];
  upNext: UpNext | null;
  discover: DiscoverDetail | null;
}

// ---- Catalogue ------------------------------------------------------------

export const catalogQueries = {
  /** All movies, art/stream URLs pre-resolved. */
  moviesView: () =>
    queryOptions({
      queryKey: ['movies', 'view'] as const,
      queryFn: async (): Promise<MovieView[]> => {
        const c = lumaClient();
        return (await c.movies()).map((m) => toMovieView(c, m));
      },
    }),

  /** All shows, art pre-resolved. */
  showsView: () =>
    queryOptions({
      queryKey: ['shows', 'view'] as const,
      queryFn: async (): Promise<ShowView[]> => {
        const c = lumaClient();
        return (await c.shows()).map((s) => toShowView(c, s));
      },
    }),

  /** Raw movie list (unmapped) used where the id-only data is enough. */
  movies: () =>
    queryOptions({ queryKey: ['movies'] as const, queryFn: () => lumaClient().movies() }),

  shows: () => queryOptions({ queryKey: ['shows'] as const, queryFn: () => lumaClient().shows() }),

  item: (id: string) =>
    queryOptions({ queryKey: ['item', id] as const, queryFn: () => lumaClient().item(id) }),

  show: (id: string) =>
    queryOptions({ queryKey: ['show', id] as const, queryFn: () => lumaClient().show(id) }),

  /** The full show-detail bundle (detail + similar + up-next + discover overlay). */
  showBundle: (id: string) =>
    queryOptions({
      queryKey: ['show', id, 'bundle'] as const,
      queryFn: async (): Promise<ShowBundle> => {
        const c = lumaClient();
        const [detail, shows] = await Promise.all([c.show(id), c.shows()]);
        const show = detail.show;
        const tmdbId = show.metadata?.tmdbId ?? null;
        // The discover overlay (season availability + request state) is fetched
        // only for an enriched show and degrades to null for viewers without
        // `requests.create` (a 403 the server returns before any TMDB call).
        const [upNext, discover] = await Promise.all([
          c.upNext(show.id).catch(() => null),
          tmdbId != null ? c.discoverDetail('tv', tmdbId).catch(() => null) : Promise.resolve(null),
        ]);
        const genres = new Set(show.metadata?.genres ?? []);
        const others = shows.filter((s) => s.id !== show.id);
        const related = others.filter((s) => (s.metadata?.genres ?? []).some((g) => genres.has(g)));
        const similarShows = (related.length >= 3 ? related : others).slice(0, 12);
        return { detail, similarShows, upNext, discover };
      },
    }),

  similar: (id: string) =>
    queryOptions({
      queryKey: ['similar', id] as const,
      // The catalogue tolerates a missing similar list (falls back to genre
      // overlap), so swallow failures into an empty array here.
      queryFn: () =>
        lumaClient()
          .similar(id)
          .catch(() => []),
    }),

  personCredits: (name: string) =>
    queryOptions({
      queryKey: ['person', name] as const,
      queryFn: () => lumaClient().personCredits(name),
    }),

  upNext: (showId: string) =>
    queryOptions({
      queryKey: ['upNext', showId] as const,
      queryFn: () => lumaClient().upNext(showId),
    }),

  nextEpisode: (itemId: string) =>
    queryOptions({
      queryKey: ['nextEpisode', itemId] as const,
      queryFn: () => lumaClient().nextEpisode(itemId),
    }),

  /** The player payload: the item (art/stream URLs resolved) + its "up next"
   * episode for autoplay. */
  watch: (id: string) =>
    queryOptions({
      queryKey: ['watch', id] as const,
      queryFn: async () => {
        const c = lumaClient();
        const [item, next] = await Promise.all([c.item(id), c.nextEpisode(id)]);
        return { item: toMovieView(c, item), next };
      },
    }),
} as const;

// ---- Per-user (only mount once `ready && user`) ---------------------------

export const userQueries = {
  home: () => queryOptions({ queryKey: ['home'] as const, queryFn: () => lumaClient().home() }),

  continueWatching: () =>
    queryOptions({
      queryKey: ['continueWatching'] as const,
      queryFn: () => lumaClient().continueWatching(),
    }),

  /** Resume progress for every item, keyed for cheap lookup. */
  progress: () =>
    queryOptions({ queryKey: ['progress'] as const, queryFn: () => lumaClient().progress() }),

  myRequests: () =>
    queryOptions({
      queryKey: ['requests', 'mine'] as const,
      queryFn: () => lumaClient().listRequests({ mine: true }),
    }),

  /** The account's signed-in devices (for the /account security section). */
  sessions: () =>
    queryOptions({ queryKey: ['sessions'] as const, queryFn: () => lumaClient().listSessions() }),

  /** The account's registered passkeys (for the /account security section). */
  passkeys: () =>
    queryOptions({ queryKey: ['passkeys'] as const, queryFn: () => lumaClient().listPasskeys() }),
} as const;

// ---- Server ---------------------------------------------------------------

export const serverQueries = {
  /** Public `GET /api/health` — server version + basic counts (no auth). Used by
   * the sidebar to show the server version; cached generously as it rarely moves. */
  health: () =>
    queryOptions({
      queryKey: ['health'] as const,
      queryFn: () => lumaClient().health(),
      staleTime: 5 * 60_000,
    }),
} as const;

// ---- Discover -------------------------------------------------------------

export const discoverQueries = {
  detail: (kind: 'movie' | 'tv', tmdbId: number) =>
    queryOptions({
      queryKey: ['discover', 'detail', kind, tmdbId] as const,
      queryFn: () => lumaClient().discoverDetail(kind, tmdbId),
    }),

  trending: (type: DiscoverType, page: number) =>
    queryOptions({
      queryKey: ['discover', 'trending', type, page] as const,
      queryFn: () => lumaClient().discoverTrending({ type, page }),
    }),
} as const;
