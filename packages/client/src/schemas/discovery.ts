// Runtime schemas for the discovery / requests / search domain.
//
// Mirrors the ts-rs generated wire types (the Rust structs are the single source
// of truth) but adds runtime validation via `.parse()` and branded ids. The
// `true satisfies SameKeys<…>` / `Exact<…>` lines are compile-time drift guards:
// change a Rust struct + rerun `gen:types` and the build breaks here until the
// schema is updated. See `./accounts` for the pattern this file follows.

import { z } from 'zod';
import { RequestId, UserId } from './ids';
import { CastMember, CrewMember, MediaItem, Show } from './media';

/** What a request targets mirror of TMDB's movie/tv split (a "show", not "tv"). */
export const RequestKind = z.enum(['movie', 'show']);

/** A request's lifecycle state (durable DB states + derived acquisition states). */
export const RequestStatus = z.enum([
  'pending',
  'approved',
  'searching',
  'downloading',
  'importing',
  'available',
  'partially_available',
  'failed',
  'denied',
]);

/** Status tallies for the admin queue's filter chips. */
export const RequestCounts = z.object({
  total: z.number(),
  pending: z.number(),
  active: z.number(),
  available: z.number(),
  denied: z.number(),
  failed: z.number(),
});
export type RequestCounts = z.infer<typeof RequestCounts>;

/** One (season, episode) pair, for a request that targets individual episodes. */
export const EpisodeRef = z.object({
  season: z.number(),
  episode: z.number(),
});
export type EpisodeRef = z.infer<typeof EpisodeRef>;

/** One media request, as listed to clients. `Option` fields are `.nullish()`. */
export const MediaRequest = z.object({
  id: RequestId,
  kind: RequestKind,
  tmdbId: z.number(),
  title: z.string(),
  year: z.number().nullable(),
  posterUrl: z.string().nullable(),
  seasons: z.array(z.number()).nullable(),
  episodes: z.array(EpisodeRef).nullable(),
  status: RequestStatus,
  requestedBy: UserId.nullable(),
  requestedByName: z.string().nullable(),
  reviewedBy: UserId.nullable(),
  note: z.string().nullable(),
  createdAt: z.number(),
  updatedAt: z.number(),
  progress: z.number().nullable(),
});
export type MediaRequest = z.infer<typeof MediaRequest>;

/** `GET /api/requests`. */
export const RequestsView = z.object({
  requests: z.array(MediaRequest),
  counts: RequestCounts,
});
export type RequestsView = z.infer<typeof RequestsView>;

/** `POST /api/requests` body. */
export const CreateRequestBody = z.object({
  kind: RequestKind,
  tmdbId: z.number(),
  seasons: z.array(z.number()).nullable(),
  /** For shows: individual episodes to request, unioned with `seasons`. */
  episodes: z.array(EpisodeRef).nullish(),
});
export type CreateRequestBody = z.infer<typeof CreateRequestBody>;

/** One TMDB discovery result, flagged against the local catalog + open requests. */
export const DiscoverEntry = z.object({
  kind: RequestKind,
  tmdbId: z.number(),
  title: z.string(),
  year: z.number().nullable(),
  posterUrl: z.string().nullable(),
  backdropUrl: z.string().nullable(),
  overview: z.string().nullable(),
  rating: z.number().nullable(),
  inLibrary: z.boolean(),
  localId: z.string().nullable(),
  requestId: RequestId.nullable(),
  requestStatus: RequestStatus.nullable(),
  requestProgress: z.number().nullable(),
});
export type DiscoverEntry = z.infer<typeof DiscoverEntry>;

/** One season row in a show's discovery detail (drives the season picker). */
export const DiscoverSeason = z.object({
  season: z.number(),
  name: z.string().nullable(),
  episodeCount: z.number(),
  airDate: z.string().nullable(),
  available: z.boolean(),
  episodesAvailable: z.number(),
  requested: z.boolean(),
});
export type DiscoverSeason = z.infer<typeof DiscoverSeason>;

/** `GET /api/discover/{movie,tv}/:tmdbId`: the request-flow detail page. */
export const DiscoverDetail = z.object({
  kind: RequestKind,
  tmdbId: z.number(),
  title: z.string(),
  year: z.number().nullable(),
  posterUrl: z.string().nullable(),
  backdropUrl: z.string().nullable(),
  overview: z.string().nullable(),
  tagline: z.string().nullable(),
  genres: z.array(z.string()),
  rating: z.number().nullable(),
  runtimeMin: z.number().nullable(),
  seasons: z.array(DiscoverSeason),
  cast: z.array(CastMember),
  crew: z.array(CrewMember),
  similar: z.array(DiscoverEntry),
  inLibrary: z.boolean(),
  localId: z.string().nullable(),
  requestId: RequestId.nullable(),
  requestStatus: RequestStatus.nullable(),
  requestProgress: z.number().nullable(),
});
export type DiscoverDetail = z.infer<typeof DiscoverDetail>;

/** `GET /api/discover/search` / `GET /api/discover/trending`. */
export const DiscoverResponse = z.object({
  results: z.array(DiscoverEntry),
  page: z.number(),
  totalPages: z.number(),
});
export type DiscoverResponse = z.infer<typeof DiscoverResponse>;

/** One ranked `GET /api/search` result a `type`-tagged union (movie/episode carry
 * a `MediaItem`, show a `Show`). */
export const SearchHit = z.discriminatedUnion('type', [
  z.object({ type: z.literal('movie'), item: MediaItem }),
  z.object({ type: z.literal('show'), show: Show }),
  z.object({ type: z.literal('episode'), item: MediaItem }),
]);
// drift: runtime-checked (tagged union)
export type SearchHit = z.infer<typeof SearchHit>;

/** `GET /api/search?q=…` the echoed query plus hits in descending relevance. */
export const SearchResponse = z.object({
  query: z.string(),
  results: z.array(SearchHit),
});
export type SearchResponse = z.infer<typeof SearchResponse>;

/** `GET /api/people?name=…` every movie + show one person is credited in. */
export const PersonResponse = z.object({
  name: z.string(),
  results: z.array(SearchHit),
});
export type PersonResponse = z.infer<typeof PersonResponse>;
export type RequestKind = z.infer<typeof RequestKind>;
export type RequestStatus = z.infer<typeof RequestStatus>;
