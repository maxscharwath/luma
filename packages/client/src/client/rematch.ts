// Correcting a wrong TMDB match on one catalog element: list the ranked
// candidates, then pin the right one. Both endpoints need `library.manage`.

import { MatchCandidates } from '../schemas';
import type { RequestContext } from './base';

const JSON_HEADERS = { 'content-type': 'application/json' };

/** Route vocabulary for a rematch: the catalog's split, not TMDB's `tv`. */
export type RematchKind = 'movie' | 'show';

/** Ranked TMDB candidates for one element. `query` overrides the search text
 * when the operator types their own; scores still compare against the title and
 * year parsed from the filename, so the confidence stays honest. */
export async function matchCandidates(
  ctx: RequestContext,
  kind: RematchKind,
  id: string,
  query?: string,
): Promise<MatchCandidates> {
  const qs = query?.trim() ? `?q=${encodeURIComponent(query.trim())}` : '';
  return MatchCandidates.parse(await ctx.json(`/rematch/${kind}/${id}/candidates${qs}`));
}

/** Pin `tmdbId` to this element, or pass `null` to clear the pin and let the
 * server resolve it automatically again. Returns once the re-enrichment is
 * queued; the new art arrives via the usual item/show update event. */
export function setMatch(
  ctx: RequestContext,
  kind: RematchKind,
  id: string,
  tmdbId: number | null,
): Promise<void> {
  return ctx.json<void>(`/rematch/${kind}/${id}`, {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ tmdbId }),
  });
}
