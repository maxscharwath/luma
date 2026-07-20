import {
  collectGenres,
  formatRuntime,
  hasGenre,
  type MediaItem,
  type MessageKey,
  posterColors,
  qualityBadge,
  qualityBadgeForVideo,
  type Show,
  SORT_MODES,
  type SortMode,
  sortTitles,
} from '@kroma/core';
import { useT } from '@kroma/ui';
import { useEffect, useMemo, useState } from 'react';
import { useConnection } from '#tv/app/providers/connection';
import { useMyList } from '#tv/app/providers/mylist';
import { useWatched } from '#tv/app/providers/watched';
import { useClient, useNav, useParams } from '#tv/app/router';
import { useFocusNav } from '#tv/app/useFocusNav';
import { AmbientBackdrop } from '#tv/features/catalog/home/AmbientBackdrop';
import { TvTopNav } from '#tv/features/catalog/home/TopNav';
import { type GridCard, TvGrid as PosterGrid } from '#tv/features/catalog/home/TvGrid';
import { badgeClasses } from '#tv/shared/TvMedia';

const SORT_LABEL_KEY: Record<SortMode, MessageKey> = {
  added: 'browse.sort.added',
  release: 'browse.sort.release',
  title: 'browse.sort.title',
  rating: 'browse.sort.rating',
};

// Compact filter chip: translucent over the ambient art, amber when active.
// rgba() literal (not a `/opacity` modifier) for the legacy webOS tier.
const CHIP_CLS =
  'shrink-0 cursor-pointer rounded-full border-none bg-[rgba(255,255,255,0.08)] px-3.5 py-1.5 font-sans text-[13px] font-semibold text-muted transition-transform focus:scale-[1.06] aria-[current=true]:bg-accent aria-[current=true]:text-accent-ink';

/** One browse entry a film or a series with the fields the header reads. */
type Entry = { kind: 'movie'; item: MediaItem } | { kind: 'show'; item: Show };

/** Meta line under the focused title: year · runtime|seasons · lead genres. */
function entryLine(e: Entry, seasons: string | null): string {
  const mid = e.kind === 'movie' ? formatRuntime(e.item.durationMs) : seasons;
  const genres = e.item.metadata?.genres?.slice(0, 2) ?? [];
  return [e.item.year ? String(e.item.year) : null, mid, ...genres].filter(Boolean).join(' · ');
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

  // Base lists for the active section, before genre filter + sort.
  const baseMovies = useMemo(() => {
    if (isFilms) return movies;
    if (isSeries) return [];
    return movies.filter((m) => myList.has(m.id));
  }, [isFilms, isSeries, movies, myList]);
  const baseShows = useMemo(() => {
    if (isSeries) return shows;
    if (isFilms) return [];
    return shows.filter((s) => myList.has(s.id));
  }, [isFilms, isSeries, shows, myList]);

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
        poster: e.kind === 'movie' ? client.posterFor(e.item) : client.showPosterFor(e.item),
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
  const backdrop = focused
    ? (client.backdropFor(focused.item) ??
      (focused.kind === 'movie'
        ? client.posterFor(focused.item)
        : client.showPosterFor(focused.item)))
    : null;
  let badge: string | null = null;
  if (focused) {
    badge =
      focused.kind === 'movie'
        ? qualityBadge(focused.item)
        : qualityBadgeForVideo(focused.item.video);
  }

  let label: string;
  if (isFilms) label = t('nav.films');
  else if (isSeries) label = t('nav.series');
  else label = t('nav.myList');
  const hasItems = baseMovies.length + baseShows.length > 0;
  const empty = kind === 'mylist' && cards.length === 0;

  return (
    <div className="fixed inset-0 isolate flex flex-col overflow-hidden bg-bg">
      <AmbientBackdrop
        src={backdrop}
        colors={focused ? posterColors(focused.item.id) : ['#1c1c22', '#0a0a0c']}
      />
      <TvTopNav active={kind} />

      {/* Fixed-height header (justify-end) so the grid never reflows as the
          focus echo swaps titles; one truncated line keeps that guarantee. */}
      <header className="flex h-52 shrink-0 flex-col justify-end px-16">
        <div className="mb-2 font-sans text-[13px] font-bold uppercase tracking-[0.22em] text-accent">
          {label}
          {hasItems ? <span className="text-dim"> · {cards.length}</span> : null}
        </div>
        {focused ? (
          <div key={focused.item.id} className="animate-[tv-fade-in_0.25s_ease]">
            <h1 className="m-0 max-w-240 truncate font-display text-[clamp(30px,4.8vh,46px)] font-bold leading-[1.05] tracking-[-0.02em]">
              {focused.item.title}
            </h1>
            <div className="mt-1.5 flex items-center gap-2.5 font-sans text-[15px] font-semibold text-muted">
              {focused.item.metadata?.rating ? (
                <span className="font-bold text-accent">
                  {focused.item.metadata.rating.toFixed(1)}★
                </span>
              ) : null}
              <span>
                {entryLine(
                  focused,
                  focused.kind === 'show'
                    ? t('content.seasonCount', { count: focused.item.seasonCount })
                    : null,
                )}
              </span>
              {badge ? <span className={badgeClasses(badge)}>{badge}</span> : null}
            </div>
          </div>
        ) : null}
      </header>

      {hasItems ? (
        <div className="scrollbar-none flex shrink-0 items-center gap-2 overflow-x-auto px-16 py-3">
          {SORT_MODES.map((mode) => (
            <button
              key={mode}
              type="button"
              data-focus=""
              aria-current={mode === sort}
              onClick={() => setSort(mode)}
              className={CHIP_CLS}
            >
              {t(SORT_LABEL_KEY[mode])}
            </button>
          ))}
          {genres.length > 0 ? (
            <>
              <span className="mx-1 h-5 w-px shrink-0 bg-[rgba(255,255,255,0.14)]" />
              <button
                type="button"
                data-focus=""
                aria-current={!genre}
                onClick={() => setGenre(undefined)}
                className={CHIP_CLS}
              >
                {t('browse.allGenres')}
              </button>
              {genres.map((g) => (
                <button
                  key={g.name}
                  type="button"
                  data-focus=""
                  aria-current={g.name === genre}
                  onClick={() => setGenre(g.name)}
                  className={CHIP_CLS}
                >
                  {g.name}
                </button>
              ))}
            </>
          ) : null}
        </div>
      ) : null}

      {empty ? (
        <div className="flex flex-1 items-center justify-center px-16">
          <p className="max-w-160 text-center font-sans text-[18px] font-medium text-dim">
            {t('content.myListEmpty')}
          </p>
        </div>
      ) : (
        <PosterGrid cards={cards} />
      )}

      <div className="pointer-events-none absolute inset-x-0 bottom-0 flex justify-center gap-7.5 bg-[linear-gradient(0deg,rgba(10,10,12,0.85),transparent)] p-4 font-sans text-[13px] font-semibold text-dim">
        <span>{t('content.hintBrowseAll')}</span>
        <span>{t('content.hintRows')}</span>
        <span>
          <b className="font-bold text-accent">{t('content.hintOk')}</b> {t('content.hintOpen')}
        </span>
      </div>
    </div>
  );
}
