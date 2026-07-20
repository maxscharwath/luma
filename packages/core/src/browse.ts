// Shared "browse" helpers used by both clients (web + TV) to sort and
// genre-filter the Films / Séries catalogue screens. Everything works over the
// wire types already held in memory (MediaItem / Show and their view-models), so
// browsing needs no extra request the same approach `people.ts` takes for the
// per-person filmography.

import type { Metadata } from '@kroma/client';

/** The minimal shape every browse helper reads. `MediaItem`, `Show` and the web
 * `MovieView` / `ShowView` view-models all satisfy it. `backdropUrl` is only
 * consulted by {@link genreShowcases} to front genre cards with real art. */
export interface Sortable {
  title: string;
  year: number | null;
  addedAt: string;
  metadata?:
    | (Pick<Metadata, 'rating' | 'releaseDate' | 'genres'> & { backdropUrl?: string | null })
    | null;
}

/** Ways to order a catalogue grid. `added` = most recently added first,
 * `release` = newest release first, `title` = A→Z, `rating` = highest first. */
export type SortMode = 'added' | 'release' | 'title' | 'rating';

/** Sort modes in the order they should appear in a picker. */
export const SORT_MODES: readonly SortMode[] = ['added', 'release', 'title', 'rating'];

/** Narrow an unknown (e.g. a URL search param) to a {@link SortMode}. */
export function isSortMode(v: unknown): v is SortMode {
  return typeof v === 'string' && (SORT_MODES as readonly string[]).includes(v);
}

/** Lexical string compare (ISO timestamps compare correctly this way). */
function cmp(a: string, b: string): number {
  if (a < b) return -1;
  if (a > b) return 1;
  return 0;
}

/** A comparable release instant: the metadata release date (ms since epoch) when
 * present and parseable, else the release year as its Jan 1st, else null. */
function releaseValue(item: Sortable): number | null {
  const iso = item.metadata?.releaseDate;
  if (iso) {
    const ms = Date.parse(iso);
    if (!Number.isNaN(ms)) return ms;
  }
  if (item.year != null) return Date.UTC(item.year, 0, 1);
  return null;
}

// Comparators are made *total* (every one falls through to a deterministic
// tiebreak) so ordering is identical even on engines with an unstable Array.sort
// (legacy webOS Chromium 53 predates guaranteed stable sort). Missing release
// dates / ratings always sort last.
const byTitle = (a: Sortable, b: Sortable): number => a.title.localeCompare(b.title);

function byRelease(a: Sortable, b: Sortable): number {
  const av = releaseValue(a);
  const bv = releaseValue(b);
  if (av == null && bv == null) return byTitle(a, b);
  if (av == null) return 1;
  if (bv == null) return -1;
  return bv - av || byTitle(a, b);
}

function byRating(a: Sortable, b: Sortable): number {
  const av = a.metadata?.rating ?? null;
  const bv = b.metadata?.rating ?? null;
  if (av == null && bv == null) return (b.year ?? 0) - (a.year ?? 0) || byTitle(a, b);
  if (av == null) return 1;
  if (bv == null) return -1;
  return bv - av || (b.year ?? 0) - (a.year ?? 0) || byTitle(a, b);
}

const COMPARATORS: Record<SortMode, (a: Sortable, b: Sortable) => number> = {
  added: (a, b) => cmp(b.addedAt, a.addedAt) || byTitle(a, b),
  release: byRelease,
  title: (a, b) => byTitle(a, b) || cmp(b.addedAt, a.addedAt),
  rating: byRating,
};

/** The comparator for a sort `mode`, for callers that sort a wrapped/mixed list
 * (e.g. a genre grid of `movie`/`show` entries) rather than plain titles. */
export function compareTitles(mode: SortMode): (a: Sortable, b: Sortable) => number {
  return COMPARATORS[mode];
}

/** A new array of `items` ordered by `mode` (never mutates the input). */
export function sortTitles<T extends Sortable>(items: readonly T[], mode: SortMode): T[] {
  return [...items].sort(COMPARATORS[mode]);
}

/** A genre and how many titles carry it. */
export interface GenreCount {
  name: string;
  count: number;
}

/** Every distinct genre across `items` with its title count, most common first
 * (ties broken alphabetically). Genre names come pre-localized from the server. */
export function collectGenres(items: readonly Sortable[]): GenreCount[] {
  const counts = new Map<string, number>();
  for (const it of items) {
    for (const raw of it.metadata?.genres ?? []) {
      const name = raw.trim();
      if (!name) continue;
      counts.set(name, (counts.get(name) ?? 0) + 1);
    }
  }
  return [...counts.entries()]
    .map(([name, count]) => ({ name, count }))
    .sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
}

/** Does `item` carry `genre`? (trim-tolerant exact match). */
export function hasGenre(item: Sortable, genre: string): boolean {
  const want = genre.trim();
  return (item.metadata?.genres ?? []).some((g) => g.trim() === want);
}
