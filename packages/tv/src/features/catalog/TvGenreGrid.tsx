import { compareTitles, hasGenre, posterColors, type SortMode } from '@kroma/core';
import { useT } from '@kroma/ui';
import { useFocusNav } from '@kroma/ui/kit';
import { useEffect, useMemo, useState } from 'react';
import { useConnection } from '#tv/app/providers/connection';
import { useClient, useNav, useParams } from '#tv/app/router';
import {
  AmbientBackdrop,
  type CatalogEntry as Entry,
  entryBackdrop,
  entryPoster,
} from '#tv/features/catalog/home/AmbientBackdrop';
import { TvTopNav } from '#tv/features/catalog/home/TopNav';
import { type GridCard, TvGrid as PosterGrid } from '#tv/features/catalog/home/TvGrid';

// Best-known titles first (rating, then year) the same ranking as the person grid.
const SORT: SortMode = 'rating';

/** Every movie + show in one genre (reached from {@link TvGenres}). Filters the
 * already-loaded catalogue locally, ranked best-rated first, with the browse
 * screens' ambient backdrop following the focused tile. */
export function TvGenreGrid() {
  const { name } = useParams('genre');
  const { movies, shows } = useConnection();
  const client = useClient();
  const t = useT();
  const nav = useNav();
  useFocusNav({ onBack: nav.back, resetKey: name });

  const [focusId, setFocusId] = useState<string | null>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: name is an intentional re-run key (a genre switch clears the focus echo), not read inside the effect
  useEffect(() => setFocusId(null), [name]);

  const entries = useMemo<Entry[]>(() => {
    const tagged: Entry[] = [
      ...movies.filter((m) => hasGenre(m, name)).map((m): Entry => ({ kind: 'movie', item: m })),
      ...shows.filter((s) => hasGenre(s, name)).map((s): Entry => ({ kind: 'show', item: s })),
    ];
    const cmp = compareTitles(SORT);
    return tagged.sort((a, b) => cmp(a.item, b.item));
  }, [movies, shows, name]);

  const cards = useMemo<GridCard[]>(
    () =>
      entries.map((e) => ({
        id: e.item.id,
        title: e.item.title,
        poster: entryPoster(client, e),
        colors: posterColors(e.item.id),
        progress: e.kind === 'show' ? (e.item.progress ?? null) : null,
        onClick: () =>
          e.kind === 'movie' ? nav.go('movie', { item: e.item }) : nav.go('show', { show: e.item }),
        onFocus: () => setFocusId(e.item.id),
      })),
    [entries, client, nav],
  );

  const focused = useMemo<Entry | null>(
    () => entries.find((e) => e.item.id === focusId) ?? entries[0] ?? null,
    [entries, focusId],
  );
  const backdrop = entryBackdrop(client, focused);

  return (
    <div className="fixed inset-0 isolate flex flex-col overflow-hidden bg-bg animate-[tv-fade-in_0.3s_ease]">
      <AmbientBackdrop
        src={backdrop}
        colors={focused ? posterColors(focused.item.id) : ['#1c1c22', '#0a0a0c']}
      />
      <header className="px-16 pb-6 pt-28">
        <div className="mb-2 font-sans text-[13px] font-bold uppercase tracking-[0.22em] text-accent">
          {t('nav.genres')}
        </div>
        <h1 className="m-0 font-display text-[clamp(34px,5.5vh,60px)] font-bold leading-[0.98] tracking-[-0.02em]">
          {name}
        </h1>
        <div className="mt-2 font-sans text-[16px] font-semibold text-muted">
          {t('person.titleCount', { count: cards.length })}
        </div>
      </header>

      {cards.length ? (
        <PosterGrid cards={cards} />
      ) : (
        <div className="flex flex-1 items-center justify-center px-16">
          <p className="max-w-160 text-center font-sans text-[18px] font-medium text-dim">
            {t('genres.empty')}
          </p>
        </div>
      )}

      {/* Persistent nav last in DOM so a poster keeps the initial focus. */}
      <TvTopNav />
    </div>
  );
}
