import {
  collectGenres,
  hasGenre,
  type MediaItem,
  type MessageKey,
  posterColors,
  type Show,
  type SortMode,
  sortTitles,
} from '@kroma/core';
import { useT } from '@kroma/ui';
import { Box, Txt, useFocusNav } from '@kroma/ui/kit';
import { useEffect, useMemo, useState } from 'react';
import { useConnection } from '#tv/app/providers/connection';
import { useMyList } from '#tv/app/providers/mylist';
import { useWatched } from '#tv/app/providers/watched';
import { useClient, useNav, useParams } from '#tv/app/router';
import {
  AmbientBackdrop,
  type CatalogEntry as Entry,
  entryBackdrop,
  entryPoster,
} from '#tv/features/catalog/home/AmbientBackdrop';
import { HintBar } from '#tv/features/catalog/home/HintBar';
import { TvTopNav } from '#tv/features/catalog/home/TopNav';
import { type GridCard, TvGrid as PosterGrid } from '#tv/features/catalog/home/TvGrid';
import { BrowseFilters, BrowseHeader } from '#tv/features/catalog/TvBrowseHeader';

/** The section label over the grid, one key per section. */
const LABEL_KEY: Record<'films' | 'series' | 'mylist', MessageKey> = {
  films: 'nav.films',
  series: 'nav.series',
  mylist: 'nav.myList',
};

/** One kind's base list for the active section, before genre filter + sort: the
 * whole catalogue in its own section, nothing in the other one, and the saved
 * titles in Ma liste. */
function sectionList<T extends MediaItem | Show>(
  items: T[],
  own: boolean,
  other: boolean,
  myList: { has: (id: string) => boolean },
): T[] {
  if (own) return items;
  if (other) return [];
  return items.filter((it) => myList.has(it.id));
}

/**
 * Full-screen catalogue browse for one section (Films / Séries / Ma liste), in
 * the content-forward Disney+/Apple TV shape: a full-bleed ambient backdrop of
 * the focused tile (no dedicated hero block), a compact fixed-height header
 * echoing that tile's title + meta, a slim sort/genre chip strip, then a poster
 * grid holding ~75% of the screen. Shares the top nav with Home.
 */
export function TvGrid() {
  const { kind } = useParams('grid');
  const { movies, shows } = useConnection();
  const client = useClient();
  const t = useT();
  const nav = useNav();
  const myList = useMyList();
  const watched = useWatched();
  const isFilms = kind === 'films';
  const isSeries = kind === 'series';
  useFocusNav({ onBack: nav.back, resetKey: kind });

  const [sort, setSort] = useState<SortMode>('added');
  const [genre, setGenre] = useState<string | undefined>(undefined);
  // The id of the grid tile currently holding focus; the header + ambient art
  // follow it (falling back to the first title of the view).
  const [focusId, setFocusId] = useState<string | null>(null);
  // Films / Séries / Ma liste share this component (a top-nav jump swaps the
  // param without remounting), so drop the genre filter (it may not exist in the
  // other section's catalogue) and the focus echo when the section changes.
  // biome-ignore lint/correctness/useExhaustiveDependencies: kind is an intentional re-run key (resets the filter on a section switch), not read inside the effect
  useEffect(() => {
    setGenre(undefined);
    setFocusId(null);
  }, [kind]);

  const baseMovies = useMemo(
    () => sectionList(movies, isFilms, isSeries, myList),
    [isFilms, isSeries, movies, myList],
  );
  const baseShows = useMemo(
    () => sectionList(shows, isSeries, isFilms, myList),
    [isFilms, isSeries, shows, myList],
  );

  const genres = useMemo(
    () => collectGenres([...baseMovies, ...baseShows]),
    [baseMovies, baseShows],
  );

  const entries = useMemo<Entry[]>(() => {
    const keep = (it: MediaItem | Show) => !genre || hasGenre(it, genre);
    return [
      ...sortTitles(baseMovies.filter(keep), sort).map((m): Entry => ({ kind: 'movie', item: m })),
      ...sortTitles(baseShows.filter(keep), sort).map((s): Entry => ({ kind: 'show', item: s })),
    ];
  }, [baseMovies, baseShows, genre, sort]);

  const cards = useMemo<GridCard[]>(
    () =>
      entries.map((e) => ({
        id: e.item.id,
        title: e.item.title,
        poster: entryPoster(client, e),
        colors: posterColors(e.item.id),
        watched: watched.has(e.item.id),
        progress: e.kind === 'show' ? (e.item.progress ?? null) : null,
        onClick: () =>
          e.kind === 'movie' ? nav.go('movie', { item: e.item }) : nav.go('show', { show: e.item }),
        onFocus: () => setFocusId(e.item.id),
      })),
    [entries, client, nav, watched],
  );

  // The entry the header + ambient art echo: the focused tile, else the first
  // title of the current view (also covers a filter change dropping the id).
  const focused = useMemo<Entry | null>(
    () => entries.find((e) => e.item.id === focusId) ?? entries[0] ?? null,
    [entries, focusId],
  );
  const hasItems = baseMovies.length + baseShows.length > 0;
  const empty = kind === 'mylist' && cards.length === 0;

  return (
    <Box fill bg="bg" overflow="hidden" style={{ isolation: 'isolate' }}>
      <AmbientBackdrop
        src={entryBackdrop(client, focused)}
        colors={focused ? posterColors(focused.item.id) : ['#1c1c22', '#0a0a0c']}
      />
      <TvTopNav active={kind} />

      <BrowseHeader
        label={t(LABEL_KEY[kind])}
        count={cards.length}
        hasItems={hasItems}
        focused={focused}
      />

      {hasItems ? (
        <BrowseFilters
          sort={sort}
          onSort={setSort}
          genres={genres}
          genre={genre}
          onGenre={setGenre}
        />
      ) : null}

      {empty ? (
        <Box flex center px={64}>
          <Txt style={EMPTY} color="textDim">
            {t('content.myListEmpty')}
          </Txt>
        </Box>
      ) : (
        <PosterGrid cards={cards} />
      )}

      <HintBar browseKey="content.hintBrowseAll" strength={0.85} />
    </Box>
  );
}

const EMPTY = {
  fontSize: 18,
  fontWeight: '500' as const,
  textAlign: 'center' as const,
  maxWidth: 640,
};
