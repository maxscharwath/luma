// TMDB discovery for the request flow: search titles the library may not have,
// trending for the empty state, and a title detail (with the season list for
// the picker). Named `discovery` because `src/discover.ts` is the unrelated
// LAN server-discovery module.

import type { DiscoverDetail, DiscoverResponse } from '../types';
import type { RequestContext } from './base';

/** TMDB namespace filter for a discovery search. */
export type DiscoverType = 'movie' | 'tv' | 'all';

export function discoverSearch(
  ctx: RequestContext,
  query: string,
  opts?: { type?: DiscoverType; page?: number },
): Promise<DiscoverResponse> {
  const params = new URLSearchParams({ q: query });
  if (opts?.type && opts.type !== 'all') params.set('type', opts.type);
  if (opts?.page && opts.page > 1) params.set('page', String(opts.page));
  return ctx.json<DiscoverResponse>(`/discover/search?${params.toString()}`);
}

export function discoverTrending(
  ctx: RequestContext,
  opts?: { type?: DiscoverType; page?: number },
): Promise<DiscoverResponse> {
  const params = new URLSearchParams();
  if (opts?.type && opts.type !== 'all') params.set('type', opts.type);
  if (opts?.page && opts.page > 1) params.set('page', String(opts.page));
  const qs = params.toString();
  return ctx.json<DiscoverResponse>(`/discover/trending${qs ? `?${qs}` : ''}`);
}

/** One title's request-flow detail. `kind` follows the route vocabulary
 * (`movie` | `tv`); the response speaks the catalog's (`movie` | `show`). */
export function discoverDetail(
  ctx: RequestContext,
  kind: 'movie' | 'tv',
  tmdbId: number,
): Promise<DiscoverDetail> {
  return ctx.json<DiscoverDetail>(`/discover/${kind}/${tmdbId}`);
}
